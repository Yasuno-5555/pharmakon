use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool};
use serde_json::{Value, json};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

pub struct TerminalTool {
    session: Arc<Mutex<Option<TerminalSession>>>,
}

struct TerminalSession {
    #[allow(dead_code)]
    child: Child,
    stdin: tokio::process::ChildStdin,
    stdout_reader: BufReader<tokio::process::ChildStdout>,
    stderr_reader: BufReader<tokio::process::ChildStderr>,
}

impl Default for TerminalTool {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalTool {
    pub fn new() -> Self {
        Self {
            session: Arc::new(Mutex::new(None)),
        }
    }

    async fn ensure_session(&self) -> AgentResult<Arc<Mutex<Option<TerminalSession>>>> {
        let mut session_lock = self.session.lock().await;
        if session_lock.is_none() {
            log::info!("Starting persistent terminal session...");
            let mut child = Command::new("sh")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| AgentError(e.to_string()))?;

            let stdin = child
                .stdin
                .take()
                .ok_or_else(|| AgentError("Failed to open stdin".to_string()))?;
            let stdout = child
                .stdout
                .take()
                .ok_or_else(|| AgentError("Failed to open stdout".to_string()))?;
            let stderr = child
                .stderr
                .take()
                .ok_or_else(|| AgentError("Failed to open stderr".to_string()))?;

            *session_lock = Some(TerminalSession {
                child,
                stdin,
                stdout_reader: BufReader::new(stdout),
                stderr_reader: BufReader::new(stderr),
            });
        }
        Ok(self.session.clone())
    }
}

#[async_trait]
impl Tool for TerminalTool {
    fn name(&self) -> &str {
        "terminal"
    }
    fn description(&self) -> &str {
        "Execute a command in a persistent terminal session. Allows maintaining state between calls."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Command to execute" },
                "reset": { "type": "boolean", "default": false, "description": "Clear the session and start fresh" },
                "timeout": { "type": "integer", "default": 30, "description": "Timeout in seconds" },
                "requires_manual_approval": {
                    "type": "boolean",
                    "description": "Set to true if you judge this command is high-risk and requires user confirmation."
                }
            },
            "required": ["command"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let command = args["command"]
            .as_str()
            .ok_or_else(|| AgentError("Missing command".to_string()))?;
        let reset = args["reset"].as_bool().unwrap_or(false);
        let timeout_secs = args["timeout"].as_u64().unwrap_or(30);
        let timeout_duration = std::time::Duration::from_secs(timeout_secs);

        if args["dry_run"].as_bool().unwrap_or(false) {
            return Ok(format!("[DRY RUN] Simulation of persistent terminal command: {}", command));
        }

        if reset {
            let mut session_lock = self.session.lock().await;
            *session_lock = None;
            log::info!("Terminal session reset requested.");
        }

        let session_arc = self.ensure_session().await?;
        let mut session_lock = session_arc.lock().await;
        let session = session_lock.as_mut().unwrap();

        // Send command with a unique marker to know when it finishes
        let marker = format!("MARKER_{}", uuid::Uuid::new_v4());
        let full_command = format!("{}; echo {}\n", command, marker);

        session
            .stdin
            .write_all(full_command.as_bytes())
            .await
            .map_err(|e| AgentError(e.to_string()))?;
        session
            .stdin
            .flush()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        let mut output = String::new();
        let mut error_output = String::new();

        let result = tokio::time::timeout(timeout_duration, async {
            loop {
                let mut stdout_line = String::new();
                let mut stderr_line = String::new();

                tokio::select! {
                    res = session.stdout_reader.read_line(&mut stdout_line) => {
                        if res.unwrap_or(0) > 0 {
                            if stdout_line.trim() == marker { return Ok::<(), anyhow::Error>(()); }
                            output.push_str(&stdout_line);
                        } else { return Ok::<(), anyhow::Error>(()); }
                    }
                    res = session.stderr_reader.read_line(&mut stderr_line) => {
                        if res.unwrap_or(0) > 0 {
                            error_output.push_str(&stderr_line);
                        }
                    }
                }
            }
        })
        .await;

        match result {
            Ok(_) => {
                if error_output.is_empty() {
                    Ok(output)
                } else {
                    Ok(format!("{}\n[Errors]:\n{}", output, error_output))
                }
            }
            Err(_) => Err(AgentError(format!("Command timed out after {} seconds", timeout_secs))),
        }
    }

    fn requires_approval(&self, args: &Value) -> bool {
        args["requires_manual_approval"].as_bool().unwrap_or(false)
    }
    fn approval_description(&self, args: &Value) -> String {
        format!(
            "Run terminal command (Agent Flagged Risk): {}",
            args["command"].as_str().unwrap_or("unknown")
        )
    }
}

pub struct ShellTool;

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }
    fn description(&self) -> &str {
        "Execute a single shell command and return the output. Use for one-off tasks like listing files or checking status."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Command to execute" }
            },
            "required": ["command"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let command = args["command"]
            .as_str()
            .ok_or_else(|| AgentError("Missing command".to_string()))?;

        if args["dry_run"].as_bool().unwrap_or(false) {
            return Ok(format!("[DRY RUN] Simulation of shell command: {}", command));
        }

        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .output()
            .map_err(|e| AgentError(format!("Shell execution failed: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            Ok(stdout)
        } else {
            Ok(format!("Error: {}\nStdout: {}", stderr, stdout))
        }
    }
}

pub struct BackgroundRunTool {
    pub active_processes: Arc<Mutex<std::collections::HashMap<String, Child>>>,
}

impl BackgroundRunTool {
    pub fn new() -> Self {
        Self {
            active_processes: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }
}

impl Default for BackgroundRunTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for BackgroundRunTool {
    fn name(&self) -> &str {
        "run_background"
    }
    fn description(&self) -> &str {
        "Start a command in the background. Returns a process ID (handle) that can be used to track it."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Command to run" },
                "log_file": { "type": "string", "description": "Optional file to redirect output to" }
            },
            "required": ["command"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let command = args["command"].as_str().unwrap();
        let log_file = args["log_file"].as_str();

        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(command);

        if let Some(path) = log_file {
            let file = std::fs::File::create(path).map_err(|e| AgentError(e.to_string()))?;
            cmd.stdout(Stdio::from(
                file.try_clone().map_err(|e| AgentError(e.to_string()))?,
            ));
            cmd.stderr(Stdio::from(file));
        } else {
            cmd.stdout(Stdio::null());
            cmd.stderr(Stdio::null());
        }

        let child = cmd.spawn().map_err(|e| AgentError(e.to_string()))?;
        let id = uuid::Uuid::new_v4().to_string();
        let handle = format!("bg_{}", id);

        let mut procs = self.active_processes.lock().await;
        procs.insert(handle.clone(), child);

        Ok(format!(
            "Started background process with handle: {}. Max auto-fix retries: 3.",
            handle
        ))
    }
}

pub struct ProcessStatusTool {
    pub active_processes: Arc<Mutex<std::collections::HashMap<String, Child>>>,
    pub retry_counts: Arc<Mutex<std::collections::HashMap<String, u32>>>,
}

#[async_trait]
impl Tool for ProcessStatusTool {
    fn name(&self) -> &str {
        "get_process_status"
    }
    fn description(&self) -> &str {
        "Check the status of a background process. Monitors for errors and manages retry limits."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "handle": { "type": "string", "description": "Process handle" }
            },
            "required": ["handle"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let handle = args["handle"].as_str().unwrap();
        let mut procs = self.active_processes.lock().await;

        if let Some(child) = procs.get_mut(handle) {
            match child.try_wait() {
                Ok(Some(status)) => {
                    if status.success() {
                        Ok(format!("Process {} finished successfully.", handle))
                    } else {
                        let mut retries = self.retry_counts.lock().await;
                        let count = retries.entry(handle.to_string()).or_insert(0);
                        *count += 1;

                        if *count > 3 {
                            Ok(format!(
                                "CRITICAL: Process {} failed with status {}. RETRY LIMIT EXCEEDED ({}). HUMAN INTERVENTION REQUIRED.",
                                handle, status, *count
                            ))
                        } else {
                            Ok(format!(
                                "ERROR: Process {} failed with status {}. Retry count: {}/3. You may attempt ONE more fix.",
                                handle, status, *count
                            ))
                        }
                    }
                }
                Ok(None) => Ok(format!("Process {} is still running.", handle)),
                Err(e) => Err(AgentError(format!("Error checking process: {}", e))),
            }
        } else {
            Err(AgentError(format!("Process handle {} not found.", handle)))
        }
    }
}
