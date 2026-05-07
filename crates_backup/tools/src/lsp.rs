use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool};
use serde_json::{Value, json};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

pub struct LspClient {
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<BufReader<ChildStdout>>,
    next_id: Mutex<i64>,
}

impl LspClient {
    pub async fn spawn(command: &str, args: &[&str]) -> Result<Self, std::io::Error> {
        let mut child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let stdin = child.stdin.take().expect("Failed to open stdin");
        let stdout = child.stdout.take().expect("Failed to open stdout");

        Ok(Self {
            stdin: Mutex::new(stdin),
            stdout: Mutex::new(BufReader::new(stdout)),
            next_id: Mutex::new(1),
        })
    }

    pub async fn call(&self, method: &str, params: Value) -> Result<Value, anyhow::Error> {
        let id = {
            let mut id_lock = self.next_id.lock().await;
            let current = *id_lock;
            *id_lock += 1;
            current
        };

        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        let req_str = serde_json::to_string(&request)?;
        let full_req = format!("Content-Length: {}\r\n\r\n{}", req_str.len(), req_str);

        {
            let mut stdin = self.stdin.lock().await;
            stdin.write_all(full_req.as_bytes()).await?;
            stdin.flush().await?;
        }

        loop {
            let mut line = String::new();
            let mut stdout = self.stdout.lock().await;
            stdout.read_line(&mut line).await?;
            if line.starts_with("Content-Length: ") {
                let len: usize = line["Content-Length: ".len()..].trim().parse()?;
                stdout.read_line(&mut line).await?; // Skip the empty line (\r\n)

                let mut body = vec![0u8; len];
                stdout.read_exact(&mut body).await?;
                let response: Value = serde_json::from_slice(&body)?;

                if response.get("id") == Some(&json!(id)) {
                    if let Some(error) = response.get("error") {
                        return Err(anyhow::anyhow!("LSP error: {}", error));
                    }
                    return Ok(response.get("result").cloned().unwrap_or(Value::Null));
                }
            }
        }
    }

    pub async fn initialize(&self, root_path: &str) -> Result<(), anyhow::Error> {
        let params = json!({
            "processId": std::process::id(),
            "rootPath": root_path,
            "rootUri": format!("file://{}", root_path),
            "capabilities": {
                "textDocument": {
                    "definition": { "dynamicRegistration": true },
                    "references": { "dynamicRegistration": true },
                    "hover": { "dynamicRegistration": true }
                }
            }
        });
        self.call("initialize", params).await?;
        self.call("initialized", json!({})).await?;
        Ok(())
    }
}

pub struct LspTool {
    client: Arc<Mutex<Option<Arc<LspClient>>>>,
}

impl Default for LspTool {
    fn default() -> Self {
        Self::new()
    }
}

impl LspTool {
    pub fn new() -> Self {
        Self {
            client: Arc::new(Mutex::new(None)),
        }
    }

    async fn get_client(&self) -> AgentResult<Arc<LspClient>> {
        let mut client_lock = self.client.lock().await;
        if let Some(client) = &*client_lock {
            return Ok(client.clone());
        }

        let lsp_cmd = "rust-analyzer";
        let client = LspClient::spawn(lsp_cmd, &[])
            .await
            .map_err(|e| AgentError(format!("Failed to spawn {}: {}", lsp_cmd, e)))?;

        let client_arc = Arc::new(client);
        let root = std::env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        client_arc
            .initialize(&root)
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        *client_lock = Some(client_arc.clone());
        Ok(client_arc)
    }
}

#[async_trait]
impl Tool for LspTool {
    fn name(&self) -> &str {
        "lsp"
    }
    fn description(&self) -> &str {
        "Query rust-analyzer for code intelligence: goto_definition, find_references, hover (type info). Works best for Rust projects."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["definition", "references", "hover"] },
                "file": { "type": "string", "description": "Absolute path to the file" },
                "line": { "type": "integer", "description": "1-indexed line number" },
                "column": { "type": "integer", "description": "1-indexed column number" }
            },
            "required": ["action", "file", "line", "column"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let action = args["action"].as_str().unwrap();
        let file_path = args["file"].as_str().unwrap();
        let line = args["line"].as_u64().unwrap() as u32 - 1; // 0-indexed for LSP
        let col = args["column"].as_u64().unwrap() as u32 - 1;

        let client = self.get_client().await?;
        let params = json!({
            "textDocument": { "uri": format!("file://{}", file_path) },
            "position": { "line": line, "character": col }
        });

        match action {
            "definition" => {
                let res = client
                    .call("textDocument/definition", params)
                    .await
                    .map_err(|e| AgentError(e.to_string()))?;
                Ok(serde_json::to_string_pretty(&res).unwrap())
            }
            "references" => {
                let mut ref_params = params.clone();
                ref_params["context"] = json!({ "includeDeclaration": true });
                let res = client
                    .call("textDocument/references", ref_params)
                    .await
                    .map_err(|e| AgentError(e.to_string()))?;
                Ok(serde_json::to_string_pretty(&res).unwrap())
            }
            "hover" => {
                let res = client
                    .call("textDocument/hover", params)
                    .await
                    .map_err(|e| AgentError(e.to_string()))?;
                Ok(serde_json::to_string_pretty(&res).unwrap())
            }
            _ => Err(AgentError("Unsupported LSP action".to_string())),
        }
    }
}
