#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::types::SystemPromptContext;
use crate::variant::PromptVariant;

pub async fn get_capabilities(_variant: PromptVariant, context: SystemPromptContext) -> Option<String> {
    let browser_support = if context.supports_browser {
        ", use the browser"
    } else {
        ""
    };

    let ask_followup = if context.yolo_mode {
        ""
    } else {
        ", and ask follow-up questions"
    };

    let mut content = format!(
        r#"CAPABILITIES

- You have access to tools that let you execute CLI commands on the user's computer, list files, view source code definitions, regex search{browser_support}, read and edit files{ask_followup}. These tools help you effectively accomplish a wide range of tasks, such as writing code, making edits or improvements to existing files, understanding the current state of a project, performing system operations, and much more.
- When the user initially gives you a task, a recursive list of all filepaths in the current working directory ('{}') will be included in environment_details. This provides an overview of the project's file structure, offering key insights into the project from directory/file names (how developers conceptualize and organize their code) and file extensions (the language used). This can also guide decision-making on which files to explore further. If you need to further explore directories such as outside the current working directory, you can use the list_files tool. If you pass 'true' for the recursive parameter, it will list files recursively. Otherwise, it will list files at the top level, which is better suited for generic directories where you don't necessarily need the nested structure, like the Desktop.
- You can use search_files to perform regex searches across files in a specified directory, outputting context-rich results that include surrounding lines. This is particularly useful for understanding code patterns, finding specific implementations, or identifying areas that need refactoring.
- You can use the list_code_definition_names tool to get an overview of source code definitions for all files at the top level of a specified directory. This can be particularly useful when you need to understand the broader context and relationships between certain parts of the code. You may need to call this tool multiple times to understand various parts of the codebase related to the task.
- You can use the execute_command tool to run commands on the user's computer whenever you feel it can help accomplish the user's task. When you need to execute a CLI command, you must provide a clear explanation of what the command does. Prefer to execute complex CLI commands over creating executable scripts, since they are more flexible and easier to run. Interactive commands are supported. Each command you execute is run in a new terminal instance."#,
        context.cwd.display()
    );

    if context.supports_browser {
        content.push_str(
            r#"
- You can use the browser_action tool to interact with websites (including html files and locally running development servers) through a Puppeteer-controlled browser when you feel it is necessary in accomplishing the user's task. This tool is particularly useful for web development tasks as it allows you to launch a browser, navigate to pages, interact with elements through clicks and keyboard input, and capture the results through screenshots and console logs. This tool may be useful at key stages of web development tasks-such as after implementing new features, making substantial changes, when troubleshooting issues, or to verify the result of your work. You can analyze the provided screenshots to ensure correct rendering or identify errors, and review console logs for runtime issues."#,
        );
    }

    if !context.mcp_servers.is_empty() {
        content.push_str(
            "\n- You have access to MCP servers that may provide additional tools and resources. \
            Each server may provide different capabilities that you can use to accomplish tasks more effectively.",
        );
    }

    Some(content)
}
