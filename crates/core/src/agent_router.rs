use crate::agent::Agent;
use crate::model::AgentModel;
use crate::persistence::DbSessionStore;
use crate::soul::Soul;
use anyhow::Result;
use pharmakon_common::Config;
use pharmakon_common::ToolRegistry;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct AgentRouter {
    agents: HashMap<String, Arc<Mutex<Agent>>>,
    model: Arc<dyn AgentModel>,
    store: Arc<DbSessionStore>,
    config: Config,
    weaver: Option<Arc<pharmakon_memory::weaver::MemoryWeaver>>,
    fact_memory: Option<Arc<Mutex<pharmakon_memory::fact_memory::FactMemory>>>,
}

impl AgentRouter {
    pub fn new(
        model: Arc<dyn AgentModel>,
        store: Arc<DbSessionStore>,
        config: Config,
        weaver: Option<Arc<pharmakon_memory::weaver::MemoryWeaver>>,
        fact_memory: Option<Arc<Mutex<pharmakon_memory::fact_memory::FactMemory>>>,
    ) -> Self {
        Self {
            agents: HashMap::new(),
            model,
            store,
            config,
            weaver,
            fact_memory,
        }
    }

    pub async fn get_agent(&mut self, name: &str) -> Result<Arc<Mutex<Agent>>> {
        if let Some(agent) = self.agents.get(name) {
            return Ok(agent.clone());
        }

        let agent_config = self.config.agents.get(name).cloned();

        let mut agent = Agent::new(self.model.clone(), format!("agent-{}", name))
            .with_store(self.store.clone());

        // Dynamically load model based on agent_config.model_id
        if let Some(config) = &agent_config {
            if let Some(model_id) = &config.model_id {
                if let Some(dynamic_model) =
                    crate::providers::registry::ModelRegistry::get_model(model_id)
                {
                    agent = Agent::new(dynamic_model, format!("agent-{}", name))
                        .with_store(self.store.clone());

                    if let Some(w) = &self.weaver {
                        agent = agent.with_memory_weaver(w.clone());
                    }
                    if let Some(f) = &self.fact_memory {
                        agent = agent.with_fact_memory(f.clone());
                    }
                }
            }
        }

        if agent.memory_weaver.is_none() {
            if let Some(w) = &self.weaver {
                agent = agent.with_memory_weaver(w.clone());
            }
        }
        if agent.fact_memory.is_none() {
            if let Some(f) = &self.fact_memory {
                agent = agent.with_fact_memory(f.clone());
            }
        }

        let soul = if let Some(config) = &agent_config {
            if let Some(soul_path_str) = &config.soul_path {
                let soul_path = Path::new(soul_path_str);
                Soul::load_from_file(soul_path).unwrap_or_else(|e| {
                    log::warn!(
                        "Failed to load soul from {}: {:?}. Using default soul.",
                        soul_path_str,
                        e
                    );
                    Soul::default_soul()
                })
            } else {
                Soul::default_soul()
            }
        } else {
            Soul::default_soul()
        };

        agent = agent.with_soul(soul);

        // Load and add tools based on agent_config.allowed_tools
        if let Some(config) = &agent_config {
            if let Some(allowed_tools) = &config.allowed_tools {
                let deps = pharmakon_tools::registry::ToolDependencies {
                    model: Some(self.model.clone()),
                    store: Some(
                        self.store.clone() as Arc<dyn pharmakon_common::CommitmentPersistence>
                    ),
                    soul_manager: None, // TODO: Initialize SoulManager if needed
                    event_tx: None,     // TODO: Initialize Event broadcaster if needed
                    weaver: self.weaver.clone(),
                };

                for tool_name in allowed_tools {
                    if let Some(tool) =
                        pharmakon_tools::registry::ToolRegistry::get_tool(tool_name, &deps)
                    {
                        agent.add_tool(tool);
                    }
                }
            }
        }

        let agent_arc = Arc::new(Mutex::new(agent));
        self.agents.insert(name.to_string(), agent_arc.clone());
        Ok(agent_arc)
    }

    pub async fn create_team(&mut self, goal: &str) -> Result<crate::orchestration::Supervisor> {
        log::info!("Creating team for goal: {}", goal);

        // For now, we'll use a static team of Manager and Researcher.
        // In a future version, an LLM will analyze the goal and select agents dynamically.

        let manager = self.get_agent("Manager").await?;
        let researcher = self.get_agent("Researcher").await?;

        // Add supervisor tools to agents
        {
            let mut m = manager.lock().await;
            m.add_tool(Arc::new(crate::orchestration::TeamMessageTool {
                from: "Manager".to_string(),
            }));
            m.add_tool(Arc::new(crate::orchestration::FinalAnswerTool));
        }
        {
            let mut r = researcher.lock().await;
            r.add_tool(Arc::new(crate::orchestration::TeamMessageTool {
                from: "Researcher".to_string(),
            }));
        }

        let mut supervisor =
            crate::orchestration::Supervisor::new(goal.to_string(), "Manager".to_string());
        supervisor.add_agent("Manager".to_string(), manager);
        supervisor.add_agent("Researcher".to_string(), researcher);

        Ok(supervisor)
    }
}
