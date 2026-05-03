use async_trait::async_trait;
use serde_json::Value;
use pharmakon_common::{Tool, AgentResult, AgentError};
use pharmakon_mcp::McpClient;
use std::sync::Arc;

pub struct McpTool {
    client: Arc<McpClient>,
    name: String,
    description: String,
    parameters: Value,
}

impl McpTool {
    pub fn new(client: Arc<McpClient>, name: String, description: String, parameters: Value) -> Self {
        Self { client, name, description, parameters }
    }
}

#[async_trait]
impl Tool for McpTool {
    fn name(&self) -> &str { &self.name }
    fn description(&self) -> &str { &self.description }
    fn parameters(&self) -> Value { self.parameters.clone() }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let result: Value = self.client.call_tool(&self.name, args).await.map_err(|e| AgentError(e.to_string()))?;
        
        // MCP results often have a 'content' field with a list of parts
        if let Some(content) = result.get("content").and_then(|c| c.as_array()) {
            let mut output = String::new();
            for part in content {
                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                    output.push_str(text);
                }
            }
            Ok(output)
        } else {
            Ok(serde_json::to_string(&result).map_err(|e| AgentError(e.to_string()))?)
        }
    }
}
