#![deny(clippy::all)]
#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, watch};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbortReason {
    UserRequested,
    Timeout,
    Error,
    ContextLimit,
    ApiError,
    TaskComplete,
}

#[derive(Debug, Clone)]
pub struct AbortSignal {
    inner: Arc<AbortInner>,
}

struct AbortInner {
    aborted: AtomicBool,
    reason: std::sync::RwLock<Option<AbortReason>>,
    abort_tx: broadcast::Sender<AbortReason>,
    generation: AtomicU64,
}

impl std::fmt::Debug for AbortInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AbortInner")
            .field("aborted", &self.aborted)
            .field("reason", &self.reason)
            .field("generation", &self.generation)
            .finish()
    }
}

impl AbortSignal {
    pub fn new() -> Self {
        let (abort_tx, _) = broadcast::channel(16);
        Self {
            inner: Arc::new(AbortInner {
                aborted: AtomicBool::new(false),
                reason: std::sync::RwLock::new(None),
                abort_tx,
                generation: AtomicU64::new(0),
            }),
        }
    }

    pub fn abort(&self, reason: AbortReason) {
        if self
            .inner
            .aborted
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            if let Ok(mut guard) = self.inner.reason.write() {
                *guard = Some(reason);
            }
            let _ = self.inner.abort_tx.send(reason);
        }
    }

    pub fn is_aborted(&self) -> bool {
        self.inner.aborted.load(Ordering::SeqCst)
    }

    pub fn reason(&self) -> Option<AbortReason> {
        self.inner
            .reason
            .read()
            .ok()
            .and_then(|guard| *guard)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AbortReason> {
        self.inner.abort_tx.subscribe()
    }

    pub fn reset(&self) {
        self.inner.aborted.store(false, Ordering::SeqCst);
        if let Ok(mut guard) = self.inner.reason.write() {
            *guard = None;
        }
        self.inner.generation.fetch_add(1, Ordering::SeqCst);
    }

    pub fn generation(&self) -> u64 {
        self.inner.generation.load(Ordering::SeqCst)
    }

    pub fn check(&self) -> Result<(), AbortReason> {
        if self.is_aborted() {
            Err(self.reason().unwrap_or(AbortReason::UserRequested))
        } else {
            Ok(())
        }
    }
}

impl Default for AbortSignal {
    fn default() -> Self {
        Self::new()
    }
}

pub struct AbortController {
    signal: AbortSignal,
    status_tx: watch::Sender<AbortStatus>,
    status_rx: watch::Receiver<AbortStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbortStatus {
    Running,
    AbortRequested,
    Aborting,
    Aborted,
    Completed,
}

impl AbortController {
    pub fn new() -> Self {
        let (status_tx, status_rx) = watch::channel(AbortStatus::Running);
        Self {
            signal: AbortSignal::new(),
            status_tx,
            status_rx,
        }
    }

    pub fn signal(&self) -> &AbortSignal {
        &self.signal
    }

    pub fn request_abort(&self, reason: AbortReason) {
        let _ = self.status_tx.send(AbortStatus::AbortRequested);
        self.signal.abort(reason);
    }

    pub fn set_aborting(&self) {
        let _ = self.status_tx.send(AbortStatus::Aborting);
    }

    pub fn set_aborted(&self) {
        let _ = self.status_tx.send(AbortStatus::Aborted);
    }

    pub fn set_completed(&self) {
        let _ = self.status_tx.send(AbortStatus::Completed);
    }

    pub fn status(&self) -> AbortStatus {
        *self.status_rx.borrow()
    }

    pub fn subscribe_status(&self) -> watch::Receiver<AbortStatus> {
        self.status_rx.clone()
    }

    pub fn reset(&self) {
        self.signal.reset();
        let _ = self.status_tx.send(AbortStatus::Running);
    }
}

impl Default for AbortController {
    fn default() -> Self {
        Self::new()
    }
}

pub struct AbortGuard {
    signal: AbortSignal,
    on_abort: Option<Box<dyn FnOnce(AbortReason) + Send + 'static>>,
}

impl AbortGuard {
    pub fn new(signal: AbortSignal) -> Self {
        Self {
            signal,
            on_abort: None,
        }
    }

    pub fn on_abort<F>(mut self, f: F) -> Self
    where
        F: FnOnce(AbortReason) + Send + 'static,
    {
        self.on_abort = Some(Box::new(f));
        self
    }

    pub fn check(&self) -> Result<(), AbortReason> {
        self.signal.check()
    }
}

impl Drop for AbortGuard {
    fn drop(&mut self) {
        if self.signal.is_aborted() {
            if let Some(callback) = self.on_abort.take() {
                let reason = self.signal.reason().unwrap_or(AbortReason::UserRequested);
                callback(reason);
            }
        }
    }
}

pub struct CancellationToken {
    inner: Arc<CancellationInner>,
}

struct CancellationInner {
    cancelled: AtomicBool,
    notify: tokio::sync::Notify,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(CancellationInner {
                cancelled: AtomicBool::new(false),
                notify: tokio::sync::Notify::new(),
            }),
        }
    }

    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::SeqCst);
        self.inner.notify.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::SeqCst)
    }

    pub async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        self.inner.notify.notified().await;
    }

    pub fn child_token(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for CancellationToken {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}
