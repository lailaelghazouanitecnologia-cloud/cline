#![deny(clippy::all)]
#![forbid(unsafe_code)]

mod agent_role;
mod capabilities;
mod editing_files;
mod mcp;
mod objective;
mod rules;
mod system_info;
mod task_progress;
mod tool_use;
mod user_instructions;

pub use agent_role::*;
pub use capabilities::*;
pub use editing_files::*;
pub use mcp::*;
pub use objective::*;
pub use rules::*;
pub use system_info::*;
pub use task_progress::*;
pub use tool_use::*;
pub use user_instructions::*;

use crate::types::{PromptSection, SystemPromptContext};
use crate::variant::PromptVariant;
use std::future::Future;
use std::pin::Pin;

pub type ComponentFuture = Pin<Box<dyn Future<Output = Option<String>> + Send>>;
pub type ComponentFn = fn(&PromptVariant, &SystemPromptContext) -> ComponentFuture;

pub struct ComponentMapping {
    pub id: PromptSection,
    pub func: ComponentFn,
}

pub fn get_all_components() -> Vec<ComponentMapping> {
    vec![
        ComponentMapping {
            id: PromptSection::AgentRole,
            func: |v, c| Box::pin(get_agent_role(v.clone(), c.clone())),
        },
        ComponentMapping {
            id: PromptSection::SystemInfo,
            func: |v, c| Box::pin(get_system_info(v.clone(), c.clone())),
        },
        ComponentMapping {
            id: PromptSection::Mcp,
            func: |v, c| Box::pin(get_mcp_section(v.clone(), c.clone())),
        },
        ComponentMapping {
            id: PromptSection::UserInstructions,
            func: |v, c| Box::pin(get_user_instructions(v.clone(), c.clone())),
        },
        ComponentMapping {
            id: PromptSection::ToolUse,
            func: |v, c| Box::pin(get_tool_use(v.clone(), c.clone())),
        },
        ComponentMapping {
            id: PromptSection::EditingFiles,
            func: |v, c| Box::pin(get_editing_files(v.clone(), c.clone())),
        },
        ComponentMapping {
            id: PromptSection::Capabilities,
            func: |v, c| Box::pin(get_capabilities(v.clone(), c.clone())),
        },
        ComponentMapping {
            id: PromptSection::Rules,
            func: |v, c| Box::pin(get_rules(v.clone(), c.clone())),
        },
        ComponentMapping {
            id: PromptSection::Objective,
            func: |v, c| Box::pin(get_objective(v.clone(), c.clone())),
        },
        ComponentMapping {
            id: PromptSection::TaskProgress,
            func: |v, c| Box::pin(get_task_progress(v.clone(), c.clone())),
        },
    ]
}
