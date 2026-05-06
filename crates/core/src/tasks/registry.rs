use crate::persistence::DbSessionStore;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TaskFlow {
    pub id: String,
    pub session_id: String,
    pub goal: String,
    pub status: String,
    pub steps: Vec<String>,
}

pub struct TaskFlowRegistry {
    _store: Arc<DbSessionStore>,
}

impl TaskFlowRegistry {
    pub fn new(store: Arc<DbSessionStore>) -> Self {
        Self { _store: store }
    }

    pub async fn register_task(&self, task: TaskFlow) -> Result<()> {
        let _payload = serde_json::to_string(&task)?;
        // We can reuse the delivery_queue table or create a new task table
        // For now, let's just log it or add a dedicated table in persistence.rs later.
        log::info!("TaskFlow registered: {}", task.id);
        Ok(())
    }
}
