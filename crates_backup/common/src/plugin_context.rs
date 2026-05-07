use crate::Event;
use crate::agent_types::CommitmentPersistence;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct PluginContext {
    pub session_id: String,
    pub event_tx: Option<broadcast::Sender<Event>>,
    pub weaver: Option<Arc<dyn std::any::Any + Send + Sync>>, // Using Any to avoid circular dependencies with memory crate
    pub store: Option<Arc<dyn CommitmentPersistence>>,
}

impl PluginContext {
    pub fn new(session_id: String) -> Self {
        Self {
            session_id,
            event_tx: None,
            weaver: None,
            store: None,
        }
    }
}
