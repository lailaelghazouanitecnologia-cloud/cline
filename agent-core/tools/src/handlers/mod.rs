mod read_file;
mod shell;
mod write_file;

pub use read_file::ReadFileHandler;
pub use shell::ShellHandler;
pub use write_file::WriteFileHandler;

use crate::registry::ToolRegistry;

pub fn register_defaults(registry: &mut ToolRegistry) {
    registry.register(ReadFileHandler);
    registry.register(WriteFileHandler);
    registry.register(ShellHandler);
}
