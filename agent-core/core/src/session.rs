use agent_common::{AgentError, AgentResult, SubmissionId, CHANNEL_CAPACITY};
use agent_config::Config;
use agent_exec::Executor;
use agent_protocol::{
    Event, EventMessage, Operation, SessionId, SessionStartedMessage, SessionState,
    StateChangedMessage, Submission,
};
use agent_tools::{ToolRegistry, ToolRouter};
use async_channel::{Receiver, Sender};
use std::sync::Arc;

use crate::state::StateManager;

pub struct Session {
    id: SessionId,
    config: Arc<Config>,
    state: Arc<StateManager>,
    executor: Arc<Executor>,
    submission_sender: Sender<Submission>,
    submission_receiver: Receiver<Submission>,
    event_sender: Sender<EventMessage>,
    event_receiver: Receiver<Event>,
}

impl Session {
    pub fn new(config: Config, registry: ToolRegistry) -> AgentResult<Self> {
        let id = SessionId::generate();
        let config = Arc::new(config);
        let state = Arc::new(StateManager::new());

        let (submission_sender, submission_receiver) = async_channel::bounded(CHANNEL_CAPACITY);
        let (event_message_sender, event_message_receiver) = async_channel::bounded(CHANNEL_CAPACITY);
        let (event_sender, event_receiver) = async_channel::bounded(CHANNEL_CAPACITY);

        let router = Arc::new(ToolRouter::new(Arc::new(registry)));
        let executor = Arc::new(Executor::new(
            config.clone(),
            router,
            event_message_sender.clone(),
        ));

        let session = Self {
            id,
            config,
            state,
            executor,
            submission_sender,
            submission_receiver,
            event_sender: event_message_sender,
            event_receiver,
        };

        session.spawn_event_processor(event_message_receiver, event_sender);

        Ok(session)
    }

    pub fn id(&self) -> SessionId {
        self.id
    }

    pub fn state(&self) -> SessionState {
        self.state.current()
    }

    pub async fn start(&self) -> AgentResult<()> {
        self.transition_state(SessionState::Processing).await?;

        let message = EventMessage::SessionStarted(SessionStartedMessage {
            session_id: self.id.to_string(),
            model_id: self.config.model_id()?.to_string(),
            provider_id: self.config.provider_id.clone(),
        });

        self.event_sender
            .send(message)
            .await
            .map_err(|_| AgentError::ChannelClosed)?;

        self.transition_state(SessionState::Idle).await
    }

    pub async fn submit(&self, operation: Operation) -> AgentResult<SubmissionId> {
        let id = self.state.id_generator().next_submission();
        let submission = Submission::new(id, operation);

        self.submission_sender
            .send(submission)
            .await
            .map_err(|_| AgentError::ChannelClosed)?;

        Ok(id)
    }

    pub async fn next_event(&self) -> AgentResult<Event> {
        self.event_receiver
            .recv()
            .await
            .map_err(|_| AgentError::ChannelClosed)
    }

    pub async fn shutdown(&self) -> AgentResult<()> {
        self.submit(Operation::Shutdown).await?;
        Ok(())
    }

    async fn transition_state(&self, target: SessionState) -> AgentResult<()> {
        let previous = self.state.current();
        self.state.transition(target)?;

        let message = EventMessage::StateChanged(StateChangedMessage {
            previous,
            current: target,
        });

        self.event_sender
            .send(message)
            .await
            .map_err(|_| AgentError::ChannelClosed)
    }

    fn spawn_event_processor(
        &self,
        message_receiver: Receiver<EventMessage>,
        event_sender: Sender<Event>,
    ) {
        let state = self.state.clone();

        tokio::spawn(async move {
            loop {
                let message = match message_receiver.recv().await {
                    Ok(message) => message,
                    Err(_) => break,
                };

                let is_shutdown = matches!(message, EventMessage::ShutdownComplete);
                let id = state.id_generator().next_event();
                let event = Event::new(id, message);

                if event_sender.send(event).await.is_err() {
                    break;
                }

                if is_shutdown {
                    break;
                }
            }
        });
    }
}
