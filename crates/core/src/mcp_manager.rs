use anyhow::Result;
use pharmakon_mcp::McpClient;
use pharmakon_tools::mcp_tool::McpTool;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

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
    pub async fn load_tools() -> Result<Vec<Arc<dyn pharmakon_tools::Tool>>> {
        let mut tools: Vec<Arc<dyn pharmakon_tools::Tool>> = Vec::new();
        let config_path = Self::get_config_path()?;
        
        if !config_path.exists() {
            return Ok(tools);
        }

        let content = fs::read_to_string(config_path)?;
        let config: McpConfig = serde_json::from_str(&content)?;

        for server_cfg in config.servers {
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
                    continue;
                }
            };

            if let Err(e) = client.initialize().await {
                log::error!("Failed to initialize MCP server {}: {}", server_cfg.name, e);
                continue;
            }

            match client.list_tools().await {
                Ok(tools_list) => {
                    if let Some(tools_array) = tools_list.get("tools").and_then(|t| t.as_array()) {
                        for tool_val in tools_array {
                            let name = tool_val["name"].as_str().unwrap_or("unknown").to_string();
                            let desc = tool_val["description"].as_str().unwrap_or("").to_string();
                            let params = tool_val["inputSchema"].clone();
                            
                            log::info!("Registered MCP tool: {}/{}", server_cfg.name, name);
                            tools.push(Arc::new(McpTool::new(
                                client.clone(),
                                name,
                                desc,
                                params
                            )));
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to list tools for MCP server {}: {}", server_cfg.name, e);
                }
            }
        }

        Ok(tools)
    }

    fn get_config_path() -> Result<PathBuf> {
        let home = dirs::home_dir().expect("Could not find home directory");
        Ok(home.join(".pharmakon").join("mcp_servers.json"))
    }
}
