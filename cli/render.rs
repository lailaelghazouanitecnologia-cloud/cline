use ratatui::{
    prelude::*,
    text::{Line, Span},
};
use syntect::easy::HighlightLines;
use syntect::parsing::SyntaxSet;
use syntect::highlighting::{ThemeSet, Style as SyntectStyle};
use syntect::util::LinesWithEndings;

use crate::theme::Theme;
use crate::config::Config;

/// Lazy static syntax highlighting setup
pub struct Highlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

impl Highlighter {
    pub fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
        }
    }
    
    pub fn highlight_code(&self, code: &str, lang: &str) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();
        
        // Find syntax for the language
        let syntax = self.syntax_set
            .find_syntax_by_token(lang)
            .or_else(|| self.syntax_set.find_syntax_by_extension(lang))
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());
        
        // Use base16-eighties as it's close to Atom dark
        let theme = &self.theme_set.themes["base16-eighties.dark"];
        let mut highlighter = HighlightLines::new(syntax, theme);
        
        for (line_num, line) in LinesWithEndings::from(code).enumerate() {
            let mut spans: Vec<Span<'static>> = Vec::new();
            
            // Line number
            spans.push(Span::styled(
                format!("{:3} │ ", line_num + 1),
                Style::default().fg(Theme::LINE_NUMBER),
            ));
            
            // Highlighted code
            if let Ok(highlighted) = highlighter.highlight_line(line, &self.syntax_set) {
                for (style, text) in highlighted {
                    let color = syntect_to_ratatui_color(style);
                    spans.push(Span::styled(
                        text.trim_end_matches('\n').to_string(),
                        Style::default().fg(color),
                    ));
                }
            } else {
                spans.push(Span::styled(
                    line.trim_end_matches('\n').to_string(),
                    Style::default().fg(Theme::FG_PRIMARY),
                ));
            }
            
            lines.push(Line::from(spans));
        }
        
        lines
    }
}

fn syntect_to_ratatui_color(style: SyntectStyle) -> Color {
    Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b)
}

/// Global highlighter instance
thread_local! {
    static HIGHLIGHTER: Highlighter = Highlighter::new();
}

/// Parses markdown text and returns styled Lines for ratatui
pub fn parse_markdown(text: &str) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut in_code_block = false;
    let mut code_block_content: Vec<String> = Vec::new();
    let mut code_lang = String::new();
    let mut in_table = false;
    let mut table_rows: Vec<String> = Vec::new();
    
    for line in text.lines() {
        let trimmed = line.trim();
        
        // Handle code blocks
        if trimmed.starts_with("```") {
            if in_code_block {
                // End code block - render with syntax highlighting
                let code = code_block_content.join("\n");
                let lang_display = if code_lang.is_empty() { "code" } else { &code_lang };
                let line_count = code_block_content.len();
                let show_collapsed = line_count > Config::CODE_COLLAPSE_THRESHOLD;
                
                // Header with compact badges - always show line count
                lines.push(Line::from(vec![
                    Span::styled("┌ ", Style::default().fg(Theme::CODE_BORDER)),
                    Span::styled(
                        format!("{}", lang_display),
                        Style::default().fg(Color::Black).bg(Color::White),
                    ),
                    Span::styled(" ", Style::default()),
                    Span::styled(
                        format!("{}L", line_count),
                        Style::default().fg(Theme::FG_SECONDARY),
                    ),
                    if show_collapsed {
                        Span::styled(
                            " ▶ ver",
                            Style::default().fg(Color::White).bg(Color::Rgb(160, 50, 50)),
                        )
                    } else {
                        Span::styled("", Style::default())
                    },
                    Span::styled(
                        " ─────────────────────────────────────",
                        Style::default().fg(Theme::CODE_BORDER),
                    ),
                ]));
                
                if show_collapsed {
                    // Show preview: first N lines
                    let preview_code = code_block_content
                        .iter()
                        .take(Config::CODE_PREVIEW_LINES_START)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("\n");
                    
                    HIGHLIGHTER.with(|h| {
                        let highlighted = h.highlight_code(&preview_code, &code_lang);
                        lines.extend(highlighted);
                    });
                    
                    // Collapsed indicator
                    let hidden = line_count - Config::CODE_PREVIEW_LINES_START - Config::CODE_PREVIEW_LINES_END;
                    lines.push(Line::from(Span::styled(
                        format!("    ··· {} líneas ocultas ···", hidden),
                        Style::default().fg(Theme::FG_SECONDARY).add_modifier(Modifier::ITALIC),
                    )));
                    
                    // Show last N lines
                    let end_code = code_block_content
                        .iter()
                        .skip(line_count - Config::CODE_PREVIEW_LINES_END)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("\n");
                    
                    HIGHLIGHTER.with(|h| {
                        let highlighted = h.highlight_code(&end_code, &code_lang);
                        // Adjust line numbers for end preview
                        for (i, mut line) in highlighted.into_iter().enumerate() {
                            if let Some(span) = line.spans.get_mut(0) {
                                let real_line_num = line_count - Config::CODE_PREVIEW_LINES_END + i + 1;
                                *span = Span::styled(
                                    format!("{:3} │ ", real_line_num),
                                    Style::default().fg(Theme::LINE_NUMBER),
                                );
                            }
                            lines.push(line);
                        }
                    });
                } else {
                    // Show full code
                    HIGHLIGHTER.with(|h| {
                        let highlighted = h.highlight_code(&code, &code_lang);
                        lines.extend(highlighted);
                    });
                }
                
                lines.push(Line::from(Span::styled(
                    "└─────────────────────────────────────────────",
                    Style::default().fg(Theme::CODE_BORDER),
                )));
                
                code_block_content.clear();
                code_lang.clear();
                in_code_block = false;
            } else {
                // Start code block - capture language
                code_lang = trimmed[3..].trim().to_string();
                // Map common aliases
                if code_lang == "ts" {
                    code_lang = "typescript".to_string();
                } else if code_lang == "js" {
                    code_lang = "javascript".to_string();
                } else if code_lang == "py" {
                    code_lang = "python".to_string();
                } else if code_lang == "rb" {
                    code_lang = "ruby".to_string();
                } else if code_lang == "sh" || code_lang == "bash" {
                    code_lang = "shell".to_string();
                }
                in_code_block = true;
            }
            continue;
        }
        
        if in_code_block {
            code_block_content.push(line.to_string());
            continue;
        }
        
        // Handle tables
        if trimmed.contains('|') && !trimmed.is_empty() {
            if !in_table {
                in_table = true;
            }
            table_rows.push(trimmed.to_string());
            continue;
        } else if in_table {
            lines.extend(render_table(&table_rows));
            table_rows.clear();
            in_table = false;
        }
        
        // Parse regular lines
        if trimmed.is_empty() {
            lines.push(Line::from(""));
        } else {
            lines.push(parse_line(line));
        }
    }
    
    // Handle unclosed code block
    if in_code_block {
        let code = code_block_content.join("\n");
        lines.push(Line::from(Span::styled(
            format!("┌─ {} ", if code_lang.is_empty() { "code" } else { &code_lang }),
            Style::default().fg(Theme::CODE_BORDER),
        )));
        
        HIGHLIGHTER.with(|h| {
            let highlighted = h.highlight_code(&code, &code_lang);
            lines.extend(highlighted);
        });
        
        lines.push(Line::from(Span::styled(
            "└────────────────────────────────────────",
            Style::default().fg(Theme::CODE_BORDER),
        )));
    }
    
    // Handle unclosed table
    if in_table && !table_rows.is_empty() {
        lines.extend(render_table(&table_rows));
    }
    
    lines
}

fn render_table(rows: &[String]) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let border_style = Style::default().fg(Theme::CODE_BORDER);
    let header_style = Style::default().fg(Theme::ACCENT).add_modifier(Modifier::BOLD);
    let cell_style = Style::default().fg(Theme::FG_PRIMARY);
    
    for (i, row) in rows.iter().enumerate() {
        if row.contains("---") || row.contains(":-") || row.contains("-:") {
            lines.push(Line::from(Span::styled(
                "├───────────────────────────────────────────",
                border_style,
            )));
            continue;
        }
        
        let cells: Vec<&str> = row.split('|')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        
        if cells.is_empty() {
            continue;
        }
        
        let mut spans: Vec<Span> = vec![Span::styled("│ ", border_style)];
        
        for (j, cell) in cells.iter().enumerate() {
            let style = if i == 0 { header_style } else { cell_style };
            spans.push(Span::styled(cell.to_string(), style));
            if j < cells.len() - 1 {
                spans.push(Span::styled(" │ ", border_style));
            }
        }
        spans.push(Span::styled(" │", border_style));
        
        lines.push(Line::from(spans));
    }
    
    lines
}

fn parse_line(line: &str) -> Line<'static> {
    let trimmed = line.trim();
    
    // Horizontal rule
    if trimmed == "---" || trimmed == "***" || trimmed == "___" {
        return Line::from(Span::styled(
            "────────────────────────────────────────────────",
            Style::default().fg(Theme::CODE_BORDER),
        ));
    }
    
    // Headers with gradient effect - dark red to lighter, expanding bars
    if trimmed.starts_with("#### ") {
        let title = &trimmed[5..];
        return Line::from(vec![
            Span::styled("▌", Style::default().fg(Color::Rgb(80, 20, 20))),
            Span::styled("▐", Style::default().fg(Color::Rgb(100, 35, 35))),
            Span::styled(" ", Style::default().bg(Color::Rgb(120, 50, 50))),
            Span::styled(
                format!(" {} ", title),
                Style::default().fg(Color::White).bg(Color::Rgb(140, 65, 65)),
            ),
        ]);
    }
    if trimmed.starts_with("### ") {
        let title = &trimmed[4..];
        return Line::from(vec![
            Span::styled("█", Style::default().fg(Color::Rgb(60, 15, 15))),
            Span::styled("▓", Style::default().fg(Color::Rgb(90, 30, 30))),
            Span::styled("▒", Style::default().fg(Color::Rgb(120, 45, 45))),
            Span::styled(" ", Style::default().bg(Color::Rgb(150, 60, 60))),
            Span::styled(
                format!(" {} ", title),
                Style::default().fg(Color::White).bg(Color::Rgb(170, 75, 75)),
            ),
        ]);
    }
    if trimmed.starts_with("## ") {
        let title = &trimmed[3..];
        return Line::from(vec![
            Span::styled("██", Style::default().fg(Color::Rgb(50, 10, 10))),
            Span::styled("▓▓", Style::default().fg(Color::Rgb(80, 25, 25))),
            Span::styled("▒▒", Style::default().fg(Color::Rgb(110, 40, 40))),
            Span::styled("░░", Style::default().fg(Color::Rgb(140, 55, 55))),
            Span::styled(
                format!("  {}  ", title),
                Style::default().fg(Color::White).bg(Color::Rgb(180, 70, 70)).add_modifier(Modifier::BOLD),
            ),
        ]);
    }
    if trimmed.starts_with("# ") {
        let title = &trimmed[2..];
        return Line::from(vec![
            Span::styled("███", Style::default().fg(Color::Rgb(40, 5, 5))),
            Span::styled("▓▓▓", Style::default().fg(Color::Rgb(70, 20, 20))),
            Span::styled("▒▒▒", Style::default().fg(Color::Rgb(100, 35, 35))),
            Span::styled("░░░", Style::default().fg(Color::Rgb(130, 50, 50))),
            Span::styled(
                format!("  {}  ", title),
                Style::default().fg(Color::White).bg(Color::Rgb(200, 80, 80)).add_modifier(Modifier::BOLD),
            ),
        ]);
    }
    
    // Blockquotes
    if trimmed.starts_with("> ") {
        let content = &trimmed[2..];
        return Line::from(vec![
            Span::styled("┃ ", Style::default().fg(Theme::FG_SECONDARY)),
            Span::styled(content.to_string(), Style::default().fg(Theme::FG_SECONDARY).add_modifier(Modifier::ITALIC)),
        ]);
    }
    
    // Bullet lists
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
        let content = &trimmed[2..];
        let mut spans = vec![Span::styled("  • ", Style::default().fg(Theme::ACCENT))];
        spans.extend(parse_inline(content));
        return Line::from(spans);
    }
    
    // Checkbox lists
    if trimmed.starts_with("- [ ] ") {
        let content = &trimmed[6..];
        let mut spans = vec![Span::styled("  ☐ ", Style::default().fg(Theme::FG_SECONDARY))];
        spans.extend(parse_inline(content));
        return Line::from(spans);
    }
    if trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ") {
        let content = &trimmed[6..];
        let mut spans = vec![Span::styled("  ☑ ", Style::default().fg(Theme::SUCCESS))];
        spans.extend(parse_inline(content));
        return Line::from(spans);
    }
    
    // Numbered lists
    if let Some(pos) = trimmed.find(". ") {
        if pos <= 3 && trimmed[..pos].chars().all(|c| c.is_ascii_digit()) {
            let content = &trimmed[pos + 2..];
            let num = &trimmed[..pos];
            let mut spans = vec![Span::styled(
                format!("  {}. ", num),
                Style::default().fg(Theme::ACCENT),
            )];
            spans.extend(parse_inline(content));
            return Line::from(spans);
        }
    }
    
    // Regular text
    Line::from(parse_inline(trimmed))
}

fn parse_inline(text: &str) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut chars = text.chars().peekable();
    let mut current = String::new();
    
    while let Some(c) = chars.next() {
        match c {
            '`' => {
                if !current.is_empty() {
                    spans.push(Span::styled(current.clone(), Style::default().fg(Theme::FG_PRIMARY)));
                    current.clear();
                }
                let mut code = String::new();
                while let Some(&next) = chars.peek() {
                    if next == '`' {
                        chars.next();
                        break;
                    }
                    code.push(chars.next().unwrap());
                }
                spans.push(Span::styled(
                    format!(" {} ", code),
                    Style::default().fg(Theme::STRING).bg(Theme::CODE_BG),
                ));
            }
            '*' => {
                if chars.peek() == Some(&'*') {
                    chars.next();
                    if !current.is_empty() {
                        spans.push(Span::styled(current.clone(), Style::default().fg(Theme::FG_PRIMARY)));
                        current.clear();
                    }
                    let mut bold_text = String::new();
                    while let Some(next) = chars.next() {
                        if next == '*' && chars.peek() == Some(&'*') {
                            chars.next();
                            break;
                        }
                        bold_text.push(next);
                    }
                    spans.push(Span::styled(
                        bold_text,
                        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                    ));
                } else {
                    if !current.is_empty() {
                        spans.push(Span::styled(current.clone(), Style::default().fg(Theme::FG_PRIMARY)));
                        current.clear();
                    }
                    let mut italic_text = String::new();
                    while let Some(next) = chars.next() {
                        if next == '*' {
                            break;
                        }
                        italic_text.push(next);
                    }
                    spans.push(Span::styled(
                        italic_text,
                        Style::default().fg(Theme::FG_PRIMARY).add_modifier(Modifier::ITALIC),
                    ));
                }
            }
            '[' => {
                if !current.is_empty() {
                    spans.push(Span::styled(current.clone(), Style::default().fg(Theme::FG_PRIMARY)));
                    current.clear();
                }
                let mut link_text = String::new();
                while let Some(next) = chars.next() {
                    if next == ']' {
                        break;
                    }
                    link_text.push(next);
                }
                if chars.peek() == Some(&'(') {
                    chars.next();
                    while let Some(next) = chars.next() {
                        if next == ')' {
                            break;
                        }
                    }
                }
                spans.push(Span::styled(
                    link_text,
                    Style::default().fg(Theme::FUNCTION).add_modifier(Modifier::UNDERLINED),
                ));
            }
            _ => {
                current.push(c);
            }
        }
    }
    
    if !current.is_empty() {
        spans.push(Span::styled(current, Style::default().fg(Theme::FG_PRIMARY)));
    }
    
    if spans.is_empty() {
        spans.push(Span::raw(""));
    }
    
    spans
}
