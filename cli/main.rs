use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    prelude::*,
    widgets::{Paragraph, Wrap, Scrollbar, ScrollbarOrientation, ScrollbarState},
};
use serde::{Deserialize, Serialize};
use std::io::{stdout, BufRead, BufReader, Result};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use dotenvy::dotenv;

mod agent;
mod config;
mod render;
mod theme;

use agent::{AgentBridge, get_model, get_system_prompt};
use config::Config;

// ========== Groq API Types ==========
#[derive(Serialize)]
struct GroqRequest {
    model: String,
    messages: Vec<GroqMessage>,
    temperature: f32,
    max_completion_tokens: u32,
    top_p: f32,
    stream: bool,
}

#[derive(Serialize, Clone)]
struct GroqMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
}

#[derive(Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

// ========== Chat Message ==========
struct ChatMessage {
    content: String,
    is_user: bool,
    rendered: Option<Vec<Line<'static>>>,  // Cached rendered lines
}

impl ChatMessage {
    fn new(content: &str, is_user: bool) -> Self {
        let rendered = if is_user {
            None  // User messages are simple, render inline
        } else {
            Some(render::parse_markdown(content))  // Cache AI response rendering
        };
        Self {
            content: content.to_string(),
            is_user,
            rendered,
        }
    }
}

// ========== Text Input ==========
struct TextInput {
    text: String,
    cursor_pos: usize,
}

impl TextInput {
    fn new() -> Self {
        Self {
            text: String::new(),
            cursor_pos: 0,
        }
    }

    fn insert_char(&mut self, c: char) {
        self.text.insert(self.cursor_pos, c);
        self.cursor_pos += 1;
    }

    fn delete_char(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            self.text.remove(self.cursor_pos);
        }
    }

    fn move_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    fn move_right(&mut self) {
        if self.cursor_pos < self.text.len() {
            self.cursor_pos += 1;
        }
    }

    fn submit(&mut self) -> Option<String> {
        if self.text.trim().is_empty() {
            return None;
        }
        let result = self.text.clone();
        self.text.clear();
        self.cursor_pos = 0;
        Some(result)
    }
}

// ========== Streaming Message ==========
enum StreamMessage {
    Chunk(String),
    Done,
    Error(String),
}

// ========== Groq Client with Streaming ==========
fn call_groq_api_streaming(api_key: &str, conversation: Vec<GroqMessage>, tx: Sender<StreamMessage>) {
    let client = reqwest::blocking::Client::new();
    
    let request = GroqRequest {
        model: "openai/gpt-oss-20b".to_string(),
        messages: conversation,
        temperature: 1.0,
        max_completion_tokens: 8192,
        top_p: 1.0,
        stream: true,
    };
    
    let response = match client
        .post("https://api.groq.com/openai/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            let _ = tx.send(StreamMessage::Error(format!("Request error: {}", e)));
            return;
        }
    };
    
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().unwrap_or_default();
        let _ = tx.send(StreamMessage::Error(format!("API error {}: {}", status, text)));
        return;
    }
    
    let reader = BufReader::new(response);
    
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        
        if line.starts_with("data: ") {
            let data = &line[6..];
            
            if data == "[DONE]" {
                let _ = tx.send(StreamMessage::Done);
                return;
            }
            
            if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                if let Some(choice) = chunk.choices.first() {
                    if let Some(content) = &choice.delta.content {
                        let _ = tx.send(StreamMessage::Chunk(content.clone()));
                    }
                }
            }
        }
    }
    
    let _ = tx.send(StreamMessage::Done);
}

// ========== App State ==========
struct App {
    messages: Vec<ChatMessage>,
    conversation: Vec<(String, String)>,
    input: TextInput,
    is_working: bool,
    work_start: Option<Instant>,
    spinner_frame: usize,
    should_quit: bool,
    api_key: String,
    model: String,
    agent_bridge: Option<AgentBridge>,
    stream_rx: Option<Receiver<StreamMessage>>,
    current_response: String,
    scroll_offset: u16,
    total_lines: u16,
    auto_scroll: bool,
    term_width: u16,
    term_height: u16,
}

impl App {
    fn new() -> Self {
        let api_key = std::env::var("GROQ_API_KEY").unwrap_or_default();
        let model = get_model();

        let (initial_message, agent_bridge) = if api_key.is_empty() {
            ("⚠️ GROQ_API_KEY not set! Set it with: export GROQ_API_KEY=your_key".to_string(), None)
        } else {
            let bridge = AgentBridge::new(&api_key, &model);
            (format!("Connected to {} via agent-core. How can I help?", model), Some(bridge))
        };

        Self {
            messages: vec![ChatMessage::new(&initial_message, false)],
            conversation: vec![("system".to_string(), get_system_prompt())],
            input: TextInput::new(),
            is_working: false,
            work_start: None,
            spinner_frame: 0,
            should_quit: false,
            api_key,
            model,
            agent_bridge,
            stream_rx: None,
            current_response: String::new(),
            scroll_offset: 0,
            total_lines: 0,
            auto_scroll: true,
            term_width: 80,
            term_height: 24,
        }
    }
    
    fn update_term_size(&mut self, width: u16, height: u16) {
        self.term_width = width;
        self.term_height = height;
    }
    
    /// Get scaled value based on terminal size and config scale factor
    fn scaled(&self, base: u16) -> u16 {
        let size_factor = if self.term_width >= Config::COMPACT_WIDTH_THRESHOLD 
            && self.term_height >= Config::COMPACT_HEIGHT_THRESHOLD {
            1.0
        } else {
            0.8
        };
        ((base as f32) * Config::SCALE_FACTOR * size_factor) as u16
    }
    
    fn is_compact(&self) -> bool {
        self.term_width < Config::COMPACT_WIDTH_THRESHOLD 
            || self.term_height < Config::COMPACT_HEIGHT_THRESHOLD
    }

    fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset = self.scroll_offset.saturating_sub(3);
            self.auto_scroll = false;
        }
    }

    fn scroll_down(&mut self, visible_height: u16) {
        let max_scroll = self.total_lines.saturating_sub(visible_height);
        if self.scroll_offset < max_scroll {
            self.scroll_offset = (self.scroll_offset + 3).min(max_scroll);
        }
        if self.scroll_offset >= max_scroll.saturating_sub(3) {
            self.auto_scroll = true;
        }
    }

    fn scroll_to_bottom(&mut self, visible_height: u16) {
        let max_scroll = self.total_lines.saturating_sub(visible_height);
        self.scroll_offset = max_scroll;
    }

    fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        match (code, modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            (KeyCode::Esc, _) => {
                if self.is_working {
                    self.is_working = false;
                    self.work_start = None;
                    self.stream_rx = None;
                    if !self.current_response.is_empty() {
                        self.messages.push(ChatMessage::new(&self.current_response, false));
                        self.conversation.push(("assistant".to_string(), self.current_response.clone()));
                    }
                    self.current_response.clear();
                    self.messages.push(ChatMessage::new("(Interrupted)", false));
                }
            }
            (KeyCode::Enter, _) => {
                if !self.is_working {
                    if let Some(text) = self.input.submit() {
                        self.send_message(text);
                    }
                }
            }
            (KeyCode::Backspace, _) => self.input.delete_char(),
            (KeyCode::Left, _) => self.input.move_left(),
            (KeyCode::Right, _) => self.input.move_right(),
            (KeyCode::Up, _) => self.scroll_up(),
            (KeyCode::Down, _) => self.scroll_down(20),
            (KeyCode::PageUp, _) => {
                self.scroll_offset = self.scroll_offset.saturating_sub(10);
                self.auto_scroll = false;
            }
            (KeyCode::PageDown, _) => {
                self.scroll_offset = self.scroll_offset.saturating_add(10);
                self.auto_scroll = true;
            }
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                self.input.insert_char(c);
            }
            _ => {}
        }
    }

    fn send_message(&mut self, text: String) {
        self.messages.push(ChatMessage::new(&text, true));
        self.auto_scroll = true;

        self.conversation.push(("user".to_string(), text.clone()));

        let Some(ref bridge) = self.agent_bridge else {
            self.messages.push(ChatMessage::new("Error: GROQ_API_KEY not set", false));
            return;
        };

        self.is_working = true;
        self.work_start = Some(Instant::now());
        self.current_response.clear();

        let (tx, rx): (Sender<StreamMessage>, Receiver<StreamMessage>) = mpsc::channel();
        self.stream_rx = Some(rx);

        let api_key = self.api_key.clone();
        let model = self.model.clone();
        let conversation = self.conversation.clone();

        thread::spawn(move || {
            let bridge = AgentBridge::new(&api_key, &model);
            bridge.call_streaming_sync(conversation, tx);
        });
    }

    fn tick(&mut self) {
        self.spinner_frame = self.spinner_frame.wrapping_add(1);
        
        // Process stream messages
        if let Some(rx) = &self.stream_rx {
            loop {
                match rx.try_recv() {
                    Ok(StreamMessage::Chunk(content)) => {
                        self.current_response.push_str(&content);
                    }
                    Ok(StreamMessage::Done) => {
                        if !self.current_response.is_empty() {
                            self.messages.push(ChatMessage::new(&self.current_response, false));
                            self.conversation.push(("assistant".to_string(), self.current_response.clone()));
                        }
                        self.current_response.clear();
                        self.is_working = false;
                        self.work_start = None;
                        self.stream_rx = None;
                        break;
                    }
                    Ok(StreamMessage::Error(error)) => {
                        self.messages.push(ChatMessage::new(&format!("Error: {}", error), false));
                        self.current_response.clear();
                        self.is_working = false;
                        self.work_start = None;
                        self.stream_rx = None;
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.is_working = false;
                        self.stream_rx = None;
                        break;
                    }
                }
            }
        }
    }
}

// ========== UI Rendering ==========
fn ui(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    
    // Update terminal size in app
    app.update_term_size(area.width, area.height);
    
    // Use scaled values based on config - ensure input is always visible
    let input_height = 3_u16;  // Fixed height for input box
    let footer_height = if app.is_compact() { 0 } else { 1 };
    let status_height = if app.is_working { 1 } else { 0 };
    
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),                    // History
            Constraint::Length(status_height),    // Status
            Constraint::Length(input_height),     // Input
            Constraint::Length(footer_height),    // Footer
        ])
        .split(area);

    render_history(frame, app, layout[0]);
    if app.is_working {
        render_status(frame, app, layout[1]);
    }
    render_input(frame, app, layout[2]);
    if !app.is_compact() {
        render_footer(frame, layout[3]);
    }
}

fn render_history(frame: &mut Frame, app: &mut App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();
    
    for msg in &app.messages {
        if msg.is_user {
            // User messages - simple inline render
            lines.push(Line::from(vec![
                Span::styled("❯ ", Style::default().fg(Color::Rgb(180, 140, 60))),
                Span::styled(msg.content.clone(), Style::default().fg(Color::Rgb(200, 170, 100))),
            ]));
            lines.push(Line::from(""));
        } else if let Some(cached) = &msg.rendered {
            // Use cached rendered lines for AI responses
            lines.extend(cached.clone());
            lines.push(Line::from(""));
        }
    }
    
    // Streaming response - must re-render since content is changing
    if !app.current_response.is_empty() {
        // Only render simple text during streaming for performance
        for line in app.current_response.lines() {
            lines.push(Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(Color::Rgb(171, 178, 191)),
            )));
        }
    }
    
    // Update total lines for scrolling
    app.total_lines = lines.len() as u16;
    
    // Auto-scroll to bottom when receiving new content
    let visible_height = area.height.saturating_sub(2);
    if app.auto_scroll && app.total_lines > visible_height {
        app.scroll_offset = app.total_lines.saturating_sub(visible_height);
    }
    
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((app.scroll_offset, 0));
    
    frame.render_widget(paragraph, area);
    
    // Render scrollbar if needed
    if app.total_lines > visible_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .thumb_symbol("█");
        let mut scrollbar_state = ScrollbarState::new(app.total_lines as usize)
            .position(app.scroll_offset as usize);
        frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
    }
}

fn render_status(frame: &mut Frame, app: &App, area: Rect) {
    if !app.is_working {
        return;
    }
    
    let spinners = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let spinner = spinners[app.spinner_frame % spinners.len()];
    
    let elapsed = app.work_start.map(|s| s.elapsed().as_secs()).unwrap_or(0);
    let status_text = format!("{} Working ({}s • Esc to interrupt)", spinner, elapsed);
    
    let status = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Yellow));
    
    frame.render_widget(status, area);
}

fn render_input(frame: &mut Frame, app: &App, area: Rect) {
    let display_text = if app.input.text.is_empty() {
        "Ask AI to do anything".to_string()
    } else {
        app.input.text.clone()
    };
    
    let style = if app.input.text.is_empty() {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };
    
    // Input without border, just the red indicator
    let input_line = Line::from(vec![
        Span::styled("▌ ", Style::default().fg(Color::Rgb(200, 60, 60))),
        Span::styled(display_text, style),
    ]);
    
    let input = Paragraph::new(input_line);
    frame.render_widget(input, area);
}

fn render_footer(frame: &mut Frame, area: Rect) {
    let hints = " ⏎ send  ↑↓ scroll  PgUp/PgDn  Esc interrupt  ⌃C quit";
    let footer = Paragraph::new(hints).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, area);
}

// ========== Main ==========
fn main() -> Result<()> {
    let _ = dotenv();
    
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let mut app = App::new();

    while !app.should_quit {
        terminal.draw(|frame| ui(frame, &mut app))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    app.handle_key(key.code, key.modifiers);
                }
            }
        }
        
        app.tick();
    }

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}
