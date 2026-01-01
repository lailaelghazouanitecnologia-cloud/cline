use agent_common::{AgentError, AgentResult, SubmissionId};
use agent_config::{Config, ConfigLoader};
use agent_protocol::{Event, Operation, SessionId, SessionState, UserMessageOperation};
use agent_tools::handlers::register_defaults;
use agent_tools::ToolRegistry;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::session::Session;

pub struct Agent {
    session: Arc<RwLock<Option<Session>>>,
}

impl Agent {
    pub fn new() -> Self {
        Self {
            session: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn start(&self, config: Config) -> AgentResult<SessionId> {
        let mut registry = ToolRegistry::new();
        register_defaults(&mut registry);

        let session = Session::new(config, registry)?;
        let session_id = session.id();

        session.start().await?;

        let mut guard = self.session.write().await;
        *guard = Some(session);

        Ok(session_id)
    }

    pub async fn start_with_defaults(&self) -> AgentResult<SessionId> {
        let loader = ConfigLoader::from_default_home()?;
        let config = loader.load()?;
        self.start(config).await
    }

    pub async fn submit(&self, operation: Operation) -> AgentResult<SubmissionId> {
        let guard = self.session.read().await;
        let session = guard
            .as_ref()
            .ok_or_else(|| AgentError::invalid_state("none", "active"))?;

        session.submit(operation).await
    }

    pub async fn send_message(&self, content: impl Into<String>) -> AgentResult<SubmissionId> {
        let operation = Operation::UserMessage(UserMessageOperation::text(content));
        self.submit(operation).await
    }

    pub async fn next_event(&self) -> AgentResult<Event> {
        let guard = self.session.read().await;
        let session = guard
            .as_ref()
            .ok_or_else(|| AgentError::invalid_state("none", "active"))?;

        session.next_event().await
    }

    pub async fn state(&self) -> Option<SessionState> {
        let guard = self.session.read().await;
        guard.as_ref().map(|s| s.state())
    }

    pub async fn session_id(&self) -> Option<SessionId> {
        let guard = self.session.read().await;
        guard.as_ref().map(|s| s.id())
    }

    pub async fn shutdown(&self) -> AgentResult<()> {
        let guard = self.session.read().await;
        if let Some(session) = guard.as_ref() {
            session.shutdown().await?;
        }
        drop(guard);

        let mut guard = self.session.write().await;
        *guard = None;

        Ok(())
    }

    pub async fn is_active(&self) -> bool {
        let guard = self.session.read().await;
        guard
            .as_ref()
            .map(|s| s.state().is_active())
            .unwrap_or(false)
    }

    pub async fn interrupt(&self) -> AgentResult<()> {
        self.submit(Operation::Interrupt).await?;
        Ok(())
    }
}

impl Default for Agent {
    fn default() -> Self {
        Self::new()
    }
}
