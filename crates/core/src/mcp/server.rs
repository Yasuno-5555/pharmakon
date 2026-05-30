use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{self};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};

use crate::mcp::{McpRequest, McpResponse};

/// An MCP Server that exposes Pharmakon tools via the Model Context Protocol.
/// Communicates over stdio using JSON-RPC 2.0 (newline-delimited).
pub struct McpServer {
    tools: Vec<McpToolDef>,
}

#[derive(Clone)]
pub struct McpToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub handler: fn(Value) -> Result<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpInitializeResult {
    pub protocol_version: String,
    pub capabilities: McpCapabilities,
    pub server_info: McpServerInfo,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpCapabilities {
    pub tools: Option<McpToolsCapability>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpToolsCapability {}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub name: String,
    pub version: String,
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

impl McpServer {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    pub fn add_tool(&mut self, tool: McpToolDef) {
        self.tools.push(tool);
    }

    /// Run the MCP Server, reading from stdin and writing to stdout.
    pub async fn run(&self) -> Result<()> {
        let stdin = io::stdin();
        let stdout = io::stdout();
        let mut reader = BufReader::new(stdin);
        let mut writer = BufWriter::new(stdout);

        log::info!("MCP Server starting on stdio...");

        loop {
            let mut line = String::new();
            let bytes_read = reader.read_line(&mut line).await?;
            if bytes_read == 0 {
                break; // EOF
            }

            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let request: McpRequest = match serde_json::from_str(line) {
                Ok(r) => r,
                Err(e) => {
                    let error_response = json!({
                        "jsonrpc": "2.0",
                        "error": { "code": -32700, "message": format!("Parse error: {}", e) },
                        "id": null
                    });
                    let mut out = serde_json::to_string(&error_response)?;
                    out.push('\n');
                    writer.write_all(out.as_bytes()).await?;
                    writer.flush().await?;
                    continue;
                }
            };

            let response = self.handle_request(&request).await;
            let mut out = serde_json::to_string(&response)?;
            out.push('\n');
            writer.write_all(out.as_bytes()).await?;
            writer.flush().await?;
        }

        Ok(())
    }

    async fn handle_request(&self, request: &McpRequest) -> McpResponse {
        match request.method.as_str() {
            "initialize" => {
                let result = McpInitializeResult {
                    protocol_version: "2024-11-05".to_string(),
                    capabilities: McpCapabilities {
                        tools: Some(McpToolsCapability {}),
                    },
                    server_info: McpServerInfo {
                        name: "pharmakon".to_string(),
                        version: "0.1.0".to_string(),
                    },
                };
                McpResponse {
                    jsonrpc: "2.0".to_string(),
                    result: Some(serde_json::to_value(result).unwrap()),
                    error: None,
                    id: request.id,
                }
            }
            "notifications/initialized" => {
                // No response needed for notifications, but since we always respond:
                McpResponse {
                    jsonrpc: "2.0".to_string(),
                    result: Some(json!({})),
                    error: None,
                    id: request.id,
                }
            }
            "tools/list" => {
                let tools: Vec<Value> = self
                    .tools
                    .iter()
                    .map(|t| {
                        json!({
                            "name": t.name,
                            "description": t.description,
                            "inputSchema": t.input_schema,
                        })
                    })
                    .collect();
                McpResponse {
                    jsonrpc: "2.0".to_string(),
                    result: Some(json!({ "tools": tools })),
                    error: None,
                    id: request.id,
                }
            }
            "tools/call" => {
                let tool_name = request.params["name"].as_str().unwrap_or("");
                let arguments = request.params["arguments"].clone();

                if let Some(tool) = self.tools.iter().find(|t| t.name == tool_name) {
                    match (tool.handler)(arguments) {
                        Ok(result) => McpResponse {
                            jsonrpc: "2.0".to_string(),
                            result: Some(json!({
                                "content": [{ "type": "text", "text": result }]
                            })),
                            error: None,
                            id: request.id,
                        },
                        Err(e) => McpResponse {
                            jsonrpc: "2.0".to_string(),
                            result: Some(json!({
                                "content": [{ "type": "text", "text": format!("Error: {}", e) }],
                                "isError": true
                            })),
                            error: None,
                            id: request.id,
                        },
                    }
                } else {
                    McpResponse {
                        jsonrpc: "2.0".to_string(),
                        result: None,
                        error: Some(
                            json!({ "code": -32602, "message": format!("Unknown tool: {}", tool_name) }),
                        ),
                        id: request.id,
                    }
                }
            }
            _ => McpResponse {
                jsonrpc: "2.0".to_string(),
                result: None,
                error: Some(
                    json!({ "code": -32601, "message": format!("Method not found: {}", request.method) }),
                ),
                id: request.id,
            },
        }
    }
}
