use std::collections::HashMap;
use std::sync::Arc;

use crate::handler::ToolHandler;
use crate::spec::ToolSpec;

pub struct ToolRegistry {
    handlers: HashMap<String, Arc<dyn ToolHandler>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    pub fn register<H: ToolHandler + 'static>(&mut self, handler: H) {
        let spec = handler.spec();
        self.handlers.insert(spec.name.clone(), Arc::new(handler));
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn ToolHandler>> {
        self.handlers.get(name).cloned()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.handlers.contains_key(name)
    }

    pub fn specs(&self) -> Vec<ToolSpec> {
        self.handlers.values().map(|h| h.spec()).collect()
    }

    pub fn names(&self) -> Vec<&String> {
        self.handlers.keys().collect()
    }

    pub fn count(&self) -> usize {
        self.handlers.len()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
