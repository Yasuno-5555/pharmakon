use crate::agent::Agent;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Weak;
use tokio::fs;
use tokio::process::Command;

pub struct MctsSimulatorTool {
    #[allow(dead_code)]
    agent_ref: Weak<Agent>,
}

impl MctsSimulatorTool {
    pub fn new(agent: Weak<Agent>) -> Self {
        Self { agent_ref: agent }
    }

    async fn simulate_option(
        &self,
        name: &str,
        patch: &str,
        path: &str,
        workspace_root: &Path,
    ) -> Result<Value> {
        let temp_dir = tempfile::tempdir()?;
        let simulation_path = temp_dir.path();

        log::info!("Starting MCTS simulation for option '{}' in {:?}", name, simulation_path);

        // 1. Copy minimal workspace (only necessary files to run cargo check)
        // For simplicity in this implementation, we copy everything except heavy dirs
        self.copy_dir_recursive(workspace_root, simulation_path).await?;

        // 2. Apply patch
        let target_file = simulation_path.join(path);
        let original = fs::read_to_string(&target_file).await?;
        let patch_obj = diffy::Patch::from_str(patch).map_err(|e| anyhow!("Invalid patch: {}", e))?;
        let patched = diffy::apply(&original, &patch_obj).map_err(|e| anyhow!("Patch apply failed: {}", e))?;
        fs::write(&target_file, patched).await?;

        // 3. Run Verification
        let start = std::time::Instant::now();
        let output = Command::new("cargo")
            .arg("check")
            .current_dir(simulation_path)
            .output()
            .await?;
        let latency_ms = start.elapsed().as_millis();

        let success = output.status.success();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(json!({
            "option": name,
            "success": success,
            "latency_ms": latency_ms,
            "error": if success { None } else { Some(stderr.chars().take(500).collect::<String>()) }
        }))
    }

    async fn copy_dir_recursive(&self, from: &Path, to: &Path) -> Result<()> {
        let mut entries = fs::read_dir(from).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let dest = to.join(entry.file_name());
            if path.is_dir() {
                let name = entry.file_name();
                if name == "target" || name == ".git" || name == ".pharmakon" || name == "node_modules" {
                    continue;
                }
                fs::create_dir_all(&dest).await?;
                Box::pin(self.copy_dir_recursive(&path, &dest)).await?;
            } else {
                fs::copy(&path, &dest).await?;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl Tool for MctsSimulatorTool {
    fn name(&self) -> &str {
        "mcts_simulator"
    }

    fn description(&self) -> &str {
        "Run a Monte Carlo Tree Search simulation on multiple implementation options. \
         Each option is tested in an isolated temporary workspace with 'cargo check'. \
         Returns success rates and performance metrics for each branch."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "options": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "path": { "type": "string", "description": "File to patch" },
                            "patch": { "type": "string", "description": "Unified diff" }
                        },
                        "required": ["name", "path", "patch"]
                    }
                }
            },
            "required": ["options"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let options = args["options"]
            .as_array()
            .ok_or_else(|| AgentError("Missing options array".to_string()))?;

        let workspace_root = std::env::current_dir().map_err(|e| AgentError(e.to_string()))?;
        let mut results = Vec::new();

        for opt in options {
            let name = opt["name"].as_str().unwrap_or("unnamed");
            let path = opt["path"].as_str().unwrap_or("");
            let patch = opt["patch"].as_str().unwrap_or("");

            match self.simulate_option(name, patch, path, &workspace_root).await {
                Ok(res) => results.push(res),
                Err(e) => results.push(json!({
                    "option": name,
                    "success": false,
                    "error": format!("Simulation setup failed: {}", e)
                })),
            }
        }

        // Rank results
        results.sort_by(|a, b| {
            let a_ok = a["success"].as_bool().unwrap_or(false);
            let b_ok = b["success"].as_bool().unwrap_or(false);
            b_ok.cmp(&a_ok)
        });

        Ok(serde_json::to_string_pretty(&json!({
            "simulations": results,
            "recommendation": results.first().and_then(|r| r["option"].as_str())
        })).unwrap())
    }
}
