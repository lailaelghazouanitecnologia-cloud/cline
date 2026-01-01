#![deny(clippy::all)]
#![forbid(unsafe_code)]

use crate::item::{FocusItem, FocusStatus};
use agent_common::AgentResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FocusEvent {
    ItemAdded { item: FocusItem },
    ItemUpdated { item: FocusItem },
    ItemRemoved { id: String },
    ChainCleared,
    FocusChanged { current_id: Option<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainState {
    pub items: Vec<FocusItem>,
    pub current_focus: Option<String>,
    pub completed_count: usize,
    pub total_count: usize,
}

pub struct FocusChain {
    items: Arc<RwLock<HashMap<String, FocusItem>>>,
    order: Arc<RwLock<Vec<String>>>,
    current_focus: Arc<RwLock<Option<String>>>,
    event_tx: broadcast::Sender<FocusEvent>,
}

impl FocusChain {
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(100);
        Self {
            items: Arc::new(RwLock::new(HashMap::new())),
            order: Arc::new(RwLock::new(Vec::new())),
            current_focus: Arc::new(RwLock::new(None)),
            event_tx,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<FocusEvent> {
        self.event_tx.subscribe()
    }

    pub async fn add(&self, item: FocusItem) -> AgentResult<()> {
        let id = item.id.clone();
        {
            let mut items = self.items.write().await;
            let mut order = self.order.write().await;
            items.insert(id.clone(), item.clone());
            if !order.contains(&id) {
                order.push(id);
            }
        }
        let _ = self.event_tx.send(FocusEvent::ItemAdded { item });
        Ok(())
    }

    pub async fn add_after(&self, item: FocusItem, after_id: &str) -> AgentResult<()> {
        let id = item.id.clone();
        {
            let mut items = self.items.write().await;
            let mut order = self.order.write().await;
            items.insert(id.clone(), item.clone());

            if let Some(pos) = order.iter().position(|x| x == after_id) {
                order.insert(pos + 1, id);
            } else {
                order.push(id);
            }
        }
        let _ = self.event_tx.send(FocusEvent::ItemAdded { item });
        Ok(())
    }

    pub async fn get(&self, id: &str) -> Option<FocusItem> {
        self.items.read().await.get(id).cloned()
    }

    pub async fn update(&self, id: &str, status: FocusStatus) -> AgentResult<bool> {
        let updated = {
            let mut items = self.items.write().await;
            if let Some(item) = items.get_mut(id) {
                item.status = status;
                item.updated_at = std::time::SystemTime::now();
                Some(item.clone())
            } else {
                None
            }
        };

        if let Some(item) = updated {
            let _ = self.event_tx.send(FocusEvent::ItemUpdated { item });
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn remove(&self, id: &str) -> AgentResult<Option<FocusItem>> {
        let removed = {
            let mut items = self.items.write().await;
            let mut order = self.order.write().await;
            order.retain(|x| x != id);
            items.remove(id)
        };

        if removed.is_some() {
            let _ = self.event_tx.send(FocusEvent::ItemRemoved { id: id.to_string() });
        }
        Ok(removed)
    }

    pub async fn clear(&self) -> AgentResult<()> {
        {
            let mut items = self.items.write().await;
            let mut order = self.order.write().await;
            items.clear();
            order.clear();
        }
        *self.current_focus.write().await = None;
        let _ = self.event_tx.send(FocusEvent::ChainCleared);
        Ok(())
    }

    pub async fn set_focus(&self, id: Option<String>) -> AgentResult<()> {
        *self.current_focus.write().await = id.clone();
        let _ = self.event_tx.send(FocusEvent::FocusChanged { current_id: id });
        Ok(())
    }

    pub async fn current(&self) -> Option<FocusItem> {
        let focus_id = self.current_focus.read().await.clone();
        if let Some(id) = focus_id {
            self.get(&id).await
        } else {
            None
        }
    }

    pub async fn next_pending(&self) -> Option<FocusItem> {
        let order = self.order.read().await;
        let items = self.items.read().await;

        for id in order.iter() {
            if let Some(item) = items.get(id) {
                if item.status == FocusStatus::Pending {
                    return Some(item.clone());
                }
            }
        }
        None
    }

    pub async fn advance(&self) -> AgentResult<Option<FocusItem>> {
        if let Some(current_id) = self.current_focus.read().await.clone() {
            self.update(&current_id, FocusStatus::Completed).await?;
        }

        let next = self.next_pending().await;
        if let Some(ref item) = next {
            self.set_focus(Some(item.id.clone())).await?;
            self.update(&item.id, FocusStatus::InProgress).await?;
        } else {
            self.set_focus(None).await?;
        }
        Ok(next)
    }

    pub async fn state(&self) -> ChainState {
        let order = self.order.read().await;
        let items_map = self.items.read().await;
        let current = self.current_focus.read().await.clone();

        let items: Vec<FocusItem> = order
            .iter()
            .filter_map(|id| items_map.get(id).cloned())
            .collect();

        let completed = items.iter().filter(|i| i.is_done()).count();

        ChainState {
            total_count: items.len(),
            completed_count: completed,
            items,
            current_focus: current,
        }
    }

    pub async fn pending_items(&self) -> Vec<FocusItem> {
        let order = self.order.read().await;
        let items = self.items.read().await;

        order
            .iter()
            .filter_map(|id| items.get(id))
            .filter(|item| item.status == FocusStatus::Pending)
            .cloned()
            .collect()
    }

    pub async fn subtasks_of(&self, parent_id: &str) -> Vec<FocusItem> {
        let items = self.items.read().await;
        items
            .values()
            .filter(|item| item.parent_id.as_deref() == Some(parent_id))
            .cloned()
            .collect()
    }

    pub async fn progress(&self) -> (usize, usize) {
        let items = self.items.read().await;
        let completed = items.values().filter(|i| i.is_done()).count();
        (completed, items.len())
    }
}

impl Default for FocusChain {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for FocusChain {
    fn clone(&self) -> Self {
        Self {
            items: Arc::clone(&self.items),
            order: Arc::clone(&self.order),
            current_focus: Arc::clone(&self.current_focus),
            event_tx: self.event_tx.clone(),
        }
    }
}
