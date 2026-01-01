mod act_respond;
mod apply_patch;
mod ask_followup;
mod attempt_completion;
mod browser;
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
pub use ask_followup::AskFollowupHandler;
pub use attempt_completion::AttemptCompletionHandler;
pub use browser::BrowserHandler;
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
    registry.register(BrowserHandler::new());
    registry.register(AskFollowupHandler);
    registry.register(AttemptCompletionHandler);
    registry.register(PlanModeRespondHandler::new());
    registry.register(ActModeRespondHandler::new());
    registry.register(CondenseHandler::new());
    registry.register(NewTaskHandler::new());
    registry.register(SummarizeTaskHandler::new());
}
