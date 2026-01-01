#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::types::{HookDefinition, HookEvent};
use std::collections::HashMap;

pub struct HookRegistry {
    hooks: HashMap<HookEvent, Vec<HookDefinition>>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self {
            hooks: HashMap::new(),
        }
    }

    pub fn register(&mut self, hook: HookDefinition) {
        self.hooks
            .entry(hook.event)
            .or_default()
            .push(hook);
    }

    pub fn unregister(&mut self, name: &str) {
        for hooks in self.hooks.values_mut() {
            hooks.retain(|h| h.name != name);
        }
    }

    pub fn get(&self, event: HookEvent) -> Vec<&HookDefinition> {
        self.hooks
            .get(&event)
            .map(|hooks| hooks.iter().filter(|h| h.enabled).collect())
            .unwrap_or_default()
    }

    pub fn get_all(&self) -> Vec<&HookDefinition> {
        self.hooks.values().flatten().collect()
    }

    pub fn clear(&mut self) {
        self.hooks.clear();
    }

    pub fn count(&self) -> usize {
        self.hooks.values().map(|v| v.len()).sum()
    }

    pub fn has_hooks_for(&self, event: HookEvent) -> bool {
        self.hooks
            .get(&event)
            .map(|hooks| hooks.iter().any(|h| h.enabled))
            .unwrap_or(false)
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Vec<HookDefinition>> for HookRegistry {
    fn from(hooks: Vec<HookDefinition>) -> Self {
        let mut registry = Self::new();
        for hook in hooks {
            registry.register(hook);
        }
        registry
    }
}
