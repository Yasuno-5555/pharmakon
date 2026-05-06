use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool};
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

pub struct HostScriptTool;

#[async_trait]
impl Tool for HostScriptTool {
    fn name(&self) -> &str {
        "run_host_script"
    }
    fn description(&self) -> &str {
        "Execute a multi-line shell script directly on the host system. Use this as a fallback when Docker or other sandboxed environments are unavailable or restricted."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "script": { "type": "string", "description": "The full content of the shell script to execute." },
                "shell": { "type": "string", "default": "sh", "description": "The shell to use (e.g. sh, bash, zsh)." }
            },
            "required": ["script"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let script = args["script"]
            .as_str()
            .ok_or_else(|| AgentError("Missing script content".to_string()))?;
        let shell = args["shell"].as_str().unwrap_or("sh");

        // Create a temporary script file
        let temp_dir = std::env::temp_dir();
        let script_path = temp_dir.join(format!("pharmakon_script_{}.sh", uuid::Uuid::new_v4()));

        fs::write(&script_path, script)
            .map_err(|e| AgentError(format!("Failed to write temporary script: {}", e)))?;

        // Make it executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script_path)
                .map_err(|e| AgentError(e.to_string()))?
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script_path, perms).map_err(|e| AgentError(e.to_string()))?;
        }

        let output = Command::new(shell)
            .arg(&script_path)
            .output()
            .map_err(|e| AgentError(format!("Script execution failed: {}", e)))?;

        // Cleanup
        let _ = fs::remove_file(&script_path);

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            Ok(stdout)
        } else {
            Ok(format!(
                "Script failed with exit code: {:?}\nError: {}\nStdout: {}",
                output.status.code(),
                stderr,
                stdout
            ))
        }
    }
}
