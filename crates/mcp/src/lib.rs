pub mod server;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;

#[derive(Debug, Serialize, Deserialize)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: serde_json::Value,
    pub id: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpResponse {
    pub jsonrpc: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<serde_json::Value>,
    pub id: u64,
}

pub struct McpClient {
    stdin: Arc<Mutex<tokio::process::ChildStdin>>,
    stdout: Arc<Mutex<BufReader<tokio::process::ChildStdout>>>,
    next_id: Arc<Mutex<u64>>,
}

impl McpClient {
    pub async fn spawn(command: &str, args: &[&str]) -> Result<Self> {
        let mut cmd = Command::new(command);
        cmd.args(args);
        Self::spawn_with_command(cmd).await
    }

    pub async fn spawn_with_command(mut command: Command) -> Result<Self> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Stdin not captured"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Stdout not captured"))?;

        Ok(Self {
            stdin: Arc::new(Mutex::new(stdin)),
            stdout: Arc::new(Mutex::new(BufReader::new(stdout))),
            next_id: Arc::new(Mutex::new(1)),
        })
    }

    pub async fn initialize(&self) -> Result<serde_json::Value> {
        let params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "Pharmakon-Client",
                "version": "0.1.0"
            }
        });

        let result = self.call("initialize", params).await?;

        // Send notifications/initialized as per spec
        let id = {
            let mut id_lock = self.next_id.lock().await;
            let current = *id_lock;
            *id_lock += 1;
            current
        };
        let notification = McpRequest {
            jsonrpc: "2.0".to_string(),
            method: "notifications/initialized".to_string(),
            params: serde_json::json!({}),
            id,
        };
        let mut n_str = serde_json::to_string(&notification)?;
        n_str.push('\n');
        {
            let mut stdin = self.stdin.lock().await;
            stdin.write_all(n_str.as_bytes()).await?;
            stdin.flush().await?;
        }

        Ok(result)
    }

    pub async fn list_tools(&self) -> Result<serde_json::Value> {
        self.call("tools/list", serde_json::json!({})).await
    }

    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value> {
        self.call(
            "tools/call",
            serde_json::json!({
                "name": name,
                "arguments": arguments
            }),
        )
        .await
    }

    pub async fn call(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let id = {
            let mut id_lock = self.next_id.lock().await;
            let current = *id_lock;
            *id_lock += 1;
            current
        };

        let request = McpRequest {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
            id,
        };

        let mut req_str = serde_json::to_string(&request)?;
        req_str.push('\n');

        {
            let mut stdin = self.stdin.lock().await;
            stdin.write_all(req_str.as_bytes()).await?;
            stdin.flush().await?;
        }

        let mut line = String::new();
        {
            let mut stdout = self.stdout.lock().await;
            stdout.read_line(&mut line).await?;
        }

        let response: McpResponse = serde_json::from_str(&line)?;

        if let Some(error) = response.error {
            return Err(anyhow!("MCP error: {}", error));
        }

        response.result.ok_or_else(|| anyhow!("Empty MCP response"))
    }
}
