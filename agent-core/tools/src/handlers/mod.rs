mod act_respond;
mod apply_patch;
mod ask_followup;
mod attempt_completion;
mod condense;
mod list_code_definitions;
mod list_files;
mod mcp_tool;
mod plan_respond;
mod read_file;
mod replace_in_file;
mod search_files;
mod shell;
mod web_fetch;
mod web_search;
mod write_file;

pub use act_respond::ActModeRespondHandler;
pub use apply_patch::ApplyPatchHandler;
pub use ask_followup::{
    create_followup_channel, AskFollowupHandler, FollowupAnswer, FollowupQuestion,
};
pub use attempt_completion::{
    create_completion_channel, AttemptCompletionHandler, CompletionAttempt, CompletionDecision,
    CompletionFeedback,
};
pub use condense::{CondenseHandler, NewTaskHandler, SummarizeTaskHandler};
pub use list_code_definitions::ListCodeDefinitionsHandler;
pub use list_files::ListFilesHandler;
pub use mcp_tool::{AccessMcpResourceHandler, ListMcpToolsHandler, UseMcpToolHandler};
pub use plan_respond::PlanModeRespondHandler;
pub use read_file::ReadFileHandler;
pub use replace_in_file::{InsertCodeBlockHandler, ReplaceInFileHandler};
pub use search_files::SearchFilesHandler;
pub use shell::ShellHandler;
pub use web_fetch::WebFetchHandler;
pub use web_search::{SearchOptions, SearchProvider, SearchResult, WebSearchHandler};
pub use write_file::WriteFileHandler;

use crate::registry::ToolRegistry;

pub fn register_defaults(registry: &mut ToolRegistry) {
    registry.register(ReadFileHandler);
    registry.register(WriteFileHandler);
    registry.register(ReplaceInFileHandler);
    registry.register(InsertCodeBlockHandler);
    registry.register(ShellHandler);
    registry.register(ApplyPatchHandler);
    registry.register(SearchFilesHandler);
    registry.register(ListFilesHandler);
    registry.register(ListCodeDefinitionsHandler);
    registry.register(WebFetchHandler::new());
    registry.register(WebSearchHandler::new());
    registry.register(AskFollowupHandler::new());
    registry.register(AttemptCompletionHandler::new());
    registry.register(PlanModeRespondHandler::new());
    registry.register(ActModeRespondHandler::new());
    registry.register(CondenseHandler::new());
    registry.register(NewTaskHandler::new());
    registry.register(SummarizeTaskHandler::new());
}
