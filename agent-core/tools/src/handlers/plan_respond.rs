#![deny(clippy::all)]
#![forbid(unsafe_code)]

use agent_common::AgentResult;
use serde::{Deserialize, Serialize};

use crate::{ToolCall, ToolContext, ToolFuture, ToolHandler, ToolOutput, ToolSpec};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanAnalysis {
    pub summary: String,
    pub steps: Vec<PlanStep>,
    pub options: Vec<PlanOption>,
    pub questions: Vec<String>,
    pub recommended_action: Option<String>,
    pub estimated_complexity: Complexity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub order: u32,
    pub description: String,
    pub tools_needed: Vec<String>,
    pub risks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanOption {
    pub id: String,
    pub label: String,
    pub description: String,
    pub pros: Vec<String>,
    pub cons: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Complexity {
    Low,
    Medium,
    High,
    VeryHigh,
}

pub struct PlanModeRespondHandler;

impl PlanModeRespondHandler {
    pub fn new() -> Self {
        Self
    }

    fn parse_input(&self, args: &serde_json::Value) -> AgentResult<PlanAnalysis> {
        let summary = args
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let steps = args
            .get("steps")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .enumerate()
                    .filter_map(|(i, v)| {
                        Some(PlanStep {
                            order: (i + 1) as u32,
                            description: v.get("description")?.as_str()?.to_string(),
                            tools_needed: v
                                .get("tools_needed")
                                .and_then(|t| t.as_array())
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|s| s.as_str().map(String::from))
                                        .collect()
                                })
                                .unwrap_or_default(),
                            risks: v
                                .get("risks")
                                .and_then(|r| r.as_array())
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|s| s.as_str().map(String::from))
                                        .collect()
                                })
                                .unwrap_or_default(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let options = args
            .get("options")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| {
                        Some(PlanOption {
                            id: v.get("id")?.as_str()?.to_string(),
                            label: v.get("label")?.as_str()?.to_string(),
                            description: v
                                .get("description")
                                .and_then(|d| d.as_str())
                                .unwrap_or("")
                                .to_string(),
                            pros: v
                                .get("pros")
                                .and_then(|p| p.as_array())
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|s| s.as_str().map(String::from))
                                        .collect()
                                })
                                .unwrap_or_default(),
                            cons: v
                                .get("cons")
                                .and_then(|c| c.as_array())
                                .map(|a| {
                                    a.iter()
                                        .filter_map(|s| s.as_str().map(String::from))
                                        .collect()
                                })
                                .unwrap_or_default(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let questions = args
            .get("questions")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let recommended_action = args
            .get("recommended_action")
            .and_then(|v| v.as_str())
            .map(String::from);

        let complexity = args
            .get("complexity")
            .and_then(|v| v.as_str())
            .map(|s| match s {
                "low" => Complexity::Low,
                "medium" => Complexity::Medium,
                "high" => Complexity::High,
                "very_high" => Complexity::VeryHigh,
                _ => Complexity::Medium,
            })
            .unwrap_or(Complexity::Medium);

        Ok(PlanAnalysis {
            summary,
            steps,
            options,
            questions,
            recommended_action,
            estimated_complexity: complexity,
        })
    }

    fn format_response(&self, analysis: &PlanAnalysis) -> String {
        let mut output = String::new();

        output.push_str("## Plan Analysis\n\n");
        output.push_str(&analysis.summary);
        output.push_str("\n\n");

        if !analysis.steps.is_empty() {
            output.push_str("### Proposed Steps\n\n");
            for step in &analysis.steps {
                output.push_str(&format!("{}. {}\n", step.order, step.description));
                if !step.tools_needed.is_empty() {
                    output.push_str(&format!("   Tools: {}\n", step.tools_needed.join(", ")));
                }
                if !step.risks.is_empty() {
                    output.push_str(&format!("   Risks: {}\n", step.risks.join(", ")));
                }
            }
            output.push('\n');
        }

        if !analysis.options.is_empty() {
            output.push_str("### Options\n\n");
            for opt in &analysis.options {
                output.push_str(&format!("**{}**: {}\n", opt.label, opt.description));
                if !opt.pros.is_empty() {
                    output.push_str(&format!("  Pros: {}\n", opt.pros.join(", ")));
                }
                if !opt.cons.is_empty() {
                    output.push_str(&format!("  Cons: {}\n", opt.cons.join(", ")));
                }
            }
            output.push('\n');
        }

        if !analysis.questions.is_empty() {
            output.push_str("### Questions\n\n");
            for q in &analysis.questions {
                output.push_str(&format!("- {}\n", q));
            }
            output.push('\n');
        }

        if let Some(rec) = &analysis.recommended_action {
            output.push_str(&format!("### Recommendation\n\n{}\n", rec));
        }

        output
    }
}

impl Default for PlanModeRespondHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolHandler for PlanModeRespondHandler {
    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "plan_mode_respond",
            "Present analysis and plan to user in Plan Mode. Use this to share your understanding of the task, proposed steps, options, and any questions.",
        )
        .with_parameter("summary", "string", "Brief summary of your analysis", true)
        .with_parameter("steps", "array", "Proposed steps to complete the task", false)
        .with_parameter("options", "array", "Alternative approaches for user to choose", false)
        .with_parameter("questions", "array", "Clarifying questions for the user", false)
        .with_parameter("recommended_action", "string", "Your recommended approach", false)
        .with_parameter("complexity", "string", "Estimated complexity: low, medium, high, very_high", false)
    }

    fn execute(&self, _ctx: &ToolContext, call: ToolCall) -> ToolFuture {
        let args = call.to_json_value();
        let handler = Self::new();

        Box::pin(async move {
            let analysis = handler.parse_input(&args)?;
            let response = handler.format_response(&analysis);
            let option_ids: Vec<String> = analysis.options.iter().map(|o| o.id.clone()).collect();

            Ok(ToolOutput::plan_response(response, option_ids))
        })
    }

    fn is_dangerous(&self, _call: &ToolCall) -> bool {
        false
    }
}
