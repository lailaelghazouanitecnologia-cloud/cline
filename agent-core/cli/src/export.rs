#![deny(clippy::all)]

use crate::store::{Session, SessionStore, StoredMessage};
use chrono::{DateTime, Utc};
use std::path::Path;
use tokio::fs;

pub struct ConversationExporter<'a> {
    store: &'a SessionStore,
}

impl<'a> ConversationExporter<'a> {
    pub fn new(store: &'a SessionStore) -> Self {
        Self { store }
    }

    pub fn export_to_markdown(&self, session_id: &str) -> Result<String, String> {
        let session = self
            .store
            .get_session(session_id)
            .map_err(|e| format!("Failed to get session: {}", e))?
            .ok_or_else(|| "Session not found".to_string())?;

        let messages = self
            .store
            .get_messages(session_id)
            .map_err(|e| format!("Failed to get messages: {}", e))?;

        let mut markdown = String::new();

        markdown.push_str(&self.format_header(&session));
        markdown.push_str("\n---\n\n");

        for msg in messages {
            markdown.push_str(&self.format_message(&msg));
            markdown.push_str("\n\n");
        }

        markdown.push_str(&self.format_footer(&session));

        Ok(markdown)
    }

    fn format_header(&self, session: &Session) -> String {
        format!(
            r#"# {}

**Session ID:** `{}`
**Provider:** {}
**Model:** {}
**Created:** {}

"#,
            session.title,
            session.id,
            session.provider,
            session.model,
            Self::format_datetime(&session.created_at),
        )
    }

    fn format_message(&self, msg: &StoredMessage) -> String {
        let role_header = match msg.role.as_str() {
            "user" => "## 👤 User",
            "assistant" => "## 🤖 Assistant",
            "system" => "## ⚙️ System",
            _ => "## Message",
        };

        let mut content = format!("{}\n\n{}", role_header, msg.content);

        if let Some(ref tool_calls) = msg.tool_calls {
            if let Ok(tools) = serde_json::from_str::<Vec<serde_json::Value>>(tool_calls) {
                if !tools.is_empty() {
                    content.push_str("\n\n### Tool Calls\n\n");
                    for tool in tools {
                        if let Some(name) = tool.get("name").and_then(|n| n.as_str()) {
                            content.push_str(&format!("- **{}**\n", name));
                            if let Some(input) = tool.get("input") {
                                content.push_str(&format!(
                                    "  ```json\n  {}\n  ```\n",
                                    serde_json::to_string_pretty(input).unwrap_or_default()
                                ));
                            }
                        }
                    }
                }
            }
        }

        content
    }

    fn format_footer(&self, session: &Session) -> String {
        format!(
            r#"---

*Exported from Cline Agent*
*Session: {}*
*Last updated: {}*
"#,
            session.id,
            Self::format_datetime(&session.updated_at),
        )
    }

    fn format_datetime(dt: &DateTime<Utc>) -> String {
        dt.format("%Y-%m-%d %H:%M:%S UTC").to_string()
    }

    pub async fn save_to_file(&self, session_id: &str, path: &Path) -> Result<(), String> {
        let markdown = self.export_to_markdown(session_id)?;
        fs::write(path, markdown)
            .await
            .map_err(|e| format!("Failed to write file: {}", e))
    }
}

pub fn export_sessions_list(sessions: &[Session]) -> String {
    let mut markdown = String::from("# Session History\n\n");

    if sessions.is_empty() {
        markdown.push_str("*No sessions found.*\n");
        return markdown;
    }

    markdown.push_str("| # | Title | Provider | Model | Created |\n");
    markdown.push_str("|---|-------|----------|-------|----------|\n");

    for (i, session) in sessions.iter().enumerate() {
        let title = if session.title.len() > 40 {
            format!("{}...", &session.title[..40])
        } else {
            session.title.clone()
        };

        markdown.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            i + 1,
            title,
            session.provider,
            session.model,
            session.created_at.format("%Y-%m-%d %H:%M"),
        ));
    }

    markdown
}

pub fn format_usage_summary(
    input_tokens: u64,
    output_tokens: u64,
    total_cost: f64,
    request_count: u32,
) -> String {
    format!(
        r#"## Usage Summary

| Metric | Value |
|--------|-------|
| Input Tokens | {} |
| Output Tokens | {} |
| Total Tokens | {} |
| Total Cost | ${:.4} |
| Requests | {} |
"#,
        format_number(input_tokens),
        format_number(output_tokens),
        format_number(input_tokens + output_tokens),
        total_cost,
        request_count,
    )
}

fn format_number(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(500), "500");
        assert_eq!(format_number(1500), "1.5K");
        assert_eq!(format_number(1_500_000), "1.50M");
    }

    #[test]
    fn test_usage_summary() {
        let summary = format_usage_summary(1000, 500, 0.0123, 5);
        assert!(summary.contains("Input Tokens"));
        assert!(summary.contains("1.0K"));
        assert!(summary.contains("500"));
    }
}
