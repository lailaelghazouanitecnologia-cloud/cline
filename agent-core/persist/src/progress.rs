#![deny(clippy::all)]
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    pub task_id: String,
    pub stage: ProgressStage,
    pub message: String,
    pub current: u64,
    pub total: Option<u64>,
    pub percentage: Option<f32>,
    pub elapsed_ms: u64,
    pub eta_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressStage {
    Starting,
    Running,
    Processing,
    Streaming,
    Completing,
    Completed,
    Failed,
    Cancelled,
}

impl Progress {
    pub fn new(task_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            stage: ProgressStage::Starting,
            message: message.into(),
            current: 0,
            total: None,
            percentage: None,
            elapsed_ms: 0,
            eta_ms: None,
        }
    }

    pub fn with_total(mut self, total: u64) -> Self {
        self.total = Some(total);
        self.percentage = Some(0.0);
        self
    }

    pub fn update(&mut self, current: u64, elapsed_ms: u64) {
        self.current = current;
        self.elapsed_ms = elapsed_ms;

        if let Some(total) = self.total {
            if total > 0 {
                let pct = (current as f32 / total as f32) * 100.0;
                self.percentage = Some(pct);

                if current > 0 && elapsed_ms > 0 {
                    let rate = current as f64 / elapsed_ms as f64;
                    let remaining = total.saturating_sub(current);
                    self.eta_ms = Some((remaining as f64 / rate) as u64);
                }
            }
        }
    }

    pub fn is_complete(&self) -> bool {
        matches!(
            self.stage,
            ProgressStage::Completed | ProgressStage::Failed | ProgressStage::Cancelled
        )
    }
}

pub struct ProgressTracker {
    task_id: String,
    progress: Arc<RwLock<Progress>>,
    start_time: Instant,
    update_tx: broadcast::Sender<Progress>,
}

impl ProgressTracker {
    pub fn new(task_id: impl Into<String>, message: impl Into<String>) -> Self {
        let task_id = task_id.into();
        let (update_tx, _) = broadcast::channel(64);
        Self {
            progress: Arc::new(RwLock::new(Progress::new(&task_id, message))),
            task_id,
            start_time: Instant::now(),
            update_tx,
        }
    }

    pub fn with_total(self, total: u64) -> Self {
        let progress = self.progress.clone();
        tokio::spawn(async move {
            let mut guard = progress.write().await;
            guard.total = Some(total);
            guard.percentage = Some(0.0);
        });
        self
    }

    pub async fn set_stage(&self, stage: ProgressStage) {
        let mut progress = self.progress.write().await;
        progress.stage = stage;
        progress.elapsed_ms = self.start_time.elapsed().as_millis() as u64;
        let _ = self.update_tx.send(progress.clone());
    }

    pub async fn update(&self, current: u64) {
        let mut progress = self.progress.write().await;
        let elapsed = self.start_time.elapsed().as_millis() as u64;
        progress.update(current, elapsed);
        let _ = self.update_tx.send(progress.clone());
    }

    pub async fn set_message(&self, message: impl Into<String>) {
        let mut progress = self.progress.write().await;
        progress.message = message.into();
        progress.elapsed_ms = self.start_time.elapsed().as_millis() as u64;
        let _ = self.update_tx.send(progress.clone());
    }

    pub async fn increment(&self, amount: u64) {
        let mut progress = self.progress.write().await;
        let new_current = progress.current + amount;
        let elapsed = self.start_time.elapsed().as_millis() as u64;
        progress.update(new_current, elapsed);
        let _ = self.update_tx.send(progress.clone());
    }

    pub async fn complete(&self, message: impl Into<String>) {
        let mut progress = self.progress.write().await;
        progress.stage = ProgressStage::Completed;
        progress.message = message.into();
        progress.elapsed_ms = self.start_time.elapsed().as_millis() as u64;
        if let Some(total) = progress.total {
            progress.current = total;
            progress.percentage = Some(100.0);
        }
        progress.eta_ms = Some(0);
        let _ = self.update_tx.send(progress.clone());
    }

    pub async fn fail(&self, message: impl Into<String>) {
        let mut progress = self.progress.write().await;
        progress.stage = ProgressStage::Failed;
        progress.message = message.into();
        progress.elapsed_ms = self.start_time.elapsed().as_millis() as u64;
        let _ = self.update_tx.send(progress.clone());
    }

    pub async fn cancel(&self, message: impl Into<String>) {
        let mut progress = self.progress.write().await;
        progress.stage = ProgressStage::Cancelled;
        progress.message = message.into();
        progress.elapsed_ms = self.start_time.elapsed().as_millis() as u64;
        let _ = self.update_tx.send(progress.clone());
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Progress> {
        self.update_tx.subscribe()
    }

    pub async fn current(&self) -> Progress {
        self.progress.read().await.clone()
    }

    pub fn task_id(&self) -> &str {
        &self.task_id
    }
}

pub struct MultiProgressTracker {
    trackers: Arc<RwLock<Vec<ProgressTracker>>>,
    aggregate_tx: broadcast::Sender<Vec<Progress>>,
}

impl MultiProgressTracker {
    pub fn new() -> Self {
        let (aggregate_tx, _) = broadcast::channel(64);
        Self {
            trackers: Arc::new(RwLock::new(Vec::new())),
            aggregate_tx,
        }
    }

    pub async fn add(&self, tracker: ProgressTracker) {
        self.trackers.write().await.push(tracker);
    }

    pub async fn remove(&self, task_id: &str) {
        self.trackers
            .write()
            .await
            .retain(|t| t.task_id != task_id);
    }

    pub async fn all_progress(&self) -> Vec<Progress> {
        let trackers = self.trackers.read().await;
        let mut progress = Vec::with_capacity(trackers.len());
        for tracker in trackers.iter() {
            progress.push(tracker.current().await);
        }
        progress
    }

    pub async fn active_count(&self) -> usize {
        let trackers = self.trackers.read().await;
        let mut count = 0;
        for tracker in trackers.iter() {
            if !tracker.current().await.is_complete() {
                count += 1;
            }
        }
        count
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Vec<Progress>> {
        self.aggregate_tx.subscribe()
    }

    pub async fn broadcast_update(&self) {
        let progress = self.all_progress().await;
        let _ = self.aggregate_tx.send(progress);
    }
}

impl Default for MultiProgressTracker {
    fn default() -> Self {
        Self::new()
    }
}
