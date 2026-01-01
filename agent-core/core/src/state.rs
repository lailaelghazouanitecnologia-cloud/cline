use agent_common::{AgentError, AgentResult, IdGenerator};
use agent_protocol::SessionState;
use std::sync::RwLock;

pub struct StateManager {
    current: RwLock<SessionState>,
    id_generator: IdGenerator,
}

impl StateManager {
    pub fn new() -> Self {
        Self {
            current: RwLock::new(SessionState::Idle),
            id_generator: IdGenerator::new(),
        }
    }

    pub fn current(&self) -> SessionState {
        *self.current.read().unwrap()
    }

    pub fn transition(&self, target: SessionState) -> AgentResult<()> {
        let mut current = self.current.write().unwrap();

        if !current.can_transition_to(target) {
            return Err(AgentError::invalid_state(
                format!("{:?}", *current),
                format!("{:?}", target),
            ));
        }

        *current = target;
        Ok(())
    }

    pub fn id_generator(&self) -> &IdGenerator {
        &self.id_generator
    }
}

impl Default for StateManager {
    fn default() -> Self {
        Self::new()
    }
}
