use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct DiscoverToolsTool {
    pub tool_registry: Arc<Mutex<Vec<Arc<dyn Tool>>>>,
}

impl DiscoverToolsTool {
    pub fn new() -> Self {
        Self {
            tool_registry: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait]
impl Tool for DiscoverToolsTool {
    fn name(&self) -> &str {
        "discover_tools"
    }
    fn description(&self) -> &str {
        "Search for available tools based on a query. Use this when you are not sure which tool to use for a specific task."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Keyword or description to search for in tool names/descriptions" }
            },
            "required": ["query"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let query = args["query"].as_str().unwrap_or("").to_lowercase();
        let registry = self.tool_registry.lock().await;

        let mut matches = Vec::new();
        for tool in registry.iter() {
            if tool.name().to_lowercase().contains(&query)
                || tool.description().to_lowercase().contains(&query)
            {
                matches.push(format!("- **{}**: {}", tool.name(), tool.description()));
            }
        }

        if matches.is_empty() {
            Ok(format!(
                "No tools found matching '{}'. Try a broader search or connect a new MCP server.",
                query
            ))
        } else {
            Ok(format!(
                "### Discovered Tools for '{}':\n\n{}",
                query,
                matches.join("\n")
            ))
        }
    }
}
