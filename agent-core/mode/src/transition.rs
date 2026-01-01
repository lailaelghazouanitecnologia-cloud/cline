#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::state::{ModeManager, TaskPhase};
use agent_common::AgentResult;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanOption {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlanResponse {
    SelectOption { option_id: String },
    SwitchToAct,
    ProvideFeedback { feedback: String },
    Cancel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanPrompt {
    pub message: String,
    pub options: Vec<PlanOption>,
    pub allow_feedback: bool,
    pub allow_switch_to_act: bool,
}

impl PlanPrompt {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            options: Vec::new(),
            allow_feedback: true,
            allow_switch_to_act: true,
        }
    }

    pub fn with_option(mut self, id: impl Into<String>, label: impl Into<String>) -> Self {
        self.options.push(PlanOption {
            id: id.into(),
            label: label.into(),
            description: None,
        });
        self
    }

    pub fn with_option_desc(
        mut self,
        id: impl Into<String>,
        label: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        self.options.push(PlanOption {
            id: id.into(),
            label: label.into(),
            description: Some(description.into()),
        });
        self
    }

    pub fn no_feedback(mut self) -> Self {
        self.allow_feedback = false;
        self
    }

    pub fn no_switch(mut self) -> Self {
        self.allow_switch_to_act = false;
        self
    }
}

pub struct ModeTransition {
    manager: ModeManager,
}

impl ModeTransition {
    pub fn new(manager: ModeManager) -> Self {
        Self { manager }
    }

    pub async fn handle_plan_response(&self, response: PlanResponse) -> AgentResult<TransitionResult> {
        match response {
            PlanResponse::SelectOption { option_id } => {
                self.manager.set_awaiting_plan_response(false).await;
                Ok(TransitionResult::OptionSelected(option_id))
            }
            PlanResponse::SwitchToAct => {
                self.manager.switch_to_act().await;
                self.manager.set_awaiting_plan_response(false).await;
                Ok(TransitionResult::SwitchedToAct)
            }
            PlanResponse::ProvideFeedback { feedback } => {
                self.manager.set_awaiting_plan_response(false).await;
                Ok(TransitionResult::FeedbackProvided(feedback))
            }
            PlanResponse::Cancel => {
                self.manager.cancel().await;
                Ok(TransitionResult::Cancelled)
            }
        }
    }

    pub async fn start_planning(&self) -> AgentResult<()> {
        self.manager.set_phase(TaskPhase::Planning).await;
        self.manager.set_awaiting_plan_response(true).await;
        Ok(())
    }

    pub async fn start_executing(&self) -> AgentResult<()> {
        self.manager.set_phase(TaskPhase::Executing).await;
        Ok(())
    }

    pub async fn request_approval(&self) -> AgentResult<()> {
        self.manager.set_phase(TaskPhase::WaitingApproval).await;
        Ok(())
    }

    pub async fn request_input(&self) -> AgentResult<()> {
        self.manager.set_phase(TaskPhase::WaitingInput).await;
        Ok(())
    }

    pub async fn resume_from_approval(&self, approved: bool) -> AgentResult<TransitionResult> {
        if approved {
            self.manager.set_phase(TaskPhase::Executing).await;
            Ok(TransitionResult::ApprovalGranted)
        } else {
            Ok(TransitionResult::ApprovalDenied)
        }
    }

    pub async fn complete_task(&self) -> AgentResult<()> {
        self.manager.complete().await;
        Ok(())
    }

    pub async fn fail_task(&self) -> AgentResult<()> {
        self.manager.fail().await;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum TransitionResult {
    OptionSelected(String),
    SwitchedToAct,
    FeedbackProvided(String),
    ApprovalGranted,
    ApprovalDenied,
    Cancelled,
}
