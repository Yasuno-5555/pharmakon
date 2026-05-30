use crate::mcp::McpClient;
use crate::mcp_tool::McpTool;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct McpConfig {
    pub servers: Vec<McpServerConfig>,
}

pub struct McpManager;

impl McpManager {
    pub async fn load_tools() -> Result<Vec<Arc<dyn pharmakon_common::Tool>>> {
        let config_path = Self::get_config_path()?;

        if !config_path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(config_path)?;
        let config: McpConfig = serde_json::from_str(&content)?;

        let futures = config.servers.into_iter().map(|server_cfg| async move {
            let mut tools: Vec<Arc<dyn pharmakon_common::Tool>> = Vec::new();
            log::info!("Initializing MCP server: {}", server_cfg.name);
            let args: Vec<&str> = server_cfg.args.iter().map(|s| s.as_str()).collect();

            let mut cmd = tokio::process::Command::new(&server_cfg.command);
            cmd.args(&args);
            if let Some(env) = &server_cfg.env {
                cmd.envs(env);
            }

            let client: Arc<McpClient> = match McpClient::spawn_with_command(cmd).await {
                Ok(c) => Arc::new(c),
                Err(e) => {
                    log::error!("Failed to spawn MCP server {}: {}", server_cfg.name, e);
                    return tools;
                }
            };

            if let Err(e) = client.initialize().await {
                log::error!("Failed to initialize MCP server {}: {}", server_cfg.name, e);
                return tools;
            }

            match client.list_tools().await {
                Ok(tools_list) => {
                    if let Some(tools_array) = tools_list.get("tools").and_then(|t| t.as_array()) {
                        for tool_val in tools_array {
                            let name = tool_val["name"].as_str().unwrap_or("unknown").to_string();
                            let desc = tool_val["description"].as_str().unwrap_or("").to_string();
                            let params = tool_val["inputSchema"].clone();

                            log::info!("Registered MCP tool: {}/{}", server_cfg.name, name);
                            tools.push(Arc::new(McpTool::new(client.clone(), name, desc, params)));
                        }
                    }
                }
                Err(e) => {
                    log::error!(
                        "Failed to list tools for MCP server {}: {}",
                        server_cfg.name,
                        e
                    );
                }
            }
            tools
        });

        let results = futures::future::join_all(futures).await;
        let all_tools = results.into_iter().flatten().collect();
        Ok(all_tools)
    }

    fn get_config_path() -> Result<PathBuf> {
        let home = dirs::home_dir().expect("Could not find home directory");
        Ok(home.join(".pharmakon").join("mcp_servers.json"))
    }
}
