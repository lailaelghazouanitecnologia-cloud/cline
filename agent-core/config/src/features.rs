use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Feature {
    ShellTool,
    FileReadTool,
    FileWriteTool,
    WebSearch,
    ParallelTools,
    Streaming,
}

impl Feature {
    pub fn key(&self) -> &'static str {
        match self {
            Self::ShellTool => "shell_tool",
            Self::FileReadTool => "file_read_tool",
            Self::FileWriteTool => "file_write_tool",
            Self::WebSearch => "web_search",
            Self::ParallelTools => "parallel_tools",
            Self::Streaming => "streaming",
        }
    }

    pub fn default_enabled(&self) -> bool {
        match self {
            Self::ShellTool => true,
            Self::FileReadTool => true,
            Self::FileWriteTool => true,
            Self::WebSearch => false,
            Self::ParallelTools => true,
            Self::Streaming => true,
        }
    }

    pub fn all() -> &'static [Feature] {
        &[
            Self::ShellTool,
            Self::FileReadTool,
            Self::FileWriteTool,
            Self::WebSearch,
            Self::ParallelTools,
            Self::Streaming,
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Features {
    enabled: BTreeSet<Feature>,
}

impl Features {
    pub fn new() -> Self {
        let mut enabled = BTreeSet::new();
        for feature in Feature::all() {
            if feature.default_enabled() {
                enabled.insert(*feature);
            }
        }
        Self { enabled }
    }

    pub fn all_enabled() -> Self {
        Self {
            enabled: Feature::all().iter().copied().collect(),
        }
    }

    pub fn none_enabled() -> Self {
        Self {
            enabled: BTreeSet::new(),
        }
    }

    pub fn is_enabled(&self, feature: Feature) -> bool {
        self.enabled.contains(&feature)
    }

    pub fn enable(&mut self, feature: Feature) {
        self.enabled.insert(feature);
    }

    pub fn disable(&mut self, feature: Feature) {
        self.enabled.remove(&feature);
    }

    pub fn set(&mut self, feature: Feature, enabled: bool) {
        if enabled {
            self.enable(feature);
        } else {
            self.disable(feature);
        }
    }

    pub fn enabled_features(&self) -> impl Iterator<Item = &Feature> {
        self.enabled.iter()
    }
}

impl Default for Features {
    fn default() -> Self {
        Self::new()
    }
}
