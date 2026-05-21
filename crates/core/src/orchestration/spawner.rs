use crate::agent::Agent;
use crate::model::AgentModel;
use crate::persistence::DbSessionStore;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use pharmakon_common::AgentSpawner;
use std::sync::Arc;

pub struct DefaultAgentSpawner {
    model: Arc<dyn AgentModel>,
    session_store: Option<Arc<DbSessionStore>>,
}

impl DefaultAgentSpawner {
    pub fn new(model: Arc<dyn AgentModel>, store: Option<Arc<DbSessionStore>>) -> Self {
        Self {
            model,
            session_store: store,
        }
    }
}

#[async_trait]
impl AgentSpawner for DefaultAgentSpawner {
    async fn spawn(&self, task: &str, _soul: Option<String>, depth: u8) -> Result<String> {
        if depth > 3 {
            return Err(anyhow!(
                "Maximum sub-agent recursion depth exceeded (limit: 3)"
            ));
        }

        let session_id = format!("subagent-{}", uuid::Uuid::new_v4());
        let mut agent = Agent::new(self.model.clone(), session_id);

        if let Some(store) = &self.session_store {
            agent = agent.with_store(store.clone());
        }

        // Initialize all agent tools for the sub-agent so they can perform tasks
        crate::tool_init::init_all_agent_tools(&agent).await?;

        log::info!(
            "Sub-agent starting task (depth: {}) in session: {}",
            depth,
            *agent.session_id.lock().await
        );
        agent.chat(task).await
    }
}
