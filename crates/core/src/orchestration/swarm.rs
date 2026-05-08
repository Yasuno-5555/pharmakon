use crate::agent::Agent;
use crate::orchestration::scheduler::{ManagedTask, classify_task_complexity};
use async_trait::async_trait;
use pharmakon_common::AgentSpawner;
use std::sync::Arc;
use tokio::sync::Mutex;

// --- Spawn Decision (Cost-Benefit Analysis) ---

/// Decision about whether and how to spawn sub-agents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpawnDecision {
    /// Tasks are independent and parallelizable — spawn sub-agents.
    Parallel {
        /// Number of sub-agents to spawn.
        count: usize,
        /// Estimated token savings from parallel execution.
        estimated_savings_tokens: usize,
    },
    /// Tasks have dependencies — execute sequentially.
    Sequential,
    /// Overhead exceeds benefit — execute inline (no spawn).
    Inline {
        /// Why spawning would be wasteful.
        reason: String,
    },
}

/// Analyze whether spawning sub-agents is worth the overhead.
///
/// The break-even formula:
///   benefit = parallelism_gain - spawn_overhead - context_dependency_cost
///
/// Where:
///   - parallelism_gain: tokens saved by running tasks in parallel (vs sequential)
///   - spawn_overhead: tokens burned to set up each sub-agent (~500 tokens)
///   - context_dependency_cost: tokens needed to share state between agents
pub fn analyze_spawn_decision(
    sub_tasks: &[String],
    shared_context_size: usize,
) -> SpawnDecision {
    const SPAWN_OVERHEAD_PER_AGENT: usize = 500; // ~500 tokens to set up a sub-agent
    const MIN_TASK_SIZE_FOR_SPAWN: usize = 200; // Don't spawn for trivial tasks

    let task_count = sub_tasks.len();

    // Rule 1: Single task → never spawn
    if task_count <= 1 {
        return SpawnDecision::Inline {
            reason: "Single task — no parallelism benefit".to_string(),
        };
    }

    // Rule 2: Any task too small → inline
    if sub_tasks.iter().any(|t| t.len() < MIN_TASK_SIZE_FOR_SPAWN) {
        return SpawnDecision::Inline {
            reason: "Sub-tasks too small — spawn overhead exceeds benefit".to_string(),
        };
    }

    // Rule 3: Calculate break-even
    let spawn_overhead = task_count * SPAWN_OVERHEAD_PER_AGENT;

    // Parallelism gain: rough estimate — if executed sequentially,
    // each sub-task burns its own context. In parallel, they share none.
    let avg_task_tokens = sub_tasks.iter().map(|t| t.len() / 4).sum::<usize>() / task_count;
    let sequential_cost = task_count * avg_task_tokens;
    let parallel_cost = avg_task_tokens + spawn_overhead;
    let net_savings = sequential_cost.saturating_sub(parallel_cost + shared_context_size);

    if net_savings > 0 {
        SpawnDecision::Parallel {
            count: task_count,
            estimated_savings_tokens: net_savings,
        }
    } else if shared_context_size < avg_task_tokens * 2 {
        // Context dependency is manageable → run sequentially (no spawn, but not wasted)
        SpawnDecision::Sequential
    } else {
        SpawnDecision::Inline {
            reason: format!(
                "Context dependency ({}) exceeds benefit — inline execution preferred",
                shared_context_size
            ),
        }
    }
}

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
            registry,
            knowledge_nexus,
            semantic_search,
            fact_memory,
            territory_manager,
        ) = {
            let parent_lock = self.parent.lock().await;
            (
                parent_lock.model.clone(),
                parent_lock.session_store.clone(),
                parent_lock.registry.clone(),
                parent_lock.knowledge_nexus.clone(),
                parent_lock.semantic_search.clone(),
                parent_lock.fact_memory.clone(),
                parent_lock.territory_manager.clone(),
            )
        };

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

        let soul = crate::soul::Soul::expert(&role_str);
        sub_agent.set_soul(soul).await;

        let sub_agent_arc = Arc::new(Mutex::new(sub_agent));
        let task_clone = task.to_string();
        let (tx, rx) = tokio::sync::oneshot::channel();
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
                        "Sub-agent {} completed task. Response length: {} chars",
                        session_id_clone,
                        res.len()
                    );
                    let agent_lock = sub_agent_arc.lock().await;
                    if let Err(e) = agent_lock.commit_knowledge().await {
                        log::error!(
                            "Sub-agent {} failed to commit knowledge: {}",
                            session_id_clone,
                            e
                        );
                    }
                    let _ = tx.send(Ok(res));
                }
                Err(e) => {
                    log::error!("Sub-agent {} failed: {}", session_id_clone, e);
                    let _ = tx.send(Err(e));
                }
            }
        });

        // Return deployment confirmation immediately (sub-agent runs in background).
        // Use spawn_with_handle() if you need the actual result.
        Ok(format!(
            "Sub-agent [{}] deployed successfully as a {}.",
            session_id, role_str
        ))
    }

    /// Return a SpawnHandle that resolves when the sub-agent completes.
    async fn spawn_with_handle(
        &self,
        task: &str, soul: Option<String>, depth: u8,
    ) -> anyhow::Result<pharmakon_common::SpawnHandle> {
        if depth > 2 {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = tx.send(Ok(
                "Swarm depth limit reached. Task aborted to prevent recursion loop.".to_string(),
            ));
            return Ok(pharmakon_common::SpawnHandle::new(rx));
        }

        // Clone parent resources for the background spawn
        let task_owned = task.to_string();
        let soul_owned = soul;
        let parent = self.parent.clone();

        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            // Build and run sub-agent directly (avoids recursive Arc<dyn AgentSpawner>)
            let result = run_swarm_sub_agent(&parent, &task_owned, soul_owned, depth).await;
            let _ = tx.send(result.map_err(|e| anyhow::anyhow!(e)));
        });

        Ok(pharmakon_common::SpawnHandle::new(rx))
    }
}

/// Run a swarm sub-agent with all parent resources cloned.
async fn run_swarm_sub_agent(
    parent: &Arc<Mutex<Agent>>,
    task: &str,
    role: Option<String>,
    depth: u8,
) -> anyhow::Result<String> {
    let role_str = role.unwrap_or_else(|| "researcher".to_string());
    let (model, session_store, registry, knowledge_nexus, semantic_search, fact_memory, territory_manager) = {
        let parent_lock = parent.lock().await;
        (parent_lock.model.clone(), parent_lock.session_store.clone(), parent_lock.registry.clone(),
         parent_lock.knowledge_nexus.clone(), parent_lock.semantic_search.clone(),
         parent_lock.fact_memory.clone(), parent_lock.territory_manager.clone())
    };
    let session_id = format!("swarm-depth{}-{}", depth, rand::random::<u32>());
    let inner = { let m = model.lock().await; m.clone() };
    let mut sub = Agent::new(inner, session_id.clone());
    if let Some(s) = session_store { sub = sub.with_store(s); }
    if let Some(n) = knowledge_nexus { sub = sub.with_knowledge_nexus(n).with_isolated_knowledge(); }
    if let Some(s) = semantic_search { sub = sub.with_semantic_search(s); }
    sub.fact_memory = fact_memory;
    sub.territory_manager = territory_manager;
    sub.set_soul(crate::soul::Soul::expert(&role_str)).await;
    sub.chat(task).await
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

pub struct FractalSwarmTool {
    spawner: Arc<dyn AgentSpawner>,
    depth: u8,
}

impl FractalSwarmTool {
    pub fn new(spawner: Arc<dyn AgentSpawner>, depth: u8) -> Self {
        Self { spawner, depth }
    }
}

#[async_trait]
impl pharmakon_common::Tool for FractalSwarmTool {
    fn name(&self) -> &str {
        "fractal_swarm"
    }

    fn description(&self) -> &str {
        "Decompose a task into nested micro-agent work packets and execute them in parallel. \
         Waits for all sub-agents to finish and returns a collective result. Use for complex, \
         parallelizable engineering tasks."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "goal": { "type": "string", "description": "The overall goal" },
                "sub_tasks": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "task": { "type": "string" },
                            "role": { "type": "string" }
                        },
                        "required": ["task", "role"]
                    }
                }
            },
            "required": ["goal", "sub_tasks"]
        })
    }

    async fn call(&self, args: serde_json::Value) -> pharmakon_common::AgentResult<String> {
        let goal = args["goal"].as_str().unwrap_or_default();
        let sub_tasks = args["sub_tasks"]
            .as_array()
            .ok_or_else(|| pharmakon_common::AgentError("Missing sub_tasks".to_string()))?;

        log::info!("FractalSwarm: Processing goal '{}' with {} sub-tasks", goal, sub_tasks.len());

        let mut handles = Vec::new();
        for task_val in sub_tasks {
            let task = task_val["task"].as_str().unwrap_or_default().to_string();
            let role = task_val["role"].as_str().map(|s| s.to_string());
            let spawner = self.spawner.clone();
            let depth = self.depth;

            // Use spawn_with_handle to get actual results from sub-agents
            handles.push(async move {
                match spawner.spawn_with_handle(&task, role, depth + 1).await {
                    Ok(handle) => match handle.await_result().await {
                        Ok(result) => Ok(result),
                        Err(e) => Err(e),
                    },
                    Err(e) => Err(e),
                }
            });
        }

        let results = futures::future::join_all(handles).await;
        let mut summary = format!("Fractal Swarm execution for: {}\n\n", goal);
        for (i, res) in results.into_iter().enumerate() {
            match res {
                Ok(msg) => summary.push_str(&format!("Task {}: OK — {}\n", i + 1, msg)),
                Err(e) => summary.push_str(&format!("Task {}: FAILED — {}\n", i + 1, e)),
            }
        }

        Ok(summary)
    }
}