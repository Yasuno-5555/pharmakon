use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde_json::{Value, json};
use std::process::Command;

pub struct GitStatusTool;

#[async_trait]
impl Tool for GitStatusTool {
    fn name(&self) -> &str {
        "git_status"
    }
    fn description(&self) -> &str {
        "Show the working tree status. Essential for understanding pending changes."
    }
    fn parameters(&self) -> Value {
        json!({ "type": "object", "properties": {} })
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }
    async fn call(&self, _args: Value) -> AgentResult<String> {
        let out = Command::new("git")
            .arg("status")
            .output()
            .map_err(|e| AgentError(e.to_string()))?;
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    }
}

pub struct GitDiffTool;

#[async_trait]
impl Tool for GitDiffTool {
    fn name(&self) -> &str {
        "git_diff"
    }
    fn description(&self) -> &str {
        "Show changes between commits, commit and working tree, etc."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Specific file to diff" },
                "cached": { "type": "boolean", "default": false, "description": "Show staged changes" }
            }
        })
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }
    async fn call(&self, args: Value) -> AgentResult<String> {
        let mut cmd = Command::new("git");
        cmd.arg("diff");
        if args["cached"].as_bool().unwrap_or(false) {
            cmd.arg("--cached");
        }
        if let Some(path) = args["path"].as_str() {
            cmd.arg(path);
        }
        let out = cmd.output().map_err(|e| AgentError(e.to_string()))?;
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    }
}

pub struct GitCommitTool;

#[async_trait]
impl Tool for GitCommitTool {
    fn name(&self) -> &str {
        "git_commit"
    }
    fn description(&self) -> &str {
        "Record changes to the repository. AI should use this after significant progress."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "message": { "type": "string", "description": "Commit message" },
                "all": { "type": "boolean", "default": true, "description": "Stage all changes before commit" }
            },
            "required": ["message"]
        })
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }
    async fn call(&self, args: Value) -> AgentResult<String> {
        let message = args["message"]
            .as_str()
            .ok_or_else(|| AgentError("Missing message".to_string()))?;
        if args["all"].as_bool().unwrap_or(true) {
            Command::new("git").arg("add").arg("-A").output().ok();
        }
        let out = Command::new("git")
            .arg("commit")
            .arg("-m")
            .arg(message)
            .output()
            .map_err(|e| AgentError(e.to_string()))?;
        Ok(
            String::from_utf8_lossy(&out.stdout).to_string()
                + &String::from_utf8_lossy(&out.stderr),
        )
    }
}
