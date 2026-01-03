#![deny(clippy::all)]
#![forbid(unsafe_code)]

use agent_client::{AgentMode, ModelVariant, PromptPresets};
use agent_common::AgentResult;
use agent_focus::{FocusChain, FocusItem, FocusStatus};
use agent_mode::{ModeManager, ModeTransition};
use agent_tools::{ApprovalManager, ApprovalPolicy, ApprovalRequest, ApprovalResponse};
use agent_tools::handlers::{
    create_completion_channel, create_followup_channel, AskFollowupHandler,
    AttemptCompletionHandler, CompletionAttempt, CompletionFeedback, FollowupAnswer,
    FollowupQuestion,
};
use agent_tools::ToolRegistry;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, oneshot, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CoordinatorEvent {
    SessionStarted { session_id: String },
    ModeChanged { mode: String, phase: String },
    TaskStarted { task_id: String, description: String },
    TaskCompleted { task_id: String },
    TaskFailed { task_id: String, error: String },
    ApprovalRequired { request_id: String, tool_name: String },
    ApprovalReceived { request_id: String, approved: bool },
    FollowupRequired { question: String },
    CompletionAttempted { result: String },
    SessionEnded,
}

pub struct Coordinator {
    session_id: String,
    mode_manager: Arc<ModeManager>,
    mode_transition: Arc<ModeTransition>,
    focus_chain: FocusChain,
    tool_registry: Arc<RwLock<ToolRegistry>>,
    approval_manager: Arc<ApprovalManager>,
    approval_policy: Arc<ApprovalPolicy>,
    event_tx: broadcast::Sender<CoordinatorEvent>,
    id_counter: AtomicU64,
    config: CoordinatorConfig,
}

#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    pub initial_mode: AgentMode,
    pub yolo_mode: bool,
    pub strict_plan_mode: bool,
    pub max_concurrent_tools: usize,
    pub tool_timeout_secs: u64,
    pub auto_approve_read_only: bool,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            initial_mode: AgentMode::Act,
            yolo_mode: false,
            strict_plan_mode: false,
            max_concurrent_tools: 1,
            tool_timeout_secs: 300,
            auto_approve_read_only: true,
        }
    }
}

impl Coordinator {
    pub fn new(config: CoordinatorConfig) -> (Self, CoordinatorChannels) {
        let (event_tx, _) = broadcast::channel(100);
        let (approval_manager, approval_rx) = ApprovalManager::new();
        let (followup_tx, followup_rx) = create_followup_channel();
        let (completion_tx, completion_rx) = create_completion_channel();

        let mode = match config.initial_mode {
            AgentMode::Act => agent_mode::AgentMode::Act,
            AgentMode::Plan => agent_mode::AgentMode::Plan,
            _ => agent_mode::AgentMode::Act,
        };

        let mode_manager = Arc::new(ModeManager::new(mode));
        let mode_transition = Arc::new(ModeTransition::new(ModeManager::new(mode)));

        let mut registry = ToolRegistry::new();
        agent_tools::handlers::register_defaults(&mut registry);

        registry.register(AskFollowupHandler::with_channel(followup_tx));
        registry.register(AttemptCompletionHandler::with_channel(completion_tx));

        let session_id = format!("session-{}", uuid::Uuid::new_v4());

        let coordinator = Self {
            session_id: session_id.clone(),
            mode_manager,
            mode_transition,
            focus_chain: FocusChain::new(),
            tool_registry: Arc::new(RwLock::new(registry)),
            approval_manager: Arc::new(approval_manager),
            approval_policy: Arc::new(ApprovalPolicy::default()),
            event_tx,
            id_counter: AtomicU64::new(0),
            config,
        };

        let channels = CoordinatorChannels {
            approval_rx,
            followup_rx,
            completion_rx,
        };

        (coordinator, channels)
    }

    fn next_id(&self, prefix: &str) -> String {
        let n = self.id_counter.fetch_add(1, Ordering::SeqCst);
        format!("{}-{}", prefix, n)
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn subscribe(&self) -> broadcast::Receiver<CoordinatorEvent> {
        self.event_tx.subscribe()
    }

    pub async fn start(&self) -> AgentResult<()> {
        self.send_event(CoordinatorEvent::SessionStarted {
            session_id: self.session_id.clone(),
        })
        .await;

        if self.config.yolo_mode {
            self.mode_manager.enable_yolo().await;
            self.approval_manager.set_auto_approve(true).await;
        }

        Ok(())
    }

    pub async fn add_task(&self, description: impl Into<String>) -> AgentResult<String> {
        let task_id = self.next_id("task");
        let desc = description.into();

        let item = FocusItem::task(&task_id, &desc).with_description(&desc);
        self.focus_chain.add(item).await?;

        self.send_event(CoordinatorEvent::TaskStarted {
            task_id: task_id.clone(),
            description: desc,
        })
        .await;

        Ok(task_id)
    }

    pub async fn complete_task(&self, task_id: &str) -> AgentResult<()> {
        self.focus_chain
            .update(task_id, FocusStatus::Completed)
            .await?;

        self.send_event(CoordinatorEvent::TaskCompleted {
            task_id: task_id.to_string(),
        })
        .await;

        Ok(())
    }

    pub async fn fail_task(&self, task_id: &str, error: impl Into<String>) -> AgentResult<()> {
        self.focus_chain
            .update(task_id, FocusStatus::Blocked)
            .await?;

        self.send_event(CoordinatorEvent::TaskFailed {
            task_id: task_id.to_string(),
            error: error.into(),
        })
        .await;

        Ok(())
    }

    pub async fn switch_mode(&self, mode: AgentMode) -> AgentResult<()> {
        match mode {
            AgentMode::Act => {
                self.mode_manager.switch_to_act().await;
            }
            AgentMode::Plan => {
                self.mode_manager.switch_to_plan().await;
            }
            _ => {}
        }

        let state = self.mode_manager.get_state().await;
        self.send_event(CoordinatorEvent::ModeChanged {
            mode: format!("{:?}", state.mode),
            phase: format!("{:?}", state.phase),
        })
        .await;

        Ok(())
    }

    pub async fn can_execute(&self) -> bool {
        self.mode_manager.can_execute_tools().await
    }

    pub async fn should_auto_approve(&self, tool_name: &str, input: &serde_json::Value) -> bool {
        if self.mode_manager.should_auto_approve().await {
            return true;
        }

        if self.config.auto_approve_read_only {
            let read_only = ["read_file", "list_files", "search_files", "list_code_definitions"];
            if read_only.contains(&tool_name) {
                return true;
            }
        }

        !self.approval_policy.requires_approval(tool_name, input)
    }

    pub async fn build_prompt(&self, variant: ModelVariant) -> String {
        let builder = match variant {
            ModelVariant::Coding => PromptPresets::coding_assistant(),
            ModelVariant::Planning => PromptPresets::planner(),
            ModelVariant::Reviewing => PromptPresets::code_reviewer(),
            ModelVariant::Debugging => PromptPresets::debugger(),
            ModelVariant::Explaining => PromptPresets::teacher(),
            ModelVariant::Default => PromptPresets::task_executor(),
        };

        let state = self.mode_manager.get_state().await;
        let mode = match state.mode {
            agent_mode::AgentMode::Act => AgentMode::Act,
            agent_mode::AgentMode::Plan => AgentMode::Plan,
        };

        builder.mode(mode).build()
    }

    pub async fn progress(&self) -> (usize, usize) {
        self.focus_chain.progress().await
    }

    pub async fn current_task(&self) -> Option<FocusItem> {
        self.focus_chain.current().await
    }

    pub async fn advance_task(&self) -> AgentResult<Option<FocusItem>> {
        self.focus_chain.advance().await
    }

    pub async fn shutdown(&self) -> AgentResult<()> {
        self.focus_chain.clear().await?;
        self.approval_manager.clear_all_approvals().await;
        self.send_event(CoordinatorEvent::SessionEnded).await;
        Ok(())
    }

    pub fn tool_registry(&self) -> Arc<RwLock<ToolRegistry>> {
        Arc::clone(&self.tool_registry)
    }

    pub fn approval_manager(&self) -> Arc<ApprovalManager> {
        Arc::clone(&self.approval_manager)
    }

    pub fn mode_manager(&self) -> Arc<ModeManager> {
        Arc::clone(&self.mode_manager)
    }

    pub fn focus_chain(&self) -> &FocusChain {
        &self.focus_chain
    }

    async fn send_event(&self, event: CoordinatorEvent) {
        let _ = self.event_tx.send(event);
    }
}

pub struct CoordinatorChannels {
    pub approval_rx: mpsc::Receiver<ApprovalRequest>,
    pub followup_rx: mpsc::Receiver<(String, FollowupQuestion, oneshot::Sender<FollowupAnswer>)>,
    pub completion_rx: mpsc::Receiver<(String, CompletionAttempt, oneshot::Sender<CompletionFeedback>)>,
}

impl CoordinatorChannels {
    pub async fn handle_events<F, G, H>(
        mut self,
        mut on_approval: F,
        mut on_followup: G,
        mut on_completion: H,
    ) where
        F: FnMut(ApprovalRequest) -> ApprovalResponse,
        G: FnMut(FollowupQuestion) -> FollowupAnswer,
        H: FnMut(CompletionAttempt) -> CompletionFeedback,
    {
        loop {
            tokio::select! {
                Some(request) = self.approval_rx.recv() => {
                    let _response = on_approval(request);
                }
                Some((_, question, tx)) = self.followup_rx.recv() => {
                    let answer = on_followup(question);
                    let _ = tx.send(answer);
                }
                Some((_, attempt, tx)) = self.completion_rx.recv() => {
                    let feedback = on_completion(attempt);
                    let _ = tx.send(feedback);
                }
                else => break,
            }
        }
    }
}

pub struct CoordinatorBuilder {
    config: CoordinatorConfig,
}

impl CoordinatorBuilder {
    pub fn new() -> Self {
        Self {
            config: CoordinatorConfig::default(),
        }
    }

    pub fn initial_mode(mut self, mode: AgentMode) -> Self {
        self.config.initial_mode = mode;
        self
    }

    pub fn yolo_mode(mut self, enabled: bool) -> Self {
        self.config.yolo_mode = enabled;
        self
    }

    pub fn strict_plan_mode(mut self, enabled: bool) -> Self {
        self.config.strict_plan_mode = enabled;
        self
    }

    pub fn max_concurrent_tools(mut self, max: usize) -> Self {
        self.config.max_concurrent_tools = max;
        self
    }

    pub fn tool_timeout(mut self, secs: u64) -> Self {
        self.config.tool_timeout_secs = secs;
        self
    }

    pub fn auto_approve_read_only(mut self, enabled: bool) -> Self {
        self.config.auto_approve_read_only = enabled;
        self
    }

    pub fn build(self) -> (Coordinator, CoordinatorChannels) {
        Coordinator::new(self.config)
    }
}

impl Default for CoordinatorBuilder {
    fn default() -> Self {
        Self::new()
    }
}
