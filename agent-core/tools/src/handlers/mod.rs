mod apply_patch;
mod ask_followup;
mod attempt_completion;
mod list_code_definitions;
mod list_files;
mod read_file;
mod replace_in_file;
mod search_files;
mod shell;
mod web_fetch;
mod write_file;

pub use apply_patch::ApplyPatchHandler;
pub use ask_followup::AskFollowupHandler;
pub use attempt_completion::AttemptCompletionHandler;
pub use list_code_definitions::ListCodeDefinitionsHandler;
pub use list_files::ListFilesHandler;
pub use read_file::ReadFileHandler;
pub use replace_in_file::{InsertCodeBlockHandler, ReplaceInFileHandler};
pub use search_files::SearchFilesHandler;
pub use shell::ShellHandler;
pub use web_fetch::WebFetchHandler;
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
    registry.register(AskFollowupHandler);
    registry.register(AttemptCompletionHandler);
}
