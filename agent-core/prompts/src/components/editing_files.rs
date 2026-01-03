#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::types::SystemPromptContext;
use crate::variant::PromptVariant;

pub async fn get_editing_files(_variant: PromptVariant, _context: SystemPromptContext) -> Option<String> {
    Some(EDITING_FILES_CONTENT.to_string())
}

const EDITING_FILES_CONTENT: &str = r#"EDITING FILES

You have access to two tools for working with files: **write_to_file** and **replace_in_file**. Understanding when to use each tool is crucial for efficient and accurate file modifications.

# write_to_file

## Purpose
- Create a new file, or overwrite the entire contents of an existing file.

## When to Use
- Initial file creation, such as when scaffolding a new project
- Completely rewriting a file's content based on major changes
- When the complexity or number of changes would make replace_in_file unwieldy
- Creating configuration files, documentation, or any file from scratch
- Duplicating or templating files with significant modifications

## Important Considerations
- Using write_to_file requires providing the file's complete final content
- If you only need to make small changes to an existing file, consider using replace_in_file instead to avoid unnecessarily rewriting the entire file
- While write_to_file should not be your default choice, don't hesitate to use it when the situation truly calls for it

# replace_in_file

## Purpose
- Make targeted edits to specific parts of an existing file without affecting the rest of the content.

## When to Use
- Small, localized changes like updating a few lines, function implementations, changing variable names, modifying a section of text, etc.
- Targeted improvements like bug fixes, refactoring a specific function, or updating particular values
- Iterative development and making incremental improvements to code
- Most modifications to existing files

## Advantages
- More efficient for small edits since you don't need to re-supply unchanged file content
- Reduces the chance of errors that could occur when rewriting entire files
- Clearer intent in showing exactly what is being modified

## Important Considerations
- The SEARCH parameter must match the associated file content exactly, character-for-character, including all whitespace and indentation
- If you want to replace multiple sections, use multiple separate SEARCH/REPLACE pairs in a single replace_in_file tool use
- Since the SEARCH parameter must be an exact match, if there are any issues with the tool working correctly, re-read the file to ensure you have the correct content

# Choosing the Right Tool

- **Default to replace_in_file** for most changes to existing files
- **Use write_to_file** when creating new files or when changes are so extensive that replacement-based editing becomes impractical
- **Consider file size and change scope**: for small, targeted changes, replace_in_file is more efficient and less error-prone
- **When in doubt**, prefer replace_in_file for existing files as it maintains more of the original file content and makes your changes clearer"#;
