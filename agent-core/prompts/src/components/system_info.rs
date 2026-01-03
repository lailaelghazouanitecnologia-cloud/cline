#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::types::SystemPromptContext;
use crate::variant::PromptVariant;

pub async fn get_system_info(_variant: PromptVariant, context: SystemPromptContext) -> Option<String> {
    let mut lines = vec![
        "SYSTEM INFORMATION".to_string(),
        String::new(),
        format!("Operating System: {}", context.platform),
        format!("Default Shell: {}", get_default_shell(&context.platform)),
        format!("Current Working Directory: {}", context.cwd.display()),
    ];

    if !context.ide.is_empty() {
        lines.push(format!("IDE: {}", context.ide));
    }

    if context.multi_root_enabled && !context.workspace_roots.is_empty() {
        lines.push(String::new());
        lines.push("Workspace Roots:".to_string());
        for root in &context.workspace_roots {
            let vcs = root.vcs.as_deref().unwrap_or("none");
            lines.push(format!("  - {} ({}) [vcs: {}]", root.name, root.path.display(), vcs));
        }
    }

    Some(lines.join("\n"))
}

fn get_default_shell(platform: &str) -> &'static str {
    match platform {
        "windows" => "PowerShell",
        "macos" | "darwin" => "zsh",
        _ => "bash",
    }
}
