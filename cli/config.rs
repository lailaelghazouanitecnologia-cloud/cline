/// Configuration settings for the TUI
/// Modify these values to customize the appearance

pub struct Config;

impl Config {
    // ===========================================
    // DISPLAY SCALING
    // ===========================================
    
    /// Factor to adjust content density based on terminal size
    /// Higher values = more compact, Lower values = more spacious
    /// Recommended: 0.8 - 1.5
    pub const SCALE_FACTOR: f32 = 0.4;
    
    /// Minimum terminal width before applying compact mode
    pub const COMPACT_WIDTH_THRESHOLD: u16 = 80;
    
    /// Minimum terminal height before applying compact mode
    pub const COMPACT_HEIGHT_THRESHOLD: u16 = 24;
    
    // ===========================================
    // CODE BLOCKS
    // ===========================================
    
    /// Maximum lines to show before collapsing code block
    pub const CODE_COLLAPSE_THRESHOLD: usize = 50;
    
    /// Lines to show at start when collapsed
    pub const CODE_PREVIEW_LINES_START: usize = 5;
    
    /// Lines to show at end when collapsed
    pub const CODE_PREVIEW_LINES_END: usize = 3;
    
    /// Show line numbers in code blocks
    pub const CODE_SHOW_LINE_NUMBERS: bool = true;
    
    // ===========================================
    // SCROLLING
    // ===========================================
    
    /// Lines to scroll per key press
    pub const SCROLL_LINES: u16 = 3;
    
    /// Lines to scroll per page up/down
    pub const SCROLL_PAGE_LINES: u16 = 10;
    
    // ===========================================
    // INPUT
    // ===========================================
    
    /// Placeholder text for input field
    pub const INPUT_PLACEHOLDER: &'static str = " Ask AI to do anything";
    
    /// Input border style (true = visible, false = minimal)
    pub const INPUT_SHOW_BORDER: bool = true;
    
    // ===========================================
    // API
    // ===========================================
    
    /// Default model to use
    pub const DEFAULT_MODEL: &'static str = "openai/gpt-oss-20b";
    
    /// Maximum tokens for response
    pub const MAX_TOKENS: u32 = 8192;
    
    /// Temperature for responses
    pub const TEMPERATURE: f32 = 1.0;
}

/// Returns adjusted value based on terminal size and scale factor
pub fn scaled_value(base: u16, term_width: u16, term_height: u16) -> u16 {
    let size_factor = if term_width >= Config::COMPACT_WIDTH_THRESHOLD 
        && term_height >= Config::COMPACT_HEIGHT_THRESHOLD {
        1.0
    } else {
        0.8 // More compact for small terminals
    };
    
    ((base as f32) * Config::SCALE_FACTOR * size_factor) as u16
}

/// Check if terminal is in compact mode
pub fn is_compact_mode(term_width: u16, term_height: u16) -> bool {
    term_width < Config::COMPACT_WIDTH_THRESHOLD || term_height < Config::COMPACT_HEIGHT_THRESHOLD
}
