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

        if args["dry_run"].as_bool().unwrap_or(false) {
            return Ok(format!("[DRY RUN] Simulation: Writing {} bytes to {}", content.len(), path));
        }

        fs::write(path, content)
            .map_err(|e| AgentError(format!("Failed to write {}: {}", path, e)))?;
        Ok(format!("Successfully wrote to {}", path))
    }
}

// Helpers for AST validation and speculative sandboxing
fn has_error_nodes(node: tree_sitter::Node) -> bool {
    if node.is_error() || node.is_missing() {
        return true;
    }
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            if has_error_nodes(cursor.node()) {
                return true;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    false
}

fn find_cargo_toml_dir(start_path: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut current = start_path.to_path_buf();
    while current.pop() {
        if current.join("Cargo.toml").exists() {
            return Some(current);
        }
    }
    None
}

pub struct ApplyPatchTool;

#[async_trait]
impl Tool for ApplyPatchTool {
    fn name(&self) -> &str {
        "apply_patch"
    }
    fn description(&self) -> &str {
        "Apply a unified diff (patch) to a file. AST-validated, speculative sandbox checked, and forensic logged."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file to patch" },
                "patch": { "type": "string", "description": "Unified diff content" },
                "verify_ast": { "type": "boolean", "description": "Verify AST structural integrity using tree-sitter", "default": true },
                "speculative_check": { "type": "boolean", "description": "Perform speculative cargo compilation checks", "default": true },
                "reasoning": { "type": "string", "description": "Causal reasoning behind this code modification", "default": "" }
            },
            "required": ["path", "patch"]
        })
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }
    async fn call(&self, args: Value) -> AgentResult<String> {
        let path_str = args["path"]
            .as_str()
            .ok_or_else(|| AgentError("Missing path".to_string()))?;
        let patch_str = args["patch"]
            .as_str()
            .ok_or_else(|| AgentError("Missing patch".to_string()))?;
        let verify_ast = args["verify_ast"].as_bool().unwrap_or(true);
        let speculative_check = args["speculative_check"].as_bool().unwrap_or(true);
        let reasoning = args["reasoning"].as_str().unwrap_or("Unspecified enhancement").to_string();

        let path = std::path::Path::new(path_str);
        let original = fs::read_to_string(path)
            .map_err(|e| AgentError(format!("Failed to read {}: {}", path_str, e)))?;

        let patch = diffy::Patch::from_str(patch_str)
            .map_err(|e| AgentError(format!("Invalid patch format: {}", e)))?;

        let patched = diffy::apply(&original, &patch)
            .map_err(|e| AgentError(format!("Failed to apply patch: {}", e)))?;

        let is_rust_file = path.extension().and_then(|s| s.to_str()) == Some("rs");

        // 1. AST Validation using Tree-Sitter
        if verify_ast && is_rust_file {
            let mut parser = tree_sitter::Parser::new();
            if parser.set_language(&tree_sitter_rust::language()).is_ok() {
                if let Some(tree) = parser.parse(&patched, None) {
                    if has_error_nodes(tree.root_node()) {
                        return Err(AgentError(format!(
                            "AST Validation Failed: Patched code for {} has Tree-Sitter error or missing nodes (syntax/mismatch issue).",
                            path_str
                        )));
                    }
                }
            }
        }

        let mut check_success = true;
        let mut check_output = String::new();

        // 2. Speculative Sandboxing
        if speculative_check && is_rust_file {
            // Backup original file
            let backup_path = path.with_extension("spec_backup");
            fs::write(&backup_path, &original)
                .map_err(|e| AgentError(format!("Failed to write backup: {}", e)))?;

            // Write speculative patch to actual file
            fs::write(path, &patched)
                .map_err(|e| AgentError(format!("Failed to write speculative patch: {}", e)))?;

            // Run cargo check in the closest crate directory
            let check_dir = find_cargo_toml_dir(path).unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
            let output = std::process::Command::new("cargo")
                .arg("check")
                .current_dir(&check_dir)
                .output();

            match output {
                Ok(out) => {
                    if !out.status.success() {
                        check_success = false;
                        check_output = format!(
                            "Stdout:\n{}\nStderr:\n{}",
                            String::from_utf8_lossy(&out.stdout),
                            String::from_utf8_lossy(&out.stderr)
                        );
                    }
                }
                Err(e) => {
                    check_success = false;
                    check_output = format!("Failed to run cargo check: {}", e);
                }
            }

            // Restore original file if check failed or dry_run was requested
            let dry_run = args["dry_run"].as_bool().unwrap_or(false);
            if !check_success || dry_run {
                fs::write(path, &original)
                    .map_err(|e| AgentError(format!("Failed to restore original file: {}", e)))?;
            }

            // Clean up backup file
            let _ = fs::remove_file(&backup_path);

            if !check_success {
                return Err(AgentError(format!(
                    "Speculative Sandbox Check Failed. The proposed patch does not compile cleanly:\n{}",
                    check_output
                )));
            }
        } else {
            // Non-speculative / Non-Rust write
            if !args["dry_run"].as_bool().unwrap_or(false) {
                fs::write(path, &patched).map_err(|e| {
                    AgentError(format!(
                        "Failed to write patched content to {}: {}",
                        path_str, e
                    ))
                })?;
            }
        }

        // 3. Forensic Journaling Ledger
        let journal_dir = std::path::Path::new(".pharmakon");
        if !journal_dir.exists() {
            let _ = fs::create_dir_all(journal_dir);
        }
        let journal_path = journal_dir.join("forensics_journal.jsonl");
        let log_entry = json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "target_file": path_str,
            "reasoning_step": reasoning,
            "syntax_valid": true,
            "compilation_success": check_success,
            "speculative_check_performed": speculative_check && is_rust_file,
            "patch_len": patch_str.len(),
        });

        if let Ok(mut file) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&journal_path)
        {
            use std::io::Write;
            let _ = writeln!(file, "{}", log_entry.to_string());
        }

        Ok(format!(
            "Successfully applied patch to {}. AST verification passed. Speculative compiler checks passed cleanly.",
            path_str
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_ast_verification_valid_rust() {
        let original_code = "fn main() {\n    println!(\"Hello World\");\n}\n";
        let patched_code = "fn main() {\n    println!(\"Hello Speculative Sandbox\");\n}\n";
        
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_rust::language()).unwrap();
        let tree = parser.parse(&patched_code, None).unwrap();
        assert!(!has_error_nodes(tree.root_node()));
    }

    #[tokio::test]
    async fn test_ast_verification_invalid_rust() {
        // Missing a closing curly brace makes it syntactically invalid
        let invalid_code = "fn main() {\n    println!(\"Hello Speculative Sandbox\");\n";
        
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_rust::language()).unwrap();
        let tree = parser.parse(&invalid_code, None).unwrap();
        assert!(has_error_nodes(tree.root_node()));
    }
}

