use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, ExecutionProfile, FilesystemScope, Reversibility, SideEffectLevel, Tool};
use serde_json::{Value, json};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

// ═══════════════════════════════════════════════════════════
// TerminalTool — persistent shell session
// ═══════════════════════════════════════════════════════════

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
                .map_err(|e| AgentError(format!("Failed to spawn shell: {}", e)))?;

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
        "Run a command in a persistent shell session. State (cwd, env vars) is preserved between calls. Supports cd, export, and interactive programs. Use for multi-step workflows."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Command to execute" },
                "reset": { "type": "boolean", "default": false, "description": "Kill and restart the session" },
                "timeout": { "type": "integer", "default": 30, "description": "Max seconds to wait for command to complete" },
                "workdir": { "type": "string", "description": "Change to this directory before running (cd + command)" },
                "requires_manual_approval": {
                    "type": "boolean",
                    "description": "Set to true if this is a high-risk command (rm, sudo, etc.)"
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
        let timeout_secs = args["timeout"].as_u64().unwrap_or(30).max(1).min(300);
        let timeout_duration = std::time::Duration::from_secs(timeout_secs);

        if args["dry_run"].as_bool().unwrap_or(false) {
            return Ok(format!("[DRY RUN] Would run in persistent terminal: {}", command));
        }

        if reset {
            let mut session_lock = self.session.lock().await;
            *session_lock = None;
        }

        let session_arc = self.ensure_session().await?;
        let mut session_lock = session_arc.lock().await;
        let session = session_lock.as_mut().unwrap();

        // Handle workdir: cd to workdir before running
        let full_command = if let Some(wd) = args["workdir"].as_str() {
            if !wd.is_empty() {
                format!("cd {} && {}", wd, command)
            } else {
                command.to_string()
            }
        } else {
            command.to_string()
        };

        // Send command with a unique completion marker
        let marker = format!("__PHARM_DONE_{}__", uuid::Uuid::new_v4().to_string().replace('-', ""));
        let cmd_line = format!("{}; echo \"{}\"\n", full_command, marker);

        session
            .stdin
            .write_all(cmd_line.as_bytes())
            .await
            .map_err(|e| AgentError(format!("Failed to write to terminal: {}", e)))?;
        session
            .stdin
            .flush()
            .await
            .map_err(|e| AgentError(format!("Failed to flush terminal: {}", e)))?;

        let mut output = String::new();
        let mut error_output = String::new();

        let result = tokio::time::timeout(timeout_duration, async {
            let mut stdout_buf = String::new();
            let mut stderr_buf = String::new();

            loop {
                tokio::select! {
                    res = session.stdout_reader.read_line(&mut stdout_buf) => {
                        match res {
                            Ok(0) | Err(_) => return Ok::<_, AgentError>(()), // EOF or error
                            Ok(_) => {
                                if stdout_buf.trim() == marker {
                                    return Ok(());
                                }
                                output.push_str(&stdout_buf);
                                stdout_buf.clear();
                            }
                        }
                    }
                    res = session.stderr_reader.read_line(&mut stderr_buf) => {
                        match res {
                            Ok(0) => {}
                            Ok(_) => {
                                error_output.push_str(&stderr_buf);
                                stderr_buf.clear();
                            }
                            Err(_) => {}
                        }
                    }
                }
            }
        })
        .await;

        match result {
            Ok(Ok(())) => {
                // Strip trailing newlines
                let output = output.trim_end().to_string();
                let error_output = error_output.trim_end().to_string();

                if error_output.is_empty() {
                    Ok(output)
                } else {
                    Ok(format!("{}\n--- stderr ---\n{}", output, error_output))
                }
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(AgentError(format!(
                "Command timed out after {}s. The command may still be running in the background.",
                timeout_secs
            ))),
        }
    }

    fn requires_approval(&self, args: &Value) -> bool {
        args["requires_manual_approval"].as_bool().unwrap_or(false)
    }
    fn approval_description(&self, args: &Value) -> String {
        format!(
            "Run in persistent terminal: {}",
            args["command"].as_str().unwrap_or("unknown")
        )
    }
    fn execution_profile(&self) -> ExecutionProfile {
        ExecutionProfile {
            side_effect_level: SideEffectLevel::Irreversible,
            network_access: true,
            filesystem_scope: FilesystemScope::Unrestricted,
            reversibility: Reversibility::Impractical,
            requires_human_approval: false,
        }
    }
}

// ═══════════════════════════════════════════════════════════
// ShellTool — single-shot shell command
// ═══════════════════════════════════════════════════════════

pub struct ShellTool;

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }
    fn description(&self) -> &str {
        "Execute a single shell command one-shot. Stateless — each call starts a fresh shell. Use terminal for stateful workflows (cd, env). 60s timeout."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Shell command to execute" },
                "timeout": { "type": "integer", "default": 60, "description": "Timeout in seconds (max 300)" },
                "workdir": { "type": "string", "description": "Working directory for the command" }
            },
            "required": ["command"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let command = args["command"]
            .as_str()
            .ok_or_else(|| AgentError("Missing command".to_string()))?;
        let timeout_secs = args["timeout"].as_u64().unwrap_or(60).max(1).min(300);
        let timeout_duration = std::time::Duration::from_secs(timeout_secs);

        if args["dry_run"].as_bool().unwrap_or(false) {
            return Ok(format!("[DRY RUN] Would run: {}", command));
        }

        let mut child = Command::new("sh");
        child
            .arg("-c")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // Set working directory if specified
        if let Some(wd) = args["workdir"].as_str().filter(|s| !s.is_empty()) {
            child.current_dir(wd);
        }

        let spawned = child.spawn()
            .map_err(|e| AgentError(format!("Failed to spawn shell: {}", e)))?;

        let result = tokio::time::timeout(timeout_duration, spawned.wait_with_output()).await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                if output.status.success() {
                    Ok(stdout)
                } else {
                    let code = output.status.code().unwrap_or(-1);
                    Ok(format!("Exit code: {}\nStdout: {}\nStderr: {}", code, stdout, stderr))
                }
            }
            Ok(Err(e)) => Err(AgentError(format!("Shell execution failed: {}", e))),
            Err(_) => Err(AgentError(format!(
                "Command timed out after {}s. Use terminal tool for long-running commands.", timeout_secs
            ))),
        }
    }

    fn execution_profile(&self) -> ExecutionProfile {
        ExecutionProfile {
            side_effect_level: SideEffectLevel::Irreversible,
            network_access: true,
            filesystem_scope: FilesystemScope::Unrestricted,
            reversibility: Reversibility::Impractical,
            requires_human_approval: false,
        }
    }
}

// ═══════════════════════════════════════════════════════════
// Background process management
// ═══════════════════════════════════════════════════════════

pub struct ManagedProcess {
    pub child: Mutex<Child>,
    pub stdin: Mutex<Option<tokio::process::ChildStdin>>,
    pub output_buffer: Arc<tokio::sync::RwLock<String>>,
}

static PROCESS_REGISTRY: std::sync::OnceLock<Arc<tokio::sync::RwLock<std::collections::HashMap<String, Arc<ManagedProcess>>>>> = std::sync::OnceLock::new();

fn get_process_registry() -> Arc<tokio::sync::RwLock<std::collections::HashMap<String, Arc<ManagedProcess>>>> {
    PROCESS_REGISTRY.get_or_init(|| Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()))).clone()
}

pub struct BackgroundRunTool;

impl BackgroundRunTool {
    pub fn new() -> Self { Self }
}
impl Default for BackgroundRunTool {
    fn default() -> Self { Self }
}

#[async_trait]
impl Tool for BackgroundRunTool {
    fn name(&self) -> &str {
        "run_background"
    }
    fn description(&self) -> &str {
        "Start a command in the background. Returns a handle for status checks and input via get_process_status and send_command_input. Good for servers, watchers, long builds."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Command to run in background" },
                "workdir": { "type": "string", "description": "Working directory" }
            },
            "required": ["command"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let command = args["command"].as_str().unwrap_or("");

        if command.is_empty() {
            return Err(AgentError("Empty command".to_string()));
        }

        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        if let Some(wd) = args["workdir"].as_str().filter(|s| !s.is_empty()) {
            cmd.current_dir(wd);
        }

        let mut child = cmd.spawn()
            .map_err(|e| AgentError(e.to_string()))?;

        let stdin = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let output_buffer = Arc::new(tokio::sync::RwLock::new(String::new()));

        // Spawn stdout reader
        let buffer = output_buffer.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            while let Ok(n) = reader.read_line(&mut line).await {
                if n == 0 { break; }
                buffer.write().await.push_str(&line);
                line.clear();
            }
        });

        // Spawn stderr reader (prefixed)
        let buffer = output_buffer.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            while let Ok(n) = reader.read_line(&mut line).await {
                if n == 0 { break; }
                buffer.write().await.push_str(&format!("[stderr] {}", line));
                line.clear();
            }
        });

        let id = uuid::Uuid::new_v4().to_string();
        let handle = format!("bg_{}", &id[..8]);

        let managed = Arc::new(ManagedProcess {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            output_buffer,
        });

        let registry = get_process_registry();
        registry.write().await.insert(handle.clone(), managed);

        Ok(format!("Started background process: {}\nHandle: {}", command, handle))
    }
}

pub struct ProcessStatusTool;

impl ProcessStatusTool {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl Tool for ProcessStatusTool {
    fn name(&self) -> &str {
        "get_process_status"
    }
    fn description(&self) -> &str {
        "Check status and read output of a background process started with run_background."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "handle": { "type": "string", "description": "Process handle from run_background" },
                "clear": { "type": "boolean", "default": true, "description": "Clear output buffer after reading" }
            },
            "required": ["handle"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let handle = args["handle"].as_str().unwrap_or("");
        let clear = args["clear"].as_bool().unwrap_or(true);

        if handle.is_empty() {
            // List all processes
            let registry = get_process_registry();
            let map = registry.read().await;
            let handles: Vec<String> = map.keys().cloned().collect();
            if handles.is_empty() {
                return Ok("No background processes running.".to_string());
            }
            return Ok(format!("Active background processes:\n{}", handles.join("\n")));
        }

        let registry = get_process_registry();
        let map = registry.read().await;

        let managed = map.get(handle)
            .ok_or_else(|| AgentError(format!("Process handle '{}' not found.", handle)))?;

        let mut child = managed.child.lock().await;
        let output = {
            let mut buf = managed.output_buffer.write().await;
            let text = buf.clone();
            if clear {
                buf.clear();
            }
            text
        };

        match child.try_wait() {
            Ok(Some(status)) => {
                let code = status.code().map(|c| c.to_string()).unwrap_or_else(|| "signal".to_string());
                Ok(format!(
                    "Process {} exited (code: {})\n--- output ---\n{}",
                    handle, code, output
                ))
            }
            Ok(None) => {
                Ok(format!(
                    "Process {} is running\n--- recent output ---\n{}",
                    handle,
                    if output.is_empty() { "(no output yet)" } else { &output }
                ))
            }
            Err(e) => Err(AgentError(format!("Error checking process: {}", e))),
        }
    }
}

pub struct SendCommandInputTool;

impl SendCommandInputTool {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl Tool for SendCommandInputTool {
    fn name(&self) -> &str {
        "send_command_input"
    }
    fn description(&self) -> &str {
        "Send stdin input to a running background process. Use for REPLs, prompts, or interactive programs."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "handle": { "type": "string", "description": "Process handle" },
                "input": { "type": "string", "description": "Input to send" },
                "wait_ms": { "type": "integer", "default": 500, "description": "Ms to wait for output after sending" }
            },
            "required": ["handle", "input"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let handle = args["handle"].as_str().unwrap_or("");
        let input = args["input"].as_str().unwrap_or("");
        let wait_ms = args["wait_ms"].as_u64().unwrap_or(500).min(30000);

        if handle.is_empty() || input.is_empty() {
            return Err(AgentError("Both handle and input are required.".to_string()));
        }

        let registry = get_process_registry();
        let map = registry.read().await;

        let managed = map.get(handle)
            .ok_or_else(|| AgentError(format!("Process handle '{}' not found.", handle)))?;

        let mut stdin_lock = managed.stdin.lock().await;
        let stdin = stdin_lock.as_mut()
            .ok_or_else(|| AgentError(format!("Process {} has no stdin (may already be closed).", handle)))?;

        // Auto-append newline if missing (common case: hitting Enter in a REPL)
        let final_input = if !input.ends_with('\n') {
            format!("{}\n", input)
        } else {
            input.to_string()
        };

        stdin.write_all(final_input.as_bytes()).await
            .map_err(|e| AgentError(format!("Failed to write to stdin: {}", e)))?;
        stdin.flush().await
            .map_err(|e| AgentError(format!("Failed to flush stdin: {}", e)))?;

        // Brief wait for output to accumulate
        tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;

        let mut buf = managed.output_buffer.write().await;
        let text = buf.clone();
        buf.clear();

        Ok(format!("Sent input to {}. Output:\n{}", handle, text))
    }
}
