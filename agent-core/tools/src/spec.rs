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

    pub fn with_parameter(
        mut self,
        name: &str,
        param_type: &str,
        description: &str,
        required: bool,
    ) -> Self {
        let prop = serde_json::json!({
            "type": param_type,
            "description": description
        });

        if let Value::Object(ref mut map) = self.parameters.properties {
            map.insert(name.to_string(), prop);
        }

        if required {
            self.parameters.required.push(name.to_string());
        }

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

    pub fn get_value(&self, key: &str) -> Option<Value> {
        self.arguments.get(key).cloned()
    }

    pub fn to_json_value(&self) -> Value {
        self.arguments.clone()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    Success,
    Failure,
    Pending,
    Completion,
    PlanResponse,
    Ask,
    ApprovalRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub call_id: String,
    pub status: ToolStatus,
    pub content: String,
    pub duration_ms: u64,
    pub options: Vec<String>,
    pub metadata: Option<serde_json::Value>,
}

impl ToolOutput {
    pub fn new(status: ToolStatus, content: impl Into<String>) -> Self {
        Self {
            call_id: String::new(),
            status,
            content: content.into(),
            duration_ms: 0,
            options: Vec::new(),
            metadata: None,
        }
    }

    pub fn success(content: impl Into<String>) -> Self {
        Self::new(ToolStatus::Success, content)
    }

    pub fn failure(content: impl Into<String>) -> Self {
        Self::new(ToolStatus::Failure, content)
    }

    pub fn pending(content: impl Into<String>) -> Self {
        Self::new(ToolStatus::Pending, content)
    }

    pub fn completion(content: impl Into<String>) -> Self {
        Self::new(ToolStatus::Completion, content)
    }

    pub fn plan_response(content: impl Into<String>, options: Vec<String>) -> Self {
        let mut output = Self::new(ToolStatus::PlanResponse, content);
        output.options = options;
        output
    }

    pub fn ask(content: impl Into<String>) -> Self {
        Self::new(ToolStatus::Ask, content)
    }

    pub fn approval_required(content: impl Into<String>, tool_name: impl Into<String>) -> Self {
        let mut output = Self::new(ToolStatus::ApprovalRequired, content);
        output.metadata = Some(serde_json::json!({ "tool_name": tool_name.into() }));
        output
    }

    pub fn with_call_id(mut self, call_id: impl Into<String>) -> Self {
        self.call_id = call_id.into();
        self
    }

    pub fn with_duration(mut self, duration_ms: u64) -> Self {
        self.duration_ms = duration_ms;
        self
    }

    pub fn with_options(mut self, options: Vec<String>) -> Self {
        self.options = options;
        self
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub fn is_success(&self) -> bool {
        self.status == ToolStatus::Success
    }

    pub fn is_completion(&self) -> bool {
        self.status == ToolStatus::Completion
    }

    pub fn is_pending(&self) -> bool {
        self.status == ToolStatus::Pending
    }

    pub fn requires_response(&self) -> bool {
        matches!(
            self.status,
            ToolStatus::Ask | ToolStatus::PlanResponse | ToolStatus::ApprovalRequired
        )
    }
}
