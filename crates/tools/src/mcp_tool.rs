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
        let start = std::time::Instant::now();

        // Context Injection: Add background info if it's an object
        let mut final_args = args.clone();
        if let Some(obj) = final_args.as_object_mut() {
            if !obj.contains_key("_pharmakon_context") {
                obj.insert("_pharmakon_context".to_string(), serde_json::json!({
                    "tool_name": &self.name,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }));
            }
        }

        let result: Value = self.client.call_tool(&self.name, final_args).await
            .map_err(|e| AgentError(e.to_string()))?;
        
        let elapsed = start.elapsed();
        log::info!("MCP Tool {} finished in {}ms", self.name, elapsed.as_millis());
        
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
