use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool};
use pharmakon_mcp::McpClient;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct McpTool {
    client: Arc<McpClient>,
    name: String,
    description: String,
    parameters: Value,
}

impl McpTool {
    pub fn new(
        client: Arc<McpClient>,
        name: String,
        description: String,
        parameters: Value,
    ) -> Self {
        Self {
            client,
            name,
            description,
            parameters,
        }
    }
}

#[async_trait]
impl Tool for McpTool {
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        &self.description
    }
    fn parameters(&self) -> Value {
        self.parameters.clone()
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let start = std::time::Instant::now();

        // Context Injection: Add background info if it's an object
        let mut final_args = args.clone();
        if let Some(obj) = final_args.as_object_mut() {
            if !obj.contains_key("_pharmakon_context") {
                obj.insert(
                    "_pharmakon_context".to_string(),
                    serde_json::json!({
                        "tool_name": &self.name,
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    }),
                );
            }
        }

        let result: Value = self
            .client
            .call_tool(&self.name, final_args)
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        let elapsed = start.elapsed();
        log::info!(
            "MCP Tool {} finished in {}ms",
            self.name,
            elapsed.as_millis()
        );

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

pub struct ConnectMcpServerTool {
    pub tool_registry: Arc<Mutex<Vec<Arc<dyn Tool>>>>,
}

#[async_trait]
impl Tool for ConnectMcpServerTool {
    fn name(&self) -> &str {
        "connect_mcp_server"
    }
    fn description(&self) -> &str {
        "Connect to a new MCP server and register its tools dynamically."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "command": { "type": "string" },
                "args": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["name", "command", "args"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let name = args["name"].as_str().unwrap();
        let command = args["command"].as_str().unwrap();
        let cmd_args: Vec<String> = args["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap_or_default().to_string())
            .collect();
        let mut cmd = tokio::process::Command::new(command);
        cmd.args(&cmd_args);
        let client = McpClient::spawn_with_command(cmd)
            .await
            .map_err(|e| AgentError(e.to_string()))?;
        let client_arc = Arc::new(client);
        client_arc
            .initialize()
            .await
            .map_err(|e| AgentError(e.to_string()))?;
        let tools_list = client_arc
            .list_tools()
            .await
            .map_err(|e| AgentError(e.to_string()))?;
        let mut added = Vec::new();
        if let Some(tools_array) = tools_list.get("tools").and_then(|t| t.as_array()) {
            let mut registry = self.tool_registry.lock().await;
            for tool_val in tools_array {
                let tool_name = tool_val["name"].as_str().unwrap_or("unknown").to_string();
                let desc = tool_val["description"].as_str().unwrap_or("").to_string();
                let params = tool_val["inputSchema"].clone();
                registry.push(Arc::new(McpTool::new(
                    client_arc.clone(),
                    tool_name.clone(),
                    desc,
                    params,
                )));
                added.push(tool_name);
            }
        }
        Ok(format!(
            "Connected to {}. Added: {}",
            name,
            added.join(", ")
        ))
    }
}
