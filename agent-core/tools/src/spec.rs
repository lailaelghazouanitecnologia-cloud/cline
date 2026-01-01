use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: ToolParameters,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParameters {
    #[serde(rename = "type")]
    pub param_type: String,
    pub properties: Value,
    #[serde(default)]
    pub required: Vec<String>,
}

impl ToolSpec {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters: ToolParameters {
                param_type: String::from("object"),
                properties: Value::Object(serde_json::Map::new()),
                required: Vec::new(),
            },
        }
    }

    pub fn with_parameters(mut self, properties: Value, required: Vec<String>) -> Self {
        self.parameters.properties = properties;
        self.parameters.required = required;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

impl ToolCall {
    pub fn new(id: impl Into<String>, name: impl Into<String>, arguments: Value) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            arguments,
        }
    }

    pub fn get_string(&self, key: &str) -> Option<String> {
        self.arguments
            .get(key)
            .and_then(|v| v.as_str())
            .map(String::from)
    }

    pub fn get_u64(&self, key: &str) -> Option<u64> {
        self.arguments.get(key).and_then(|v| v.as_u64())
    }

    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.arguments.get(key).and_then(|v| v.as_i64())
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.arguments.get(key).and_then(|v| v.as_bool())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub call_id: String,
    pub success: bool,
    pub content: String,
    pub duration_ms: u64,
}

impl ToolOutput {
    pub fn success(call_id: impl Into<String>, content: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            call_id: call_id.into(),
            success: true,
            content: content.into(),
            duration_ms,
        }
    }

    pub fn failure(call_id: impl Into<String>, content: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            call_id: call_id.into(),
            success: false,
            content: content.into(),
            duration_ms,
        }
    }
}
