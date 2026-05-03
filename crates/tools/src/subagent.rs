use async_trait::async_trait;
use pharmakon_common::{Tool, AgentResult, AgentError, AgentSpawner};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct SubAgentTool {
    spawner: Arc<dyn AgentSpawner>,
    depth: u8,
}

impl SubAgentTool {
    pub fn new(spawner: Arc<dyn AgentSpawner>) -> Self {
        Self { spawner, depth: 0 }
    }

    pub fn new_with_depth(spawner: Arc<dyn AgentSpawner>, depth: u8) -> Self {
        Self { spawner, depth }
    }
}

#[async_trait]
impl Tool for SubAgentTool {
    fn name(&self) -> &str { "spawn_subagent" }
    fn description(&self) -> &str { "Spawn a sub-agent to handle a specific sub-task and return the result" }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task": { "type": "string", "description": "The specific task for the sub-agent" },
                "soul": { "type": "string", "description": "Optional soul personality override" }
            },
            "required": ["task"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let task = args["task"].as_str().ok_or_else(|| AgentError("Missing task".to_string()))?;
        let soul = args["soul"].as_str().map(|s| s.to_string());
        
        log::info!("Spawning sub-agent for task: {} (current depth: {})", task, self.depth);
        self.spawner.spawn(task, soul, self.depth).await.map_err(|e| AgentError(e.to_string()))
    }
}
