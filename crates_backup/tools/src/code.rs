use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde_json::{Value, json};
use std::fs;
use std::process::Command;

pub struct GrepSearchTool;

#[async_trait]
impl Tool for GrepSearchTool {
    fn name(&self) -> &str {
        "grep_search"
    }
    fn description(&self) -> &str {
        "Search for a pattern in the workspace files. Inspired by Antigravity/Claude grep."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "The search pattern" },
                "path": { "type": "string", "default": ".", "description": "Search directory" },
                "include": { "type": "string", "description": "File glob pattern" },
                "max_results": { "type": "integer", "default": 50, "description": "Limit the number of matches to save tokens" }
            },
            "required": ["query"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| AgentError("Missing query".to_string()))?;
        let path = args["path"].as_str().unwrap_or(".");
        let max_results = args["max_results"].as_u64().unwrap_or(50);

        let mut cmd = Command::new("grep");
        cmd.arg("-r")
            .arg("-n")
            .arg("-C")
            .arg("1")
            .arg(query)
            .arg(path);

        if let Some(include) = args["include"].as_str() {
            cmd.arg("--include").arg(include);
        }

        let output = cmd
            .output()
            .map_err(|e| AgentError(format!("Grep failed: {}", e)))?;
        let result = String::from_utf8_lossy(&output.stdout).to_string();

        if result.is_empty() {
            Ok("No matches found.".to_string())
        } else {
            let lines: Vec<&str> = result.lines().collect();
            if lines.len() as u64 > max_results {
                let truncated = lines[..max_results as usize].join("\n");
                Ok(format!(
                    "{}\n\n... (Truncated {} lines for token efficiency)",
                    truncated,
                    lines.len() as u64 - max_results
                ))
            } else {
                Ok(result)
            }
        }
    }
}

pub struct CodeEditTool;

#[async_trait]
impl Tool for CodeEditTool {
    fn name(&self) -> &str {
        "edit_file"
    }
    fn description(&self) -> &str {
        "Apply a single search-and-replace edit. Equivalent to Antigravity's replace_file_content."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file" },
                "old_content": { "type": "string", "description": "Exact text to find" },
                "new_content": { "type": "string", "description": "Replacement text" }
            },
            "required": ["path", "old_content", "new_content"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| AgentError("Missing path".to_string()))?;
        let old = args["old_content"]
            .as_str()
            .ok_or_else(|| AgentError("Missing old_content".to_string()))?;
        let new = args["new_content"]
            .as_str()
            .ok_or_else(|| AgentError("Missing new_content".to_string()))?;

        let content = fs::read_to_string(path)
            .map_err(|e| AgentError(format!("Failed to read file: {}", e)))?;
        if !content.contains(old) {
            return Err(AgentError(format!("Old content not found in: {}", path)));
        }
        let new_content = content.replace(old, new);
        fs::write(path, new_content).map_err(|e| AgentError(format!("Failed to write: {}", e)))?;
        Ok(format!("Updated {}", path))
    }
}

pub struct MultiCodeEditTool;

#[async_trait]
impl Tool for MultiCodeEditTool {
    fn name(&self) -> &str {
        "multi_edit_file"
    }
    fn description(&self) -> &str {
        "Apply multiple search-and-replace edits. Equivalent to Antigravity's multi_replace_file_content."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file" },
                "edits": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "old_content": { "type": "string", "description": "Text to find" },
                            "new_content": { "type": "string", "description": "Replacement" }
                        },
                        "required": ["old_content", "new_content"]
                    }
                }
            },
            "required": ["path", "edits"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| AgentError("Missing path".to_string()))?;
        let edits = args["edits"]
            .as_array()
            .ok_or_else(|| AgentError("Missing edits".to_string()))?;
        let mut content =
            fs::read_to_string(path).map_err(|e| AgentError(format!("Failed to read: {}", e)))?;
        for edit in edits {
            let old = edit["old_content"].as_str().unwrap_or_default();
            let new = edit["new_content"].as_str().unwrap_or_default();
            if !content.contains(old) {
                return Err(AgentError(format!("Chunk not found in: {}", path)));
            }
            content = content.replace(old, new);
        }
        fs::write(path, content).map_err(|e| AgentError(format!("Failed to write: {}", e)))?;
        Ok(format!("Applied {} edits to {}", edits.len(), path))
    }
}

pub struct ListDirTool;

#[async_trait]
impl Tool for ListDirTool {
    fn name(&self) -> &str {
        "list_dir"
    }
    fn description(&self) -> &str {
        "List directory contents with metadata. Equivalent to Antigravity/Claude Code list_dir."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "default": ".", "description": "Target directory" }
            }
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let path = args["path"].as_str().unwrap_or(".");
        let entries =
            fs::read_dir(path).map_err(|e| AgentError(format!("Failed to read dir: {}", e)))?;
        let mut report = format!("### Listing: {}\n", path);
        for e in entries.flatten() {
            let m = e.metadata().map_err(|e| AgentError(e.to_string()))?;
            let name = e.file_name().to_string_lossy().to_string();
            let kind = if m.is_dir() { "DIR " } else { "FILE" };
            report.push_str(&format!(
                "- [{}] {}{}\n",
                kind,
                name,
                if m.is_file() {
                    format!(" ({}b)", m.len())
                } else {
                    "".to_string()
                }
            ));
        }
        Ok(report)
    }
}

pub struct ViewFileTool;

#[async_trait]
impl Tool for ViewFileTool {
    fn name(&self) -> &str {
        "view_file"
    }
    fn description(&self) -> &str {
        "Read file contents with line numbers. Equivalent to Antigravity's view_file."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to file" },
                "start_line": { "type": "integer", "default": 1 },
                "end_line": { "type": "integer" },
                "view_skeleton": { "type": "boolean", "default": false, "description": "Only show signatures (structs/functions) to save tokens" }
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
        let start = args["start_line"].as_u64().unwrap_or(1) as usize;
        let end = args["end_line"].as_u64().unwrap_or(start as u64 + 100) as usize;

        let view_skeleton = args["view_skeleton"].as_bool().unwrap_or(false);

        let content =
            fs::read_to_string(path).map_err(|e| AgentError(format!("Read failed: {}", e)))?;

        if view_skeleton {
            let skeleton = pharmakon_common::CodeUtils::skeletonize_code(&content);
            return Ok(format!(
                "### Skeleton: {} (Full file: {} lines)\n\n{}\n\n[Note: This is a structural skeleton. Use view_file without view_skeleton to see full implementations.]",
                path,
                content.lines().count(),
                skeleton
            ));
        }

        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();

        let mut result = format!("### File: {} (Total lines: {})\n\n", path, total);
        let actual_end = end.min(total);
        for i in (start.max(1) - 1)..actual_end {
            result.push_str(&format!("{:4}: {}\n", i + 1, lines[i]));
        }

        if actual_end < total && end >= actual_end {
            result.push_str("\n... (More lines available, use a higher start_line to see more)");
        }

        Ok(result)
    }
}

pub struct FindDefinitionTool;

#[async_trait]
impl Tool for FindDefinitionTool {
    fn name(&self) -> &str {
        "find_definition"
    }
    fn description(&self) -> &str {
        "Search for the definition of a function, struct, or type using pattern matching. LSP-lite capability."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "The name of the symbol to find" },
                "language": { "type": "string", "enum": ["rust", "python", "javascript"], "default": "rust" }
            },
            "required": ["name"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let name = args["name"]
            .as_str()
            .ok_or_else(|| AgentError("Missing name".to_string()))?;
        let lang = args["language"].as_str().unwrap_or("rust");

        let pattern = match lang {
            "rust" => format!("(fn|struct|enum|trait|type)\\s+{}", name),
            "python" => format!("(def|class)\\s+{}", name),
            "javascript" | "typescript" => {
                format!("(function|class|const|let|var|type|interface)\\s+{}", name)
            }
            _ => name.to_string(),
        };

        let out = Command::new("grep")
            .arg("-r")
            .arg("-n")
            .arg("-E")
            .arg(&pattern)
            .arg(".")
            .output()
            .map_err(|e| AgentError(e.to_string()))?;

        let result = String::from_utf8_lossy(&out.stdout).to_string();
        if result.is_empty() {
            Ok(format!("No definition found for '{}'", name))
        } else {
            Ok(result)
        }
    }
}

pub struct PythonInterpreterTool;

#[async_trait]
impl Tool for PythonInterpreterTool {
    fn name(&self) -> &str {
        "python_interpreter"
    }
    fn description(&self) -> &str {
        "Execute a Python script and return its output. Useful for data analysis, complex calculations, or scriptable logic."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "code": { "type": "string", "description": "The Python code to execute" }
            },
            "required": ["code"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let code = args["code"]
            .as_str()
            .ok_or_else(|| AgentError("Missing code".to_string()))?;

        // Preamble for CodeAct bridge
        let preamble = r#"
import os, sys, json, subprocess

class Pharmakon:
    def read_file(self, path, start=1, end=100):
        try:
            with open(path, 'r') as f:
                lines = f.readlines()
                return "".join(lines[start-1:end])
        except Exception as e:
            return f"Error: {e}"

    def list_dir(self, path='.'):
        try:
            return os.listdir(path)
        except Exception as e:
            return f"Error: {e}"

    def grep(self, query, path='.'):
        try:
            res = subprocess.check_output(['grep', '-rn', query, path], text=True)
            return res
        except:
            return "No matches."

pharmakon = Pharmakon()
"#;

        let full_code = format!("{}\n{}", preamble, code);

        let output = std::process::Command::new("python3")
            .arg("-c")
            .arg(full_code)
            .output()
            .map_err(|e| AgentError(format!("Python failed: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            Ok(stdout)
        } else {
            Ok(format!("Error: {}\nStdout: {}", stderr, stdout))
        }
    }
}

pub struct ApplyPatchTool;

#[async_trait]
impl Tool for ApplyPatchTool {
    fn name(&self) -> &str {
        "apply_patch"
    }
    fn description(&self) -> &str {
        "Apply a unified diff patch to the workspace. This is the preferred way to edit files for precision and token efficiency."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "patch": { "type": "string", "description": "The unified diff content to apply" }
            },
            "required": ["patch"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let patch = args["patch"]
            .as_str()
            .ok_or_else(|| AgentError("Missing patch content".to_string()))?;

        // Write the patch to a temporary file
        let patch_id = uuid::Uuid::new_v4().to_string();
        let patch_path = format!("/tmp/pharmakon_{}.patch", patch_id);
        fs::write(&patch_path, patch)
            .map_err(|e| AgentError(format!("Failed to write temp patch: {}", e)))?;

        // Apply the patch using the system 'patch' command
        let output = Command::new("patch")
            .arg("-p1") // Standard -p1 for unified diffs from repo root
            .arg("-i")
            .arg(&patch_path)
            .output()
            .map_err(|e| AgentError(format!("Patch execution failed: {}", e)))?;

        // Clean up
        let _ = fs::remove_file(&patch_path);

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            Ok(format!("Successfully applied patch:\n{}", stdout))
        } else {
            Err(AgentError(format!(
                "Failed to apply patch:\nStdout: {}\nStderr: {}",
                stdout, stderr
            )))
        }
    }
}
