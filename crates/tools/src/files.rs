use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde_json::{Value, json};
use std::fs;

pub struct FileReadTool;

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str {
        "read_file"
    }
    fn description(&self) -> &str {
        "Read the entire content of a file. Use view_file for large files with line numbers."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file" }
            },
            "required": ["path"]
        })
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }
    async fn call(&self, args: Value) -> AgentResult<String> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| AgentError("Missing path".to_string()))?;
        fs::read_to_string(path).map_err(|e| AgentError(format!("Failed to read {}: {}", path, e)))
    }
}

pub struct FileWriteTool;

#[async_trait]
impl Tool for FileWriteTool {
    fn name(&self) -> &str {
        "write_file"
    }
    fn description(&self) -> &str {
        "DEPRECATED: Write full content to a file. Use apply_patch instead for safer edits."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file" },
                "content": { "type": "string", "description": "Full content to write" }
            },
            "required": ["path", "content"]
        })
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }
    async fn call(&self, args: Value) -> AgentResult<String> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| AgentError("Missing path".to_string()))?;
        let content = args["content"]
            .as_str()
            .ok_or_else(|| AgentError("Missing content".to_string()))?;
        fs::write(path, content)
            .map_err(|e| AgentError(format!("Failed to write {}: {}", path, e)))?;
        Ok(format!("Successfully wrote to {}", path))
    }
}

pub struct ApplyPatchTool;

#[async_trait]
impl Tool for ApplyPatchTool {
    fn name(&self) -> &str {
        "apply_patch"
    }
    fn description(&self) -> &str {
        "Apply a unified diff (patch) to a file. Preferred way to modify code."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file to patch" },
                "patch": { "type": "string", "description": "Unified diff content" }
            },
            "required": ["path", "patch"]
        })
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }
    async fn call(&self, args: Value) -> AgentResult<String> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| AgentError("Missing path".to_string()))?;
        let patch_str = args["patch"]
            .as_str()
            .ok_or_else(|| AgentError("Missing patch".to_string()))?;

        let original = fs::read_to_string(path)
            .map_err(|e| AgentError(format!("Failed to read {}: {}", path, e)))?;

        let patch = diffy::Patch::from_str(patch_str)
            .map_err(|e| AgentError(format!("Invalid patch format: {}", e)))?;

        let patched = diffy::apply(&original, &patch)
            .map_err(|e| AgentError(format!("Failed to apply patch: {}", e)))?;

        fs::write(path, patched)
            .map_err(|e| AgentError(format!("Failed to write patched content to {}: {}", path, e)))?;

        Ok(format!("Successfully applied patch to {}", path))
    }
}
