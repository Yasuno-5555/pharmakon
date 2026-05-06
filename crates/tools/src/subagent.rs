use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, AgentSpawner, Tool};
use serde_json::{Value, json};
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
    fn name(&self) -> &str {
        "spawn_subagent"
    }
    fn description(&self) -> &str {
        "Spawn a sub-agent to handle a specific sub-task and return the result"
    }
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
        let task = args["task"]
            .as_str()
            .ok_or_else(|| AgentError("Missing task".to_string()))?;
        let soul = args["soul"].as_str().map(|s| s.to_string());

        log::info!(
            "Spawning sub-agent for task: {} (current depth: {})",
            task,
            self.depth
        );
        self.spawner
            .spawn(task, soul, self.depth)
            .await
            .map_err(|e| AgentError(e.to_string()))
    }
}

pub struct ParallelSwarmTool {
    spawner: Arc<dyn AgentSpawner>,
    depth: u8,
}

impl ParallelSwarmTool {
    pub fn new(spawner: Arc<dyn AgentSpawner>, depth: u8) -> Self {
        Self { spawner, depth }
    }
}

#[async_trait]
impl Tool for ParallelSwarmTool {
    fn name(&self) -> &str {
        "swarm_parallel"
    }
    fn description(&self) -> &str {
        "Spawn multiple sub-agents in parallel to handle distinct sub-tasks. Best for large problems that can be decomposed."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "tasks": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of distinct tasks for sub-agents to handle"
                },
                "plan": { "type": "string", "description": "Overall coordination plan for the swarm" }
            },
            "required": ["tasks"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let tasks = args["tasks"]
            .as_array()
            .ok_or_else(|| AgentError("Missing tasks array".to_string()))?;

        log::info!(
            "Initiating Swarm with {} tasks (depth: {})",
            tasks.len(),
            self.depth
        );

        let mut spawn_futures = Vec::new();
        for task_val in tasks {
            if let Some(task_str) = task_val.as_str() {
                spawn_futures.push(self.spawner.spawn(task_str, None, self.depth + 1));
            }
        }

        let results = futures::future::join_all(spawn_futures).await;

        let mut report = String::from("### Swarm Execution Report\n\n");
        for (i, res) in results.into_iter().enumerate() {
            let task_name = tasks[i].as_str().unwrap_or("Unknown Task");
            match res {
                Ok(out) => report.push_str(&format!(
                    "#### Task {}: {}\n- **Result**: SUCCESS\n- **Output**: {}\n\n",
                    i + 1,
                    task_name,
                    out
                )),
                Err(e) => report.push_str(&format!(
                    "#### Task {}: {}\n- **Result**: FAILED\n- **Error**: {}\n\n",
                    i + 1,
                    task_name,
                    e
                )),
            }
        }

        Ok(report)
    }
}

pub struct NoopSpawner;

#[async_trait]
impl AgentSpawner for NoopSpawner {
    async fn spawn(&self, _task: &str, _soul: Option<String>, _depth: u8) -> anyhow::Result<String> {
        anyhow::bail!("Sub-agent spawning is not enabled in this context. Use direct tool calls instead.")
    }
}
