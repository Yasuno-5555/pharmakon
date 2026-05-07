use crate::agent::Agent;
use async_trait::async_trait;
use pharmakon_common::AgentSpawner;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct SwarmManager {
    parent: Arc<Mutex<Agent>>,
}

impl SwarmManager {
    pub fn new(parent: Arc<Mutex<Agent>>) -> Self {
        Self { parent }
    }
}

#[async_trait]
impl AgentSpawner for SwarmManager {
    async fn spawn(&self, task: &str, role: Option<String>, depth: u8) -> anyhow::Result<String> {
        if depth > 2 {
            return Ok(
                "Swarm depth limit reached. Task aborted to prevent recursion loop.".to_string(),
            );
        }

        let role_str = role.unwrap_or_else(|| "researcher".to_string());
        log::info!(
            "SwarmManager: Spawning autonomous '{}' agent for task: '{}' (Depth: {})",
            role_str,
            task,
            depth
        );

        let (
            model,
            session_store,
            tools,
            knowledge_nexus,
            semantic_search,
            fact_memory,
            territory_manager,
        ) = {
            let parent_lock = self.parent.lock().await;
            (
                parent_lock.model.clone(),
                parent_lock.session_store.clone(),
                parent_lock.tools.clone(),
                parent_lock.knowledge_nexus.clone(),
                parent_lock.semantic_search.clone(),
                parent_lock.fact_memory.clone(),
                parent_lock.territory_manager.clone(),
            )
        };

        let mut sub_agent_tools: Vec<Arc<dyn pharmakon_common::Tool>> = {
            let t = tools.lock().await;
            t.iter().cloned().collect()
        };

        // Remove tools that might be dangerous for sub-agents or cause infinite recursion
        sub_agent_tools
            .retain(|t| t.name() != "spawn_sub_agent" && t.name() != "run_shell_command");

        let session_id = format!("swarm-depth{}-{}", depth, rand::random::<u32>());

        let inner_model = {
            let m = model.lock().await;
            m.clone()
        };
        let mut sub_agent = Agent::new(inner_model, session_id.clone());
        if let Some(store) = session_store {
            sub_agent = sub_agent.with_store(store);
        }
        if let Some(nexus) = knowledge_nexus {
            sub_agent = sub_agent
                .with_knowledge_nexus(nexus)
                .with_isolated_knowledge();
        }
        if let Some(search) = semantic_search {
            sub_agent = sub_agent.with_semantic_search(search);
        }

        sub_agent.fact_memory = fact_memory;
        sub_agent.territory_manager = territory_manager;
        sub_agent.tools = Arc::new(Mutex::new(sub_agent_tools));

        // Apply specialized Soul based on role
        let soul = crate::soul::Soul::expert(&role_str);
        sub_agent.set_soul(soul).await;

        let sub_agent_arc = Arc::new(Mutex::new(sub_agent));
        let task_clone = task.to_string();
        let session_id_clone = session_id.clone();

        tokio::spawn(async move {
            log::info!("Sub-agent {} starting task...", session_id_clone);
            let response = {
                let agent_lock = sub_agent_arc.lock().await;
                agent_lock.chat(&task_clone).await
            };

            match response {
                Ok(res) => {
                    log::info!(
                        "Sub-agent {} completed task. Response snippet: {:.100}",
                        session_id_clone,
                        res
                    );
                    // Commit isolated knowledge back to global store upon success
                    let agent_lock = sub_agent_arc.lock().await;
                    if let Err(e) = agent_lock.commit_knowledge().await {
                        log::error!(
                            "Sub-agent {} failed to commit knowledge: {}",
                            session_id_clone,
                            e
                        );
                    }
                }
                Err(e) => {
                    log::error!("Sub-agent {} failed: {}", session_id_clone, e);
                }
            }
        });

        Ok(format!(
            "Sub-agent [{}] deployed successfully as a {}.",
            session_id, role_str
        ))
    }
}

pub struct SwarmTool {
    spawner: Arc<dyn AgentSpawner>,
    depth: u8,
}

impl SwarmTool {
    pub fn new(spawner: Arc<dyn AgentSpawner>, depth: u8) -> Self {
        Self { spawner, depth }
    }
}

#[async_trait]
impl pharmakon_common::Tool for SwarmTool {
    fn name(&self) -> &str {
        "spawn_sub_agent"
    }
    fn description(&self) -> &str {
        "Spawn a parallel sub-agent with a specific role to handle a sub-task independently in the background."
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "task": { "type": "string", "description": "The specific task for the sub-agent to execute completely autonomously." },
                "role": { "type": "string", "description": "The specialized role of the sub-agent (e.g., 'researcher', 'coder', 'analyst')." }
            },
            "required": ["task"]
        })
    }

    async fn call(&self, args: serde_json::Value) -> pharmakon_common::AgentResult<String> {
        let task = args["task"].as_str().unwrap_or_default();
        let role = args["role"].as_str().map(|s| s.to_string());

        self.spawner
            .spawn(task, role, self.depth + 1)
            .await
            .map_err(|e| pharmakon_common::AgentError(e.to_string()))
    }
}
