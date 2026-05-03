use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::agent::Agent;
use crate::model::AgentModel;
use crate::persistence::DbSessionStore;
use pharmakon_common::Config;
use anyhow::Result;

pub struct AgentRouter {
    agents: HashMap<String, Arc<Mutex<Agent>>>,
    model: Arc<dyn AgentModel>,
    store: Arc<DbSessionStore>,
}

impl AgentRouter {
    pub fn new(model: Arc<dyn AgentModel>, store: Arc<DbSessionStore>) -> Self {
        Self {
            agents: HashMap::new(),
            model,
            store,
        }
    }

    pub async fn get_agent(&mut self, name: &str) -> Result<Arc<Mutex<Agent>>> {
        if let Some(agent) = self.agents.get(name) {
            return Ok(agent.clone());
        }

        // Create a new agent based on config if it exists, otherwise use defaults
        let config = Config::load().unwrap_or_default();
        let _agent_config = config.agents.get(name).cloned().unwrap_or_default();
        
        let agent = Arc::new(Mutex::new(
            Agent::new(self.model.clone(), format!("agent-{}", name))
                .with_store(self.store.clone())
        ));
        
        self.agents.insert(name.to_string(), agent.clone());
        Ok(agent)
    }
}
