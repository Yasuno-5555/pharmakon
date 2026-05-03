use async_trait::async_trait;
use pharmakon_common::{Tool, AgentResult, AgentError};
use serde_json::{json, Value};

pub struct MemorySearchTool;

#[async_trait]
impl Tool for MemorySearchTool {
    fn name(&self) -> &str { "search_memory" }
    fn description(&self) -> &str { "Search long-term memory for relevant information" }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" }
            },
            "required": ["query"]
        })
    }

    async fn call(&self, _args: Value) -> AgentResult<String> {
        // Implementation would use semantic search
        Err(AgentError("Memory search tool not yet implemented".to_string()))
    }
}
