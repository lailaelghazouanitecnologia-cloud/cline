#![deny(clippy::all)]
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentMode {
    Plan,
    Act,
}

impl Default for AgentMode {
    fn default() -> Self {
        Self::Act
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskPhase {
    Idle,
    Planning,
    Executing,
    WaitingApproval,
    WaitingInput,
    Completed,
    Failed,
    Cancelled,
}

impl Default for TaskPhase {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug, Clone)]
pub struct ModeState {
    pub mode: AgentMode,
    pub phase: TaskPhase,
    pub is_awaiting_plan_response: bool,
    pub did_switch_mode: bool,
    pub yolo_mode: bool,
    pub strict_plan_mode: bool,
    pub consecutive_mistakes: u32,
    pub max_mistakes: u32,
}

impl Default for ModeState {
    fn default() -> Self {
        Self {
            mode: AgentMode::Act,
            phase: TaskPhase::Idle,
            is_awaiting_plan_response: false,
            did_switch_mode: false,
            yolo_mode: false,
            strict_plan_mode: false,
            consecutive_mistakes: 0,
            max_mistakes: 3,
        }
    }
}

impl ModeState {
    pub fn new(mode: AgentMode) -> Self {
        Self {
            mode,
            ..Default::default()
        }
    }

    pub fn with_yolo(mut self) -> Self {
        self.yolo_mode = true;
        self
    }

    pub fn with_strict_plan(mut self) -> Self {
        self.strict_plan_mode = true;
        self
    }

    pub fn is_planning(&self) -> bool {
        self.mode == AgentMode::Plan || self.phase == TaskPhase::Planning
    }

    pub fn is_executing(&self) -> bool {
        self.mode == AgentMode::Act && self.phase == TaskPhase::Executing
    }

    pub fn can_execute_tools(&self) -> bool {
        self.mode == AgentMode::Act || self.yolo_mode
    }

    pub fn should_auto_approve(&self) -> bool {
        self.yolo_mode
    }

    pub fn increment_mistakes(&mut self) -> bool {
        self.consecutive_mistakes += 1;
        self.consecutive_mistakes >= self.max_mistakes
    }

    pub fn reset_mistakes(&mut self) {
        self.consecutive_mistakes = 0;
    }
}

pub struct ModeManager {
    state: Arc<RwLock<ModeState>>,
}

impl ModeManager {
    pub fn new(initial_mode: AgentMode) -> Self {
        Self {
            state: Arc::new(RwLock::new(ModeState::new(initial_mode))),
        }
    }

    pub async fn get_mode(&self) -> AgentMode {
        self.state.read().await.mode
    }

    pub async fn get_phase(&self) -> TaskPhase {
        self.state.read().await.phase
    }

    pub async fn get_state(&self) -> ModeState {
        self.state.read().await.clone()
    }

    pub async fn set_mode(&self, mode: AgentMode) {
        let mut state = self.state.write().await;
        if state.mode != mode {
            state.did_switch_mode = true;
        }
        state.mode = mode;
    }

    pub async fn set_phase(&self, phase: TaskPhase) {
        let mut state = self.state.write().await;
        state.phase = phase;
    }

    pub async fn switch_to_plan(&self) {
        self.set_mode(AgentMode::Plan).await;
        self.set_phase(TaskPhase::Planning).await;
    }

    pub async fn switch_to_act(&self) {
        self.set_mode(AgentMode::Act).await;
        self.set_phase(TaskPhase::Executing).await;
    }

    pub async fn set_awaiting_plan_response(&self, awaiting: bool) {
        let mut state = self.state.write().await;
        state.is_awaiting_plan_response = awaiting;
    }

    pub async fn enable_yolo(&self) {
        let mut state = self.state.write().await;
        state.yolo_mode = true;
    }

    pub async fn disable_yolo(&self) {
        let mut state = self.state.write().await;
        state.yolo_mode = false;
    }

    pub async fn can_execute_tools(&self) -> bool {
        self.state.read().await.can_execute_tools()
    }

    pub async fn should_auto_approve(&self) -> bool {
        self.state.read().await.should_auto_approve()
    }

    pub async fn record_mistake(&self) -> bool {
        let mut state = self.state.write().await;
        state.increment_mistakes()
    }

    pub async fn reset_mistakes(&self) {
        let mut state = self.state.write().await;
        state.reset_mistakes();
    }

    pub async fn complete(&self) {
        self.set_phase(TaskPhase::Completed).await;
    }

    pub async fn fail(&self) {
        self.set_phase(TaskPhase::Failed).await;
    }

    pub async fn cancel(&self) {
        self.set_phase(TaskPhase::Cancelled).await;
    }
}

impl Default for ModeManager {
    fn default() -> Self {
        Self::new(AgentMode::Act)
    }
}
