#![deny(clippy::all)]

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const SPINNER_INTERVAL: Duration = Duration::from_millis(80);

pub struct Spinner {
    message: String,
    running: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Spinner {
    pub fn new(message: &str) -> Self {
        Self {
            message: message.to_string(),
            running: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }

    pub fn start(&mut self) {
        if self.running.load(Ordering::SeqCst) {
            return;
        }

        self.running.store(true, Ordering::SeqCst);
        let running = Arc::clone(&self.running);
        let message = self.message.clone();

        self.handle = Some(std::thread::spawn(move || {
            let mut frame_idx = 0;
            let stdout = io::stdout();

            while running.load(Ordering::SeqCst) {
                let frame = SPINNER_FRAMES[frame_idx % SPINNER_FRAMES.len()];
                {
                    let mut handle = stdout.lock();
                    let _ = write!(handle, "\r\x1b[36m{}\x1b[0m {}", frame, message);
                    let _ = handle.flush();
                }
                frame_idx += 1;
                std::thread::sleep(SPINNER_INTERVAL);
            }

            let mut handle = stdout.lock();
            let _ = write!(handle, "\r\x1b[K");
            let _ = handle.flush();
        }));
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }

    pub fn stop_with_message(&mut self, msg: &str, success: bool) {
        self.stop();
        let icon = if success { "\x1b[32m✓\x1b[0m" } else { "\x1b[31m✗\x1b[0m" };
        println!("{} {}", icon, msg);
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.stop();
    }
}

pub struct ProgressBar {
    total: u64,
    current: u64,
    width: usize,
    message: String,
}

impl ProgressBar {
    pub fn new(total: u64, message: &str) -> Self {
        Self {
            total,
            current: 0,
            width: 40,
            message: message.to_string(),
        }
    }

    pub fn set(&mut self, current: u64) {
        self.current = current.min(self.total);
        self.render();
    }

    pub fn inc(&mut self, delta: u64) {
        self.set(self.current + delta);
    }

    fn render(&self) {
        let percent = if self.total > 0 {
            (self.current as f64 / self.total as f64 * 100.0) as u64
        } else {
            0
        };

        let filled = if self.total > 0 {
            (self.current as f64 / self.total as f64 * self.width as f64) as usize
        } else {
            0
        };

        let empty = self.width - filled;

        print!(
            "\r\x1b[36m{}\x1b[0m [{}{}] {:>3}%",
            self.message,
            "█".repeat(filled),
            "░".repeat(empty),
            percent
        );
        let _ = io::stdout().flush();
    }

    pub fn finish(&self) {
        println!();
    }

    pub fn finish_with_message(&self, msg: &str) {
        println!("\r\x1b[K\x1b[32m✓\x1b[0m {}", msg);
    }
}

pub struct TerminalUI;

impl TerminalUI {
    pub fn header(text: &str) {
        println!("\n\x1b[1;36m{}\x1b[0m\n{}", text, "─".repeat(text.len()));
    }

    pub fn subheader(text: &str) {
        println!("\n\x1b[1m{}\x1b[0m", text);
    }

    pub fn success(text: &str) {
        println!("\x1b[32m✓\x1b[0m {}", text);
    }

    pub fn error(text: &str) {
        eprintln!("\x1b[31m✗\x1b[0m {}", text);
    }

    pub fn warning(text: &str) {
        println!("\x1b[33m⚠\x1b[0m {}", text);
    }

    pub fn info(text: &str) {
        println!("\x1b[34mℹ\x1b[0m {}", text);
    }

    pub fn dim(text: &str) {
        println!("\x1b[2m{}\x1b[0m", text);
    }

    pub fn bold(text: &str) {
        println!("\x1b[1m{}\x1b[0m", text);
    }

    pub fn code_block(code: &str, lang: Option<&str>) {
        if let Some(l) = lang {
            println!("\x1b[2m```{}\x1b[0m", l);
        } else {
            println!("\x1b[2m```\x1b[0m");
        }
        println!("\x1b[33m{}\x1b[0m", code);
        println!("\x1b[2m```\x1b[0m");
    }

    pub fn list_item(text: &str, indent: usize) {
        let prefix = "  ".repeat(indent);
        println!("{}• {}", prefix, text);
    }

    pub fn numbered_item(num: usize, text: &str, indent: usize) {
        let prefix = "  ".repeat(indent);
        println!("{}{:>2}. {}", prefix, num, text);
    }

    pub fn key_value(key: &str, value: &str) {
        println!("\x1b[1m{}:\x1b[0m {}", key, value);
    }

    pub fn table_row(cols: &[&str], widths: &[usize]) {
        let mut line = String::new();
        for (i, col) in cols.iter().enumerate() {
            let width = widths.get(i).copied().unwrap_or(20);
            line.push_str(&format!("{:<width$}", col, width = width));
        }
        println!("{}", line);
    }

    pub fn table_separator(widths: &[usize]) {
        let total: usize = widths.iter().sum();
        println!("{}", "─".repeat(total));
    }

    pub fn clear_line() {
        print!("\r\x1b[K");
        let _ = io::stdout().flush();
    }

    pub fn move_up(lines: usize) {
        print!("\x1b[{}A", lines);
        let _ = io::stdout().flush();
    }

    pub fn divider() {
        println!("\x1b[2m{}\x1b[0m", "─".repeat(60));
    }

    pub fn empty_line() {
        println!();
    }
}

pub struct TokenDisplay;

impl TokenDisplay {
    pub fn format_tokens(input: u64, output: u64) -> String {
        format!(
            "\x1b[2m[{} in / {} out]\x1b[0m",
            Self::format_number(input),
            Self::format_number(output)
        )
    }

    pub fn format_cost(cost: f64) -> String {
        if cost < 0.01 {
            format!("\x1b[32m${:.4}\x1b[0m", cost)
        } else if cost < 1.0 {
            format!("\x1b[33m${:.3}\x1b[0m", cost)
        } else {
            format!("\x1b[31m${:.2}\x1b[0m", cost)
        }
    }

    fn format_number(n: u64) -> String {
        if n >= 1_000_000 {
            format!("{:.1}M", n as f64 / 1_000_000.0)
        } else if n >= 1_000 {
            format!("{:.1}K", n as f64 / 1_000.0)
        } else {
            n.to_string()
        }
    }
}

pub struct ToolCallDisplay;

impl ToolCallDisplay {
    pub fn start(name: &str) {
        print!("\x1b[36m⚡\x1b[0m \x1b[1m{}\x1b[0m ", name);
        let _ = io::stdout().flush();
    }

    pub fn success(output: &str) {
        if output.len() > 100 {
            println!("\x1b[32m✓\x1b[0m {}...", &output[..100]);
        } else {
            println!("\x1b[32m✓\x1b[0m {}", output);
        }
    }

    pub fn error(msg: &str) {
        println!("\x1b[31m✗\x1b[0m {}", msg);
    }

    pub fn pending() {
        println!("\x1b[33m⏳\x1b[0m awaiting approval...");
    }
}

pub struct MessageDisplay;

impl MessageDisplay {
    pub fn user(content: &str) {
        println!("\n\x1b[1;34m You:\x1b[0m {}", content);
    }

    pub fn assistant(content: &str) {
        println!("\n\x1b[1;32m Assistant:\x1b[0m");
        for line in content.lines() {
            println!("  {}", line);
        }
    }

    pub fn system(content: &str) {
        println!("\x1b[2m[system] {}\x1b[0m", content);
    }

    pub fn thinking(content: &str) {
        println!("\x1b[2;3m💭 {}\x1b[0m", content);
    }

    pub fn streaming_char(c: char) {
        print!("{}", c);
        let _ = io::stdout().flush();
    }

    pub fn streaming_text(text: &str) {
        print!("{}", text);
        let _ = io::stdout().flush();
    }

    pub fn streaming_done() {
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_format() {
        assert_eq!(TokenDisplay::format_number(500), "500");
        assert_eq!(TokenDisplay::format_number(1500), "1.5K");
        assert_eq!(TokenDisplay::format_number(1_500_000), "1.5M");
    }

    #[test]
    fn test_progress_bar() {
        let mut pb = ProgressBar::new(100, "Test");
        pb.set(50);
        assert_eq!(pb.current, 50);
        pb.inc(25);
        assert_eq!(pb.current, 75);
    }
}
