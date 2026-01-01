use agent_common::{AgentError, AgentResult};
use agent_config::Config;
use agent_protocol::{
    ApprovalRequiredMessage, EventMessage, RiskLevel, ToolCallCompletedMessage,
    ToolCallStartedMessage, ToolResult, TurnCompletedMessage,
};
use agent_tools::{ToolCall, ToolContext, ToolOutput, ToolRouter};
use async_channel::Sender;
use std::sync::Arc;

use crate::turn::{Turn, TurnState};

pub struct Executor {
    config: Arc<Config>,
    router: Arc<ToolRouter>,
    event_sender: Sender<EventMessage>,
}

impl Executor {
    pub fn new(
        config: Arc<Config>,
        router: Arc<ToolRouter>,
        event_sender: Sender<EventMessage>,
    ) -> Self {
        Self {
            config,
            router,
            event_sender,
        }
    }

    pub async fn execute_tool_call(
        &self,
        turn: &mut Turn,
        call: ToolCall,
    ) -> AgentResult<ToolOutput> {
        let requires_approval = self.requires_approval(&call);

        if requires_approval {
            self.send_approval_required(&call).await?;
            turn.set_state(TurnState::WaitingApproval);
            return Err(AgentError::approval_required(&call.name));
        }

        self.execute_tool_call_internal(turn, call).await
    }

    async fn execute_tool_call_internal(
        &self,
        turn: &mut Turn,
        call: ToolCall,
    ) -> AgentResult<ToolOutput> {
        self.send_tool_call_started(&call).await?;

        let context = ToolContext::new(self.config.clone());
        let output = self.router.execute(&context, call.clone()).await?;

        turn.increment_tool_calls();
        self.send_tool_call_completed(&call, &output).await?;

        Ok(output)
    }

    pub async fn complete_turn(&self, turn: &Turn) -> AgentResult<()> {
        let message = EventMessage::TurnCompleted(TurnCompletedMessage {
            turn_id: turn.id(),
            tool_calls_count: turn.tool_calls_count(),
            duration_ms: turn.duration_ms(),
        });

        self.event_sender
            .send(message)
            .await
            .map_err(|_| AgentError::ChannelClosed)
    }

    fn requires_approval(&self, call: &ToolCall) -> bool {
        match self.config.approval_mode {
            agent_protocol::ApprovalMode::Always => true,
            agent_protocol::ApprovalMode::Never => false,
            agent_protocol::ApprovalMode::Dangerous => self.router.is_dangerous(call),
        }
    }

    async fn send_tool_call_started(&self, call: &ToolCall) -> AgentResult<()> {
        let message = EventMessage::ToolCallStarted(ToolCallStartedMessage {
            call_id: call.id.clone(),
            tool_name: call.name.clone(),
            arguments: call.arguments.clone(),
        });

        self.event_sender
            .send(message)
            .await
            .map_err(|_| AgentError::ChannelClosed)
    }

    async fn send_tool_call_completed(&self, call: &ToolCall, output: &ToolOutput) -> AgentResult<()> {
        let result = if output.is_success() {
            ToolResult::success(&output.content)
        } else {
            ToolResult::failure(&output.content)
        };

        let message = EventMessage::ToolCallCompleted(ToolCallCompletedMessage {
            call_id: call.id.clone(),
            tool_name: call.name.clone(),
            result,
            duration_ms: output.duration_ms,
        });

        self.event_sender
            .send(message)
            .await
            .map_err(|_| AgentError::ChannelClosed)
    }

    async fn send_approval_required(&self, call: &ToolCall) -> AgentResult<()> {
        let message = EventMessage::ApprovalRequired(ApprovalRequiredMessage {
            call_id: call.id.clone(),
            tool_name: call.name.clone(),
            arguments: call.arguments.clone(),
            risk_level: RiskLevel::High,
        });

        self.event_sender
            .send(message)
            .await
            .map_err(|_| AgentError::ChannelClosed)
    }
}
