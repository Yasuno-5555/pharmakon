use crate::agent::Agent;
use crate::model::AgentModel;
use crate::persistence::DbSessionStore;
use pharmakon_common::AgentSpawner;
use anyhow::{Result, anyhow};
use std::sync::Arc;
use async_trait::async_trait;
use pharmakon_tools::subagent::SubAgentTool;

pub struct DefaultAgentSpawner {
    model: Arc<dyn AgentModel>,
    session_store: Option<Arc<DbSessionStore>>,
}

impl DefaultAgentSpawner {
    pub fn new(model: Arc<dyn AgentModel>, store: Option<Arc<DbSessionStore>>) -> Self {
        Self { model, session_store: store }
    }
}

#[async_trait]
impl AgentSpawner for DefaultAgentSpawner {
    async fn spawn(&self, task: &str, _soul: Option<String>, depth: u8) -> Result<String> {
        if depth > 3 {
            return Err(anyhow!("Maximum sub-agent recursion depth exceeded (limit: 3)"));
        }

        let session_id = format!("subagent-{}", uuid::Uuid::new_v4());
        let mut agent = Agent::new(self.model.clone(), session_id);
        
        if let Some(store) = &self.session_store {
            agent = agent.with_store(store.clone());
        }
        
        // Register tools for sub-agent (essential tools)
        agent.add_tool(Arc::new(pharmakon_tools::ShellTool));
        agent.add_tool(Arc::new(pharmakon_tools::FileReadTool));
        // Add SubAgentTool with incremented depth
        agent.add_tool(Arc::new(SubAgentTool::new_with_depth(
            Arc::new(DefaultAgentSpawner::new(self.model.clone(), self.session_store.clone())),
            depth + 1
        )));
        
        log::info!("Sub-agent starting task (depth: {}) in session: {}", depth, agent.session_id);
        agent.chat(task).await
    }
}
