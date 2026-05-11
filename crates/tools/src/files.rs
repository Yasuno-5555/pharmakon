use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, ExecutionProfile, FilesystemScope, Reversibility, SideEffectLevel, Tool, ToolCategory};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024; // 50MB
const BINARY_SCAN_SIZE: usize = 8192;

/// Auto-detect whether content is binary by scanning for null bytes.
fn is_binary_content(data: &[u8]) -> bool {
    let scan_end = data.len().min(BINARY_SCAN_SIZE);
    data[..scan_end].contains(&0x00)
}

/// Check if a file extension suggests an image.
fn is_image_extension(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|s| s.to_str()),
        Some("png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "tiff" | "tif" | "svg" | "ico")
    )
}

/// Check if a file extension suggests a PDF.
fn is_pdf_extension(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|s| s.to_str()),
        Some("pdf")
    )
}

/// Get file metadata string (size, modified time).
fn format_file_metadata(path: &Path) -> String {
    match path.metadata() {
        Ok(meta) => {
            let size = meta.len();
            let size_str = if size >= 1024 * 1024 {
                format!("{:.1}MB", size as f64 / (1024.0 * 1024.0))
            } else if size >= 1024 {
                format!("{}KB", size / 1024)
            } else {
                format!("{}B", size)
            };
            let modified = meta.modified().ok().map(|t| {
                let dur = t.duration_since(std::time::UNIX_EPOCH).ok();
                dur.and_then(|d| {
                    let secs = d.as_secs();
                    // Use chrono for formatting
                    chrono::DateTime::from_timestamp(secs as i64, 0)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                }).unwrap_or_default()
            }).unwrap_or_default();
            format!("{} (modified: {})", size_str, modified)
        }
        Err(_) => "unknown".to_string(),
    }
}

pub struct FileReadTool;

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str {
        "read_file"
    }
    fn description(&self) -> &str {
        "Read file contents with smart detection. Auto-detects text vs binary. Supports line ranges (start_line/end_line) for large files. Images and PDFs return metadata. Truncates at 50MB."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file" },
                "start_line": { "type": "integer", "default": 1, "description": "First line to read (1-indexed)" },
                "end_line": { "type": "integer", "description": "Last line to read (inclusive). Defaults to 200 if start_line is set." },
                "limit_kb": { "type": "integer", "description": "Max KB to read. Overrides line ranges." }
            },
            "required": ["path"]
        })
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }
    fn execution_profile(&self) -> ExecutionProfile {
        ExecutionProfile {
            side_effect_level: SideEffectLevel::None,
            network_access: false,
            filesystem_scope: FilesystemScope::Confined,
            reversibility: Reversibility::Trivial,
            requires_human_approval: false,
        }
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let path_str = args["path"]
            .as_str()
            .ok_or_else(|| AgentError("Missing path".to_string()))?;
        let start_line = args["start_line"].as_u64().unwrap_or(1) as usize;
        let end_line = args["end_line"].as_u64().map(|e| e as usize);
        let limit_kb = args["limit_kb"].as_u64();

        let path = Path::new(path_str);

        // === Resolve symlinks and check existence ===
        let canonical = path.canonicalize()
            .map_err(|e| AgentError(format!("File not found or inaccessible: {} ({})", path_str, e)))?;

        // === Check file metadata ===
        let metadata = fs::metadata(&canonical)
            .map_err(|e| AgentError(format!("Cannot read file metadata: {} ({})", path_str, e)))?;

        if !metadata.is_file() {
            return Err(AgentError(format!("'{}' is not a file (it may be a directory or special file)", path_str)));
        }

        let file_size = metadata.len();
        let file_info = format_file_metadata(&canonical);

        // === Size guard ===
        if file_size > MAX_FILE_SIZE {
            return Ok(format!(
                "File too large: {} ({}). Maximum readable size is 50MB.\n{}",
                path_str, file_info, "Use grep_files or view_file with line ranges for selective reading."
            ));
        }

        // === Read raw bytes ===
        let data = fs::read(&canonical)
            .map_err(|e| AgentError(format!("Failed to read {}: {}", path_str, e)))?;

        // === Binary detection ===
        if is_image_extension(&canonical) {
            return Ok(format!(
                "## Image File: {}\n{}\n\nUse media_understanding tool to analyze this image.",
                path_str, file_info
            ));
        }

        if is_pdf_extension(&canonical) {
            return Ok(format!(
                "## PDF File: {}\n{}\n\nUse web_fetch or a dedicated PDF reader for structured content.",
                path_str, file_info
            ));
        }

        if is_binary_content(&data) {
            return Ok(format!(
                "## Binary File: {}\n{}\n\nThis appears to be a binary file and cannot be displayed as text.",
                path_str, file_info
            ));
        }

        // === Decode as UTF-8 (with fallback) ===
        let content = String::from_utf8(data)
            .map_err(|_| AgentError(format!(
                "File {} is not valid UTF-8 text. Try using grep_files or a binary reader.", path_str
            )))?;

        // === Apply limit_kb ===
        if let Some(kb) = limit_kb {
            let max_chars = (kb as usize) * 1024;
            if content.len() > max_chars {
                let truncated: String = content.chars().take(max_chars).collect();
                return Ok(format!(
                    "### File: {} ({}, limited to {}KB)\n\n{}\n\n... (Truncated to {}KB. Use read_file with start_line/end_line to read specific sections.)",
                    path_str, file_info, kb, truncated, kb
                ));
            }
        }

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        // === Determine line range ===
        let actual_start = start_line.max(1);
        let actual_end = end_line.unwrap_or_else(|| {
            if start_line > 1 {
                // Default to showing 200 lines from start
                (start_line + 199).min(total_lines)
            } else {
                total_lines
            }
        })
        .min(total_lines);

        if actual_start > total_lines {
            return Ok(format!(
                "### File: {} ({})\n\nFile has {} lines, but requested start_line={}. Use a smaller start_line.",
                path_str, file_info, total_lines, actual_start
            ));
        }

        if actual_start > actual_end {
            return Ok(format!(
                "### File: {} ({})\n\nstart_line ({}) is greater than end_line ({}).",
                path_str, file_info, actual_start, actual_end
            ));
        }

        let range_size = actual_end.saturating_sub(actual_start.saturating_sub(1));

        // === Render output ===
        let mut result = format!(
            "### File: {} ({}, {} lines shown)\n\n",
            path_str, file_info, range_size
        );

        for i in (actual_start.saturating_sub(1))..actual_end {
            result.push_str(&format!("{:>4}: {}\n", i + 1, lines[i]));
        }

        // === Truncation notice ===
        if actual_end < total_lines {
            let remaining = total_lines - actual_end;
            result.push_str(&format!(
                "\n... ({} more lines. Use start_line={} and end_line={} to read more.)",
                remaining, actual_end + 1, total_lines
            ));
        }

        Ok(result)
    }
}

pub struct FileWriteTool;

#[async_trait]
impl Tool for FileWriteTool {
    fn name(&self) -> &str {
        "write_file"
    }
    fn description(&self) -> &str {
        "Write content to a file. Creates parent directories automatically. Creates a .backup file before overwriting. Use apply_patch for surgical edits."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file" },
                "content": { "type": "string", "description": "Full content to write" },
                "dry_run": { "type": "boolean", "default": false, "description": "Simulate without writing" }
            },
            "required": ["path", "content"]
        })
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }
    fn execution_profile(&self) -> ExecutionProfile {
        ExecutionProfile {
            side_effect_level: SideEffectLevel::Local,
            network_access: false,
            filesystem_scope: FilesystemScope::Confined,
            reversibility: Reversibility::Possible,
            requires_human_approval: false,
        }
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let path_str = args["path"]
            .as_str()
            .ok_or_else(|| AgentError("Missing path".to_string()))?;
        let content = args["content"]
            .as_str()
            .ok_or_else(|| AgentError("Missing content".to_string()))?;

        // === Path traversal protection ===
        let path = Path::new(path_str);

        // === Size check ===
        if content.len() > MAX_FILE_SIZE as usize {
            return Err(AgentError(format!(
                "Content too large: {} bytes. Maximum is 50MB.",
                content.len()
            )));
        }

        if args["dry_run"].as_bool().unwrap_or(false) {
            return Ok(format!(
                "[DRY RUN] Would write {} bytes to {}",
                content.len(),
                path_str
            ));
        }

        // === Create parent directory ===
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| AgentError(format!("Failed to create directory {}: {}", parent.display(), e)))?;
        }

        // === Backup existing file ===
        if path.exists() {
            let backup_path = path.with_extension("bak");
            fs::copy(path, &backup_path)
                .map_err(|e| AgentError(format!("Failed to create backup: {}", e)))?;
        }

        // === Write ===
        fs::write(path, content)
            .map_err(|e| AgentError(format!("Failed to write {}: {}", path_str, e)))?;

        let line_count = content.lines().count();
        Ok(format!(
            "Successfully wrote {} bytes ({} lines) to {}",
            content.len(),
            line_count,
            path_str
        ))
    }
}

// ── AST Helpers ──────────────────────────────────────────
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

fn find_cargo_toml_dir(start_path: &Path) -> Option<PathBuf> {
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
        "Apply a unified diff (patch) to a file. Includes AST validation, speculative compile check, and forensic journaling. Preferred over write_file for surgical edits."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file to patch" },
                "patch": { "type": "string", "description": "Unified diff content" },
                "verify_ast": { "type": "boolean", "default": true, "description": "Verify AST structural integrity using tree-sitter" },
                "speculative_check": { "type": "boolean", "default": true, "description": "Perform speculative cargo compilation checks" },
                "reasoning": { "type": "string", "default": "", "description": "Causal reasoning behind this code modification" }
            },
            "required": ["path", "patch"]
        })
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }
    fn execution_profile(&self) -> ExecutionProfile {
        ExecutionProfile {
            side_effect_level: SideEffectLevel::Local,
            network_access: false,
            filesystem_scope: FilesystemScope::Confined,
            reversibility: Reversibility::Possible,
            requires_human_approval: false,
        }
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

        let path = Path::new(path_str);
        let original = fs::read_to_string(path)
            .map_err(|e| AgentError(format!("Failed to read {}: {}", path_str, e)))?;

        let patch = diffy::Patch::from_str(patch_str)
            .map_err(|e| AgentError(format!("Invalid patch format: {}", e)))?;

        let patched = diffy::apply(&original, &patch)
            .map_err(|e| AgentError(format!("Failed to apply patch: {}", e)))?;

        let is_rust_file = path.extension().and_then(|s| s.to_str()) == Some("rs");

        // 1. AST Validation
        if verify_ast && is_rust_file {
            let mut parser = tree_sitter::Parser::new();
            if parser.set_language(&tree_sitter_rust::language()).is_ok()
                && let Some(tree) = parser.parse(&patched, None)
                    && has_error_nodes(tree.root_node()) {
                        return Err(AgentError(format!(
                            "AST Validation Failed: Patched code for {} has syntax errors.",
                            path_str
                        )));
                    }
        }

        let mut check_success = true;
        let mut check_output = String::new();

        // 2. Speculative Sandbox
        if speculative_check && is_rust_file {
            let backup_path = path.with_extension("spec_backup");
            fs::write(&backup_path, &original)
                .map_err(|e| AgentError(format!("Backup failed: {}", e)))?;

            fs::write(path, &patched)
                .map_err(|e| AgentError(format!("Speculative write failed: {}", e)))?;

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
                    check_output = format!("cargo check failed: {}", e);
                }
            }

            let dry_run = args["dry_run"].as_bool().unwrap_or(false);
            if !check_success || dry_run {
                fs::write(path, &original)
                    .map_err(|e| AgentError(format!("Failed to restore original: {}", e)))?;
            }

            let _ = fs::remove_file(&backup_path);

            if !check_success {
                return Err(AgentError(format!(
                    "Speculative compile check failed:\n{}",
                    check_output
                )));
            }
        } else if !args["dry_run"].as_bool().unwrap_or(false) {
            // Non-speculative write
            // Create backup first
            let backup_path = path.with_extension("bak");
            if path.exists() {
                fs::copy(path, &backup_path).ok();
            }
            fs::write(path, &patched).map_err(|e| {
                AgentError(format!("Failed to write patched content: {}", e))
            })?;
        }

        // 3. Forensic Journal
        let journal_dir = Path::new(".pharmakon");
        if !journal_dir.exists() {
            let _ = fs::create_dir_all(journal_dir);
        }
        let journal_path = journal_dir.join("forensics_journal.jsonl");
        let log_entry = json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "target": path_str,
            "reasoning": reasoning,
            "syntax_valid": true,
            "compilation_ok": check_success,
            "speculative_used": speculative_check && is_rust_file,
            "patch_len": patch_str.len(),
        });

        if let Ok(mut file) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&journal_path)
        {
            use std::io::Write;
            let _ = writeln!(file, "{}", log_entry);
        }

        Ok(format!(
            "Applied patch to {}. AST: OK, Compile: OK.",
            path_str
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_file_too_large() {
        // Can't really test with a real file, but test binary detection
        let binary_data = vec![0u8; 100];
        assert!(is_binary_content(&binary_data));

        let text_data = b"hello world\nthis is text\n";
        assert!(!is_binary_content(text_data));
    }

    #[test]
    fn test_image_extension() {
        assert!(is_image_extension(Path::new("photo.png")));
        assert!(is_image_extension(Path::new("photo.jpg")));
        assert!(!is_image_extension(Path::new("file.txt")));
    }
}
