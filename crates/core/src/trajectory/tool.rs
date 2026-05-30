use crate::agent::Agent;
use crate::trajectory::insight_synthesizer::InsightSynthesizer;
use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool};
use serde_json::{Value, json};
use std::sync::Weak;

pub struct InsightTool {
    agent_ref: Weak<Agent>,
}

impl InsightTool {
    pub fn new(agent: Weak<Agent>) -> Self {
        Self { agent_ref: agent }
    }
}

#[async_trait]
impl Tool for InsightTool {
    fn name(&self) -> &str {
        "synthesize_insights"
    }
    fn description(&self) -> &str {
        "Analyze the current session's trajectory and synthesize 'Lessons Learned' into the project's knowledge base. Use this at the end of a successful task or after a complex failure."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn call(&self, _args: Value) -> AgentResult<String> {
        if let Some(agent) = self.agent_ref.upgrade() {
            match InsightSynthesizer::synthesize(agent).await {
                Ok(insight) => Ok(insight),
                Err(e) => Err(AgentError(format!("Failed to synthesize insights: {}", e))),
            }
        } else {
            Err(AgentError("Agent reference lost".to_string()))
        }
    }
}

pub struct ExecutionTraceTool {
    agent_ref: Weak<Agent>,
}

impl ExecutionTraceTool {
    pub fn new(agent: Weak<Agent>) -> Self {
        Self { agent_ref: agent }
    }
}

#[async_trait]
impl Tool for ExecutionTraceTool {
    fn name(&self) -> &str {
        "execution_trace"
    }

    fn description(&self) -> &str {
        "Retrieve the detailed history of thoughts, tool calls, and results for the current session or a specific session. Useful for debugging 'why' the agent took a certain path."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string" },
                "limit": { "type": "integer", "default": 20 }
            }
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        if let Some(agent) = self.agent_ref.upgrade() {
            let _session_id = args["session_id"]
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    let id = "default".to_string();
                    // Try to get current session ID
                    if let Ok(_rt) = tokio::runtime::Handle::try_current() {
                        // We can't easily access task_local here without the scope
                    }
                    id
                });

            if let Some(_store) = &agent.session_store {
                // We need a method in store to load trajectory events
                // For now, let's assume we can get them.
                // Actually, let's just return the Markdown representation from the Agent's current trajectory
                let t = agent.trajectory.lock().await;
                Ok(t.to_markdown())
            } else {
                Err(AgentError("No session store configured".to_string()))
            }
        } else {
            Err(AgentError("Agent reference lost".to_string()))
        }
    }
}

pub struct ToolReliabilityTool {
    agent_ref: Weak<Agent>,
}

impl ToolReliabilityTool {
    pub fn new(agent: Weak<Agent>) -> Self {
        Self { agent_ref: agent }
    }
}

#[async_trait]
impl Tool for ToolReliabilityTool {
    fn name(&self) -> &str {
        "tool_reliability"
    }

    fn description(&self) -> &str {
        "Get reliability and performance statistics for all tools, including success rate and average latency."
    }

    fn parameters(&self) -> Value {
        json!({ "type": "object", "properties": {} })
    }

    async fn call(&self, _args: Value) -> AgentResult<String> {
        if let Some(agent) = self.agent_ref.upgrade() {
            if let Some(store) = &agent.session_store {
                match store.get_tool_metrics().await {
                    Ok(stats) => Ok(serde_json::to_string_pretty(&stats).unwrap_or_default()),
                    Err(e) => Err(AgentError(format!("Failed to load tool metrics: {}", e))),
                }
            } else {
                Err(AgentError("No session store configured".to_string()))
            }
        } else {
            Err(AgentError("Agent reference lost".to_string()))
        }
    }
}

pub struct SemanticGrepTool {
    agent_ref: Weak<Agent>,
}

impl SemanticGrepTool {
    pub fn new(agent: Weak<Agent>) -> Self {
        Self { agent_ref: agent }
    }
}

#[async_trait]
impl Tool for SemanticGrepTool {
    fn name(&self) -> &str {
        "semantic_grep"
    }

    fn description(&self) -> &str {
        "Search for code snippets or documentation using semantic similarity and Knowledge Nexus insights. Better than regular grep for finding conceptual matches."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Conceptual search query" },
                "limit": { "type": "integer", "default": 5 }
            },
            "required": ["query"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        if let Some(agent) = self.agent_ref.upgrade() {
            let query = args["query"].as_str().unwrap();
            let limit = args["limit"].as_u64().unwrap_or(5) as usize;

            if let Some(nexus) = &agent.knowledge_nexus {
                match nexus.smart_search(query, limit).await {
                    Ok(results) => Ok(results.join("\n---\n")),
                    Err(e) => Err(AgentError(format!("Nexus search failed: {}", e))),
                }
            } else {
                Err(AgentError("Knowledge Nexus not available".to_string()))
            }
        } else {
            Err(AgentError("Agent reference lost".to_string()))
        }
    }
}
