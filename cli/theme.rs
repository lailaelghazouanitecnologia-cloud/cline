use ratatui::prelude::Color;

/// Atom One Dark inspired theme colors
pub struct Theme;

impl Theme {
    // Background colors
    pub const BG_PRIMARY: Color = Color::Rgb(40, 44, 52);      // #282c34
    pub const BG_SECONDARY: Color = Color::Rgb(33, 37, 43);    // #21252b
    pub const BG_HIGHLIGHT: Color = Color::Rgb(44, 49, 58);    // #2c313a
    
    // Foreground colors
    pub const FG_PRIMARY: Color = Color::Rgb(171, 178, 191);   // #abb2bf
    pub const FG_SECONDARY: Color = Color::Rgb(92, 99, 112);   // #5c6370
    
    // Syntax colors
    pub const KEYWORD: Color = Color::Rgb(198, 120, 221);      // #c678dd (purple)
    pub const STRING: Color = Color::Rgb(152, 195, 121);       // #98c379 (green)
    pub const NUMBER: Color = Color::Rgb(209, 154, 102);       // #d19a66 (orange)
    pub const FUNCTION: Color = Color::Rgb(97, 175, 239);      // #61afef (blue)
    pub const VARIABLE: Color = Color::Rgb(224, 108, 117);     // #e06c75 (red)
    pub const TYPE: Color = Color::Rgb(229, 192, 123);         // #e5c07b (yellow)
    pub const COMMENT: Color = Color::Rgb(92, 99, 112);        // #5c6370 (gray)
    pub const OPERATOR: Color = Color::Rgb(86, 182, 194);      // #56b6c2 (cyan)
    pub const CONSTANT: Color = Color::Rgb(209, 154, 102);     // #d19a66 (orange)
    pub const ATTRIBUTE: Color = Color::Rgb(229, 192, 123);    // #e5c07b (yellow)
    
    // UI colors
    pub const ACCENT: Color = Color::Rgb(97, 175, 239);        // #61afef (blue)
    pub const SUCCESS: Color = Color::Rgb(152, 195, 121);      // #98c379 (green)
    pub const WARNING: Color = Color::Rgb(229, 192, 123);      // #e5c07b (yellow)
    pub const ERROR: Color = Color::Rgb(224, 108, 117);        // #e06c75 (red)
    pub const INFO: Color = Color::Rgb(86, 182, 194);          // #56b6c2 (cyan)
    
    // Code block
    pub const CODE_BG: Color = Color::Rgb(33, 37, 43);         // #21252b
    pub const CODE_BORDER: Color = Color::Rgb(62, 68, 81);     // #3e4451
    pub const LINE_NUMBER: Color = Color::Rgb(76, 82, 99);     // #4c5263
    
    // Diff colors
    pub const DIFF_ADD: Color = Color::Rgb(152, 195, 121);     // #98c379
    pub const DIFF_DEL: Color = Color::Rgb(224, 108, 117);     // #e06c75
    pub const DIFF_CHANGE: Color = Color::Rgb(229, 192, 123);  // #e5c07b
}

/// Maps syntect style to theme color based on scope
pub fn scope_to_color(scope: &str) -> Color {
    let scope_lower = scope.to_lowercase();
    
    // Keywords
    if scope_lower.contains("keyword") || scope_lower.contains("storage") {
        return Theme::KEYWORD;
    }
    
    // Strings
    if scope_lower.contains("string") {
        return Theme::STRING;
    }
    
    // Numbers
    if scope_lower.contains("constant.numeric") || scope_lower.contains("number") {
        return Theme::NUMBER;
    }
    
    // Functions
    if scope_lower.contains("function") || scope_lower.contains("entity.name.function") {
        return Theme::FUNCTION;
    }
    
    // Variables
    if scope_lower.contains("variable") {
        return Theme::VARIABLE;
    }
    
    // Types/Classes
    if scope_lower.contains("entity.name.type") 
        || scope_lower.contains("entity.name.class")
        || scope_lower.contains("support.type") 
    {
        return Theme::TYPE;
    }
    
    // Comments
    if scope_lower.contains("comment") {
        return Theme::COMMENT;
    }
    
    // Operators/Punctuation
    if scope_lower.contains("operator") || scope_lower.contains("punctuation") {
        return Theme::OPERATOR;
    }
    
    // Constants
    if scope_lower.contains("constant") {
        return Theme::CONSTANT;
    }
    
    // Attributes/Decorators
    if scope_lower.contains("attribute") || scope_lower.contains("decorator") {
        return Theme::ATTRIBUTE;
    }
    
    // Default
    Theme::FG_PRIMARY
}
