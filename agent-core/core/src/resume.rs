#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::persistence::{
    ConversationEntry, SessionPersistence, SessionSnapshot, ToolCallEntry,
};
use agent_common::{AgentError, AgentResult};
use agent_protocol::SessionState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeOptions {
    pub max_messages: Option<usize>,
    pub max_tokens: Option<usize>,
    pub preserve_tool_context: bool,
    pub include_system_messages: bool,
    pub summarize_old_messages: bool,
}

impl Default for ResumeOptions {
    fn default() -> Self {
        Self {
            max_messages: Some(50),
            max_tokens: Some(100000),
            preserve_tool_context: true,
            include_system_messages: true,
            summarize_old_messages: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumedSession {
    pub snapshot: SessionSnapshot,
    pub trimmed_messages: usize,
    pub preserved_context: PreservedContext,
    pub resume_point: ResumePoint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreservedContext {
    pub modified_files: Vec<String>,
    pub pending_operations: Vec<PendingOperation>,
    pub environment_state: HashMap<String, String>,
    pub last_tool_results: Vec<ToolCallEntry>,
}

impl Default for PreservedContext {
    fn default() -> Self {
        Self {
            modified_files: Vec::new(),
            pending_operations: Vec::new(),
            environment_state: HashMap::new(),
            last_tool_results: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingOperation {
    pub operation_type: String,
    pub target: String,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResumePoint {
    LastMessage,
    LastToolCall,
    LastUserMessage,
    Beginning,
    Custom(usize),
}

pub struct SessionResumer {
    persistence: SessionPersistence,
    options: ResumeOptions,
}

impl SessionResumer {
    pub fn new(persistence: SessionPersistence) -> Self {
        Self {
            persistence,
            options: ResumeOptions::default(),
        }
    }

    pub fn with_options(mut self, options: ResumeOptions) -> Self {
        self.options = options;
        self
    }

    pub async fn resume(&self, session_id: &str) -> AgentResult<ResumedSession> {
        let snapshot = self.persistence.load(session_id).await?;
        self.prepare_resume(snapshot).await
    }

    pub async fn resume_latest(&self) -> AgentResult<ResumedSession> {
        let sessions = self.persistence.list_sessions().await?;

        let latest = sessions
            .first()
            .ok_or_else(|| AgentError::not_found("No sessions available"))?;

        self.resume(&latest.session_id).await
    }

    async fn prepare_resume(&self, mut snapshot: SessionSnapshot) -> AgentResult<ResumedSession> {
        let original_count = snapshot.conversation.len();

        let context = self.extract_context(&snapshot);

        snapshot.conversation = self.trim_conversation(snapshot.conversation);

        let trimmed = original_count - snapshot.conversation.len();

        let resume_point = self.determine_resume_point(&snapshot.conversation);

        snapshot.state = SessionState::Idle;
        snapshot.updated_at = current_timestamp();

        Ok(ResumedSession {
            snapshot,
            trimmed_messages: trimmed,
            preserved_context: context,
            resume_point,
        })
    }

    fn trim_conversation(&self, mut messages: Vec<ConversationEntry>) -> Vec<ConversationEntry> {
        if let Some(max) = self.options.max_messages {
            if messages.len() > max {
                let skip = messages.len() - max;
                messages = messages.into_iter().skip(skip).collect();
            }
        }

        if let Some(max_tokens) = self.options.max_tokens {
            let mut total_tokens = 0;
            let mut start_idx = messages.len();

            for (i, msg) in messages.iter().enumerate().rev() {
                total_tokens += msg.token_count;
                if total_tokens > max_tokens {
                    start_idx = i + 1;
                    break;
                }
                start_idx = i;
            }

            if start_idx > 0 {
                messages = messages.into_iter().skip(start_idx).collect();
            }
        }

        messages
    }

    fn extract_context(&self, snapshot: &SessionSnapshot) -> PreservedContext {
        let mut context = PreservedContext::default();

        for entry in snapshot.conversation.iter().rev().take(10) {
            for tool_call in &entry.tool_calls {
                if self.is_file_modifying_tool(&tool_call.tool_name) {
                    if let Some(path) = self.extract_path(&tool_call.arguments) {
                        if !context.modified_files.contains(&path) {
                            context.modified_files.push(path);
                        }
                    }
                }

                if tool_call.success {
                    context.last_tool_results.push(tool_call.clone());
                }
            }
        }

        context.last_tool_results.truncate(5);

        context
    }

    fn is_file_modifying_tool(&self, tool_name: &str) -> bool {
        matches!(
            tool_name,
            "write_file" | "edit_file" | "apply_patch" | "create_file" | "delete_file" | "rename_file"
        )
    }

    fn extract_path(&self, args: &serde_json::Value) -> Option<String> {
        args.get("path")
            .or_else(|| args.get("file"))
            .or_else(|| args.get("file_path"))
            .and_then(|v| v.as_str())
            .map(String::from)
    }

    fn determine_resume_point(&self, messages: &[ConversationEntry]) -> ResumePoint {
        if messages.is_empty() {
            return ResumePoint::Beginning;
        }

        let last = &messages[messages.len() - 1];

        if last.role == "user" {
            return ResumePoint::LastUserMessage;
        }

        if !last.tool_calls.is_empty() {
            return ResumePoint::LastToolCall;
        }

        ResumePoint::LastMessage
    }

    pub async fn can_resume(&self, session_id: &str) -> bool {
        self.persistence.load(session_id).await.is_ok()
    }

    pub async fn list_resumable(&self) -> AgentResult<Vec<ResumableSession>> {
        let summaries = self.persistence.list_sessions().await?;
        let mut resumable = Vec::new();

        for summary in summaries {
            if let Ok(snapshot) = self.persistence.load(&summary.session_id).await {
                resumable.push(ResumableSession {
                    session_id: snapshot.session_id,
                    task_description: snapshot.metadata.task_description,
                    message_count: snapshot.conversation.len(),
                    last_updated: snapshot.updated_at,
                    working_directory: snapshot.working_directory,
                });
            }
        }

        Ok(resumable)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumableSession {
    pub session_id: String,
    pub task_description: Option<String>,
    pub message_count: usize,
    pub last_updated: u64,
    pub working_directory: PathBuf,
}

pub struct ConversationCompactor {
    max_messages: usize,
    preserve_recent: usize,
}

impl ConversationCompactor {
    pub fn new(max_messages: usize) -> Self {
        Self {
            max_messages,
            preserve_recent: 10,
        }
    }

    pub fn with_preserve_recent(mut self, count: usize) -> Self {
        self.preserve_recent = count;
        self
    }

    pub fn compact(&self, mut messages: Vec<ConversationEntry>) -> Vec<ConversationEntry> {
        if messages.len() <= self.max_messages {
            return messages;
        }

        let to_remove = messages.len() - self.max_messages;
        let safe_to_remove = messages.len().saturating_sub(self.preserve_recent);

        let actual_remove = to_remove.min(safe_to_remove);

        if actual_remove > 0 {
            let summary = self.summarize_messages(&messages[..actual_remove]);

            messages = messages.into_iter().skip(actual_remove).collect();

            let summary_entry = ConversationEntry {
                id: format!("summary-{}", current_timestamp()),
                role: "system".to_string(),
                content: summary,
                timestamp: current_timestamp(),
                token_count: 0,
                tool_calls: Vec::new(),
            };

            messages.insert(0, summary_entry);
        }

        messages
    }

    fn summarize_messages(&self, messages: &[ConversationEntry]) -> String {
        let mut summary = String::from("[Previous conversation summary]\n\n");

        let tool_calls: Vec<_> = messages
            .iter()
            .flat_map(|m| m.tool_calls.iter())
            .collect();

        if !tool_calls.is_empty() {
            summary.push_str("Tools used:\n");
            for call in tool_calls.iter().take(10) {
                summary.push_str(&format!("- {} ({})\n", call.tool_name,
                    if call.success { "success" } else { "failed" }));
            }
            summary.push('\n');
        }

        let user_messages: Vec<_> = messages
            .iter()
            .filter(|m| m.role == "user")
            .collect();

        if !user_messages.is_empty() {
            summary.push_str("User requests:\n");
            for msg in user_messages.iter().take(5) {
                let preview = if msg.content.len() > 100 {
                    format!("{}...", &msg.content[..100])
                } else {
                    msg.content.clone()
                };
                summary.push_str(&format!("- {}\n", preview));
            }
        }

        summary
    }
}

impl Default for ConversationCompactor {
    fn default() -> Self {
        Self::new(100)
    }
}

pub struct ContextRestorer {
    workspace: PathBuf,
}

impl ContextRestorer {
    pub fn new(workspace: impl AsRef<Path>) -> Self {
        Self {
            workspace: workspace.as_ref().to_path_buf(),
        }
    }

    pub async fn restore(&self, context: &PreservedContext) -> AgentResult<RestorationReport> {
        let mut report = RestorationReport::default();

        for file in &context.modified_files {
            let path = self.workspace.join(file);
            if path.exists() {
                report.files_verified.push(file.clone());
            } else {
                report.files_missing.push(file.clone());
            }
        }

        for op in &context.pending_operations {
            report.pending_actions.push(format!(
                "{}: {} - {}",
                op.operation_type, op.target, op.description
            ));
        }

        Ok(report)
    }

    pub fn build_context_message(&self, context: &PreservedContext) -> String {
        let mut msg = String::from("[Session resumed with context]\n\n");

        if !context.modified_files.is_empty() {
            msg.push_str("Files modified in previous session:\n");
            for file in &context.modified_files {
                msg.push_str(&format!("- {}\n", file));
            }
            msg.push('\n');
        }

        if !context.last_tool_results.is_empty() {
            msg.push_str("Recent tool results:\n");
            for result in &context.last_tool_results {
                msg.push_str(&format!(
                    "- {}: {}\n",
                    result.tool_name,
                    if result.success { "completed" } else { "failed" }
                ));
            }
        }

        msg
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RestorationReport {
    pub files_verified: Vec<String>,
    pub files_missing: Vec<String>,
    pub pending_actions: Vec<String>,
    pub warnings: Vec<String>,
}

impl RestorationReport {
    pub fn has_issues(&self) -> bool {
        !self.files_missing.is_empty() || !self.warnings.is_empty()
    }

    pub fn summary(&self) -> String {
        format!(
            "{} files verified, {} missing, {} pending actions",
            self.files_verified.len(),
            self.files_missing.len(),
            self.pending_actions.len()
        )
    }
}

fn current_timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
