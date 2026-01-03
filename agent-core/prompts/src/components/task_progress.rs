#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::types::SystemPromptContext;
use crate::variant::PromptVariant;

pub async fn get_task_progress(_variant: PromptVariant, _context: SystemPromptContext) -> Option<String> {
    Some(TASK_PROGRESS_CONTENT.to_string())
}

const TASK_PROGRESS_CONTENT: &str = r#"UPDATING TASK PROGRESS

As you work, you can optionally update task progress using the task_progress parameter in tool calls. This helps track what has been completed and what remains.

# Task Progress Format

The task_progress parameter accepts a markdown checklist that shows completed and remaining tasks:

```
- [x] Completed task 1
- [x] Completed task 2
- [ ] Current task in progress
- [ ] Remaining task 1
- [ ] Remaining task 2
```

# Guidelines

1. Include task_progress when completing significant steps
2. Mark completed tasks with [x] and remaining tasks with [ ]
3. Keep the checklist concise and focused on major milestones
4. Update the checklist as you complete each step
5. The task_progress is optional - use it when it helps track complex tasks

# Example Usage

When using a tool like replace_in_file, you can include task_progress:

[replace_in_file]
[path]src/main.rs[/path]
[diff]
...your changes...
[/diff]
[task_progress]
- [x] Analyzed existing code structure
- [x] Identified bug in main function
- [x] Fixed the null pointer issue
- [ ] Add unit tests
- [ ] Update documentation
[/task_progress]
[/replace_in_file]

This provides visibility into your progress and helps maintain focus on remaining work."#;
