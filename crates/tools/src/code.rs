use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, ExecutionProfile, FilesystemScope, Reversibility, SideEffectLevel, Tool, ToolCategory};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

const BINARY_SCAN_SIZE: usize = 8192;

/// Detect binary content by scanning for null bytes.
fn is_binary_data(data: &[u8]) -> bool {
    let scan_end = data.len().min(BINARY_SCAN_SIZE);
    data[..scan_end].contains(&0x00)
}

/// Cached ripgrep availability check (runs once).
fn has_ripgrep() -> bool {
    static RG_CHECK: OnceLock<bool> = OnceLock::new();
    *RG_CHECK.get_or_init(|| Command::new("rg").arg("--version").output().is_ok())
}

/// Check if a path is within the project workspace.
fn is_in_workspace(path: &str) -> bool {
    let cwd = std::env::current_dir().ok();
    match cwd {
        Some(wd) => {
            let p = Path::new(path);
            if p.is_absolute() {
                p.starts_with(&wd)
            } else {
                true // relative paths are resolved from cwd
            }
        }
        None => true, // can't check, allow
    }
}

// ═══════════════════════════════════════════════════════════
// GrepSearchTool
// ═══════════════════════════════════════════════════════════

pub struct GrepSearchTool;

#[async_trait]
impl Tool for GrepSearchTool {
    fn name(&self) -> &str {
        "grep_files"
    }
    fn description(&self) -> &str {
        "Search file contents for a pattern using ripgrep (falls back to grep). Supports file globs, context lines, and result limits. Binary files are auto-skipped."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "The search pattern (regex supported)" },
                "path": { "type": "string", "default": ".", "description": "Search directory (relative to workspace)" },
                "include": { "type": "string", "description": "File glob pattern (e.g. '*.rs', '*.{ts,js}')" },
                "exclude": { "type": "string", "description": "Glob pattern to exclude" },
                "context": { "type": "integer", "default": 1, "description": "Lines of context before/after each match" },
                "max_results": { "type": "integer", "default": 50, "description": "Maximum matches to return" },
                "case_sensitive": { "type": "boolean", "default": false, "description": "Case-sensitive search" }
            },
            "required": ["query"]
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
        let query = args["query"]
            .as_str()
            .ok_or_else(|| AgentError("Missing query".to_string()))?;
        let search_path = args["path"].as_str().unwrap_or(".");
        let max_results = args["max_results"].as_u64().unwrap_or(50) as usize;
        let include = args["include"].as_str();
        let exclude = args["exclude"].as_str();
        let context_lines = args["context"].as_u64().unwrap_or(1);
        let case_sensitive = args["case_sensitive"].as_bool().unwrap_or(false);

        // Guard against searching outside workspace
        if !is_in_workspace(search_path) {
            return Err(AgentError(format!(
                "Search path '{}' appears to be outside the workspace. Use absolute paths only within the workspace.",
                search_path
            )));
        }

        let use_rg = has_ripgrep();

        let (mut cmd, is_rg) = if use_rg {
            let mut cmd = Command::new("rg");
            cmd.arg("--line-number");
            if context_lines > 0 {
                cmd.arg("-C").arg(context_lines.to_string());
            }
            cmd.arg("--max-count").arg(max_results.to_string());
            if !case_sensitive {
                cmd.arg("-i");
            }
            // Skip binary files
            cmd.arg("--no-unicode");
            if let Some(inc) = include {
                cmd.arg("-g").arg(inc);
            }
            if let Some(exc) = exclude {
                cmd.arg("-g").arg(format!("!{}", exc));
            }
            cmd.arg(query).arg(search_path);
            (cmd, true)
        } else {
            let mut cmd = Command::new("grep");
            cmd.arg("--line-number").arg("-r");
            if context_lines > 0 {
                cmd.arg("-C").arg(context_lines.to_string());
            }
            if !case_sensitive {
                cmd.arg("-i");
            }
            if let Some(inc) = include {
                cmd.arg("--include").arg(inc);
            }
            // grep has no --exclude flag for path patterns, we skip
            cmd.arg(query).arg(search_path);
            (cmd, false)
        };

        let output = cmd.output()
            .map_err(|e| AgentError(format!("Search failed: {}", e)))?;

        let result = String::from_utf8_lossy(&output.stdout).to_string();

        if result.is_empty() {
            return Ok("No matches found.".to_string());
        }

        let lines: Vec<&str> = result.lines().collect();
        let total = lines.len();

        if is_rg {
            // rg already truncated with --max-count
            if total >= max_results {
                let truncated = lines[..max_results].join("\n");
                Ok(format!(
                    "{}\n\n... (Truncated to {} results out of {} or more lines)",
                    truncated, max_results, total
                ))
            } else {
                Ok(format!("Found {} matches:\n\n{}", total, result))
            }
        } else if total > max_results {
            let truncated = lines[..max_results].join("\n");
            Ok(format!(
                "{}\n\n... (Truncated to {} of {} matches)",
                truncated, max_results, total
            ))
        } else {
            Ok(format!("Found {} matches:\n\n{}", total, result))
        }
    }
}

// ═══════════════════════════════════════════════════════════
// ListDirTool
// ═══════════════════════════════════════════════════════════

/// Default ignore patterns for directory listing.
const DEFAULT_IGNORE: &[&str] = &[
    ".git", "node_modules", "target", ".DS_Store", ".next", "dist", "build",
    ".venv", "venv", "__pycache__", ".rbenv", ".bundle", "vendor/bundle",
];

pub struct ListDirTool;

#[async_trait]
impl Tool for ListDirTool {
    fn name(&self) -> &str {
        "list_dir"
    }
    fn description(&self) -> &str {
        "List directory contents with file type, size, and modification info. Ignores .git, node_modules, target by default. Supports depth control."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "default": ".", "description": "Target directory" },
                "depth": { "type": "integer", "default": 1, "description": "Recursion depth (1 = immediate children only)" },
                "show_hidden": { "type": "boolean", "default": false, "description": "Include hidden files (dotfiles)" },
                "max_entries": { "type": "integer", "default": 200, "description": "Maximum entries to show" }
            }
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
        let path = args["path"].as_str().unwrap_or(".");
        let depth = args["depth"].as_u64().unwrap_or(1) as usize;
        let show_hidden = args["show_hidden"].as_bool().unwrap_or(false);
        let max_entries = args["max_entries"].as_u64().unwrap_or(200) as usize;

        if !is_in_workspace(path) {
            return Err(AgentError(format!(
                "Path '{}' is outside the workspace.", path
            )));
        }

        let root = Path::new(path);
        if !root.is_dir() {
            return Err(AgentError(format!("'{}' is not a directory or does not exist.", path)));
        }

        let mut entries: Vec<_> = Vec::new();
        let mut dir_count = 0u64;
        let mut file_count = 0u64;
        let mut total_size = 0u64;

        collect_entries(root, root, depth, show_hidden, max_entries, &mut entries, &mut dir_count, &mut file_count, &mut total_size);

        let size_str = if total_size >= 1024 * 1024 {
            format!("{:.1}MB", total_size as f64 / (1024.0 * 1024.0))
        } else if total_size >= 1024 {
            format!("{}KB", total_size / 1024)
        } else {
            format!("{}B", total_size)
        };

        let mut result = format!(
            "### {} ({} dirs, {} files, {})\n\n",
            path, dir_count, file_count, size_str
        );

        for (indent, name, kind, size) in &entries {
            let prefix = "  ".repeat(*indent);
            let icon = match kind.as_str() {
                "dir" => "📁",
                "symlink" => "🔗",
                _ => "📄",
            };
            result.push_str(&format!(
                "{}{} {} {}{}\n",
                prefix,
                icon,
                name,
                if *kind == "file" { format!("({})", format_size(*size)) } else { String::new() },
                if *kind == "dir" { "/" } else { "" }
            ));
        }

        if entries.len() >= max_entries {
            result.push_str(&format!("\n... (max {} entries shown)", max_entries));
        }

        Ok(result)
    }
}

fn collect_entries(
    root: &Path,
    dir: &Path,
    max_depth: usize,
    show_hidden: bool,
    max_entries: usize,
    out: &mut Vec<(usize, String, String, u64)>,
    dir_count: &mut u64,
    file_count: &mut u64,
    total_size: &mut u64,
) {
    if out.len() >= max_entries {
        return;
    }

    let depth = dir.components().count().saturating_sub(root.components().count());
    if depth > max_depth {
        return;
    }

    let mut local: Vec<_> = match fs::read_dir(dir) {
        Ok(entries) => entries.filter_map(|e| e.ok()).collect(),
        Err(_) => return,
    };

    // Sort: directories first, then files, alphabetically
    local.sort_by_key(|e| {
        let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let name = e.file_name().to_string_lossy().to_string();
        (!is_dir, name)
    });

    for entry in local {
        if out.len() >= max_entries {
            return;
        }

        let name = entry.file_name();
        let name_str = name.to_string_lossy().to_string();

        // Skip ignored directories
        if !show_hidden && DEFAULT_IGNORE.contains(&name_str.as_str()) {
            continue;
        }
        if !show_hidden && name_str.starts_with('.') {
            continue;
        }

        let rel_depth = dir.components().count().saturating_sub(root.components().count());

        if let Ok(ftype) = entry.file_type() {
            if ftype.is_dir() {
                *dir_count += 1;
                out.push((rel_depth, name_str.clone(), "dir".to_string(), 0));
                collect_entries(root, &entry.path(), max_depth, show_hidden, max_entries, out, dir_count, file_count, total_size);
            } else if ftype.is_symlink() {
                out.push((rel_depth, name_str, "symlink".to_string(), 0));
            } else {
                *file_count += 1;
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                *total_size += size;
                out.push((rel_depth, name_str, "file".to_string(), size));
            }
        }
    }
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{}KB", bytes / 1024)
    } else {
        format!("{}B", bytes)
    }
}

// ═══════════════════════════════════════════════════════════
// ViewFileTool
// ═══════════════════════════════════════════════════════════

pub struct ViewFileTool;

#[async_trait]
impl Tool for ViewFileTool {
    fn name(&self) -> &str {
        "view_file"
    }
    fn description(&self) -> &str {
        "Read file contents with line numbers. Supports line ranges, skeleton view, and binary detection. Use for reading specific sections of large files."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to file" },
                "start_line": { "type": "integer", "default": 1, "description": "First line to show (1-indexed)" },
                "end_line": { "type": "integer", "description": "Last line to show. Defaults to start+200." },
                "view_skeleton": { "type": "boolean", "default": false, "description": "Only show struct/fn signatures" }
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
        let path = args["path"]
            .as_str()
            .ok_or_else(|| AgentError("Missing path".to_string()))?;
        let start_line = args["start_line"].as_u64().unwrap_or(1) as usize;
        let end_line = args["end_line"].as_u64().map(|e| e as usize);
        let view_skeleton = args["view_skeleton"].as_bool().unwrap_or(false);

        let canonical = Path::new(path);
        let data = fs::read(canonical)
            .map_err(|e| AgentError(format!("Read failed: {} ({})", path, e)))?;

        // Binary detection
        if is_binary_data(&data) {
            return Ok(format!(
                "### File: {} (binary, {} bytes)\n\nBinary file cannot be displayed as text.",
                path,
                data.len()
            ));
        }

        let content = String::from_utf8(data)
            .map_err(|_| AgentError(format!("File {} is not valid UTF-8 text.", path)))?;

        if view_skeleton {
            let skeleton = pharmakon_common::CodeUtils::skeletonize_code(&content);
            let total = content.lines().count();
            return Ok(format!(
                "### Skeleton: {} ({} lines total)\n\n{}\n\n[Use view_file without view_skeleton to see full implementations.]",
                path, total, skeleton
            ));
        }

        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();

        let actual_end = end_line.unwrap_or_else(|| {
            if start_line > 1 {
                (start_line + 199).min(total)
            } else {
                total
            }
        })
        .min(total);

        if start_line > total {
            return Ok(format!(
                "### File: {} ({} lines)\n\nRequested start_line={} exceeds file length ({} lines).",
                path, total, start_line, total
            ));
        }

        let range_size = actual_end.saturating_sub(start_line.saturating_sub(1));
        let mut result = format!("### File: {} ({} lines, showing {})\n\n", path, total, range_size);

        for i in (start_line.max(1).saturating_sub(1))..actual_end {
            result.push_str(&format!("{:>4}: {}\n", i + 1, lines[i]));
        }

        if actual_end < total {
            let remaining = total - actual_end;
            result.push_str(&format!(
                "\n... ({} more lines. Set start_line={} to continue reading.)",
                remaining, actual_end + 1
            ));
        }

        Ok(result)
    }
}

// ═══════════════════════════════════════════════════════════
// StrictReplaceContentTool
// ═══════════════════════════════════════════════════════════

pub struct StrictReplaceContentTool;

#[async_trait]
impl Tool for StrictReplaceContentTool {
    fn name(&self) -> &str {
        "replace_content"
    }
    fn description(&self) -> &str {
        "Replace exact content in a file within specified line ranges. Requires unique match — rejects ambiguous replacements. Safer than write_file for surgical edits."
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
                            "start_line": { "type": "integer", "description": "Search scope start (1-indexed)" },
                            "end_line": { "type": "integer", "description": "Search scope end (1-indexed)" },
                            "old": { "type": "string", "description": "Text to find. Exact match by default. Set trim=true to ignore surrounding whitespace (more reliable)." },
                            "new": { "type": "string", "description": "Replacement text" },
                            "trim": { "type": "boolean", "default": false, "description": "When true, trims whitespace from old+new before matching. Use when exact indentation is uncertain." }
                        },
                        "required": ["start_line", "end_line", "old", "new"]
                    }
                }
            },
            "required": ["path", "edits"]
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
        let path = args["path"]
            .as_str()
            .ok_or_else(|| AgentError("Missing path".to_string()))?;
        let edits = args["edits"]
            .as_array()
            .ok_or_else(|| AgentError("Missing edits array".to_string()))?;

        let mut content = fs::read_to_string(path)
            .map_err(|e| AgentError(format!("Failed to read: {}", e)))?;
        let ends_with_newline = content.ends_with('\n');

        for edit in edits {
            let start = edit["start_line"].as_u64().unwrap_or(1) as usize;
            let end = edit["end_line"].as_u64().unwrap_or(1) as usize;
            let raw_old = edit["old"].as_str().unwrap_or("");
            let replacement = edit["new"].as_str().unwrap_or("");
            let trim_enabled = edit["trim"].as_bool().unwrap_or(false);

            if raw_old.is_empty() {
                return Err(AgentError("Empty 'old' string in edit".to_string()));
            }

            // When trim is enabled, normalize whitespace for fuzzy matching
            let old = if trim_enabled { raw_old.trim() } else { raw_old };
            let replacement = if trim_enabled { replacement.trim() } else { replacement };

            let mut lines: Vec<&str> = content.split('\n').collect();
            if ends_with_newline && lines.last() == Some(&"") {
                lines.pop();
            }

            let total_lines = lines.len();
            let s_idx = start.saturating_sub(1).min(total_lines);
            let e_idx = end.min(total_lines);

            if s_idx >= e_idx {
                return Err(AgentError(format!("Invalid range: {}-{} (file has {} lines)", start, end, total_lines)));
            }

            let scope_text = lines[s_idx..e_idx].join("\n");

            // Try exact match first; fall back to trimmed if enabled
            let match_count = scope_text.matches(old).count();
            let (actual_old, actual_replacement) = if match_count == 0 && !trim_enabled {
                // Exact match failed — try trimming both sides (whitespace-tolerant)
                let trimmed_old = old.trim();
                let trimmed_count = scope_text.matches(trimmed_old).count();
                if trimmed_count > 0 {
                    (trimmed_old, replacement.trim())
                } else {
                    (old, replacement)
                }
            } else {
                (old, replacement)
            };

            let match_count = scope_text.matches(actual_old).count();
            if match_count == 0 {
                return Err(AgentError(format!(
                    "Target text not found in lines {}-{}. Try setting trim=true to ignore whitespace, or check exact indentation:\n---\n{}\n---",
                    start, end, &raw_old[..raw_old.len().min(100)]
                )));
            }
            if match_count > 1 {
                return Err(AgentError(format!(
                    "Target text found {} times in lines {}-{}. Narrow the range or make the text more specific.",
                    match_count, start, end
                )));
            }

            // Build result by replacing only the scope portion
            let new_scope_text = scope_text.replace(actual_old, actual_replacement);
            let mut new_lines = Vec::new();
            if s_idx > 0 {
                new_lines.push(lines[..s_idx].join("\n"));
            }
            new_lines.push(new_scope_text);
            if e_idx < total_lines {
                new_lines.push(lines[e_idx..].join("\n"));
            }

            content = new_lines.join("\n");
            if ends_with_newline {
                content.push('\n');
            }
        }

        // Backup before writing
        let path_obj = Path::new(path);
        let backup_path = path_obj.with_extension("bak");
        if path_obj.exists() {
            fs::copy(path_obj, &backup_path).ok();
        }

        fs::write(path, &content)
            .map_err(|e| AgentError(format!("Failed to write: {}", e)))?;

        Ok(format!("Applied {} replacement(s) to {}", edits.len(), path))
    }
}

// ═══════════════════════════════════════════════════════════
// FindDefinitionTool — tree-sitter AST based
// ═══════════════════════════════════════════════════════════

pub struct FindDefinitionTool;

#[async_trait]
impl Tool for FindDefinitionTool {
    fn name(&self) -> &str {
        "find_definition"
    }
    fn description(&self) -> &str {
        "Find definitions (functions, structs, traits, enums) using tree-sitter AST parsing. More accurate than grep-based search."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Symbol name to find" },
                "language": { "type": "string", "enum": ["rust", "python", "javascript", "typescript"], "default": "rust" },
                "path": { "type": "string", "default": ".", "description": "Search directory" }
            },
            "required": ["name"]
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
        let name = args["name"]
            .as_str()
            .ok_or_else(|| AgentError("Missing name".to_string()))?;
        let lang = args["language"].as_str().unwrap_or("rust");
        let search_path = args["path"].as_str().unwrap_or(".");

        // For Rust, use tree-sitter AST-based search
        if lang == "rust" {
            return self.find_in_rust(name, search_path).await;
        }

        // Fall back to grep patterns for other languages
        let pattern = match lang {
            "python" => format!("(def|class|async def)\\s+{}", regex_escape(name)),
            "javascript" | "typescript" => {
                format!("(function|class|const|interface|type)\\s+{}", regex_escape(name))
            }
            _ => regex_escape(name),
        };

        let output = Command::new("grep")
            .arg("-r")
            .arg("-n")
            .arg("-E")
            .arg(&pattern)
            .arg(search_path)
            .output()
            .map_err(|e| AgentError(e.to_string()))?;

        let result = String::from_utf8_lossy(&output.stdout).to_string();
        if result.is_empty() {
            Ok(format!("No definition found for '{}'", name))
        } else {
            Ok(result)
        }
    }
}

impl FindDefinitionTool {
    async fn find_in_rust(&self, name: &str, search_path: &str) -> AgentResult<String> {
        // Use ripgrep to find Rust files, then parse each with tree-sitter
        let rg_available = has_ripgrep();

        let mut finder = if rg_available {
            let mut cmd = Command::new("rg");
            cmd.arg("--files")
               .arg("--glob")
               .arg("*.rs")
               .arg(search_path);
            cmd
        } else {
            let mut cmd = Command::new("find");
            cmd.arg(search_path)
               .arg("-name")
               .arg("*.rs")
               .arg("-type")
               .arg("f");
            cmd
        };

        let output = finder.output()
            .map_err(|e| AgentError(format!("Failed to find files: {}", e)))?;

        let output_text = String::from_utf8_lossy(&output.stdout).to_string();
        let files: Vec<&str> = output_text
            .lines()
            .filter(|l| !l.is_empty())
            .collect();

        let mut parser = tree_sitter::Parser::new();
        if parser.set_language(&tree_sitter_rust::language()).is_err() {
            // Fall back to grep
            return self.grep_fallback(name, search_path, "rust").await;
        }

        let mut results = Vec::new();

        for file_path in &files {
            let content = match fs::read_to_string(file_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let tree = match parser.parse(&content, None) {
                Some(t) => t,
                None => continue,
            };

            // Search for definitions in the AST
            self.find_def_in_tree(tree.root_node(), name, file_path, &content, &mut results);

            if results.len() >= 10 {
                break;
            }
        }

        if results.is_empty() {
            Ok(format!("No definition found for '{}' in .rs files", name))
        } else {
            Ok(results.join("\n\n---\n\n"))
        }
    }

    fn find_def_in_tree(&self, node: tree_sitter::Node, name: &str, file_path: &str, content: &str, out: &mut Vec<String>) {
        // Check if this node is a definition node matching our name
        let kind = node.kind();
        let is_def = matches!(kind, "function_item" | "struct_item" | "enum_item" | "trait_item" | "type_item" | "macro_definition" | "impl_item" | "const_item" | "static_item" | "union_item");

        if is_def {
            // Get the name from the first child that's an identifier or type identifier
            let mut cursor = node.walk();
            if cursor.goto_first_child() {
                loop {
                    let child = cursor.node();
                    let child_kind = child.kind();
                    if matches!(child_kind, "identifier" | "type_identifier")
                        && child.utf8_text(content.as_bytes()).ok() == Some(name) {
                            // Found it!
                            let start_line = node.start_position().row + 1;
                            let end_line = node.end_position().row + 1;
                            let mut snippet = String::new();
                            for i in start_line.saturating_sub(1)..end_line.min(start_line + 20) {
                                if let Some(line) = content.lines().nth(i.saturating_sub(1)) {
                                    snippet.push_str(&format!("{:>4}: {}\n", i, line));
                                }
                            }
                            out.push(format!(
                                "{}:{}\n`{}` defined in {} (lines {}-{})\n\n{}",
                                file_path, start_line, name, file_path, start_line, end_line, snippet
                            ));
                            return;
                        }
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
            }
        }

        // Recursively search children
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                self.find_def_in_tree(cursor.node(), name, file_path, content, out);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }

    async fn grep_fallback(&self, name: &str, search_path: &str, _lang: &str) -> AgentResult<String> {
        let pattern = format!("(fn|struct|enum|trait|type|impl|macro_rules!)\\s+{}", regex_escape(name));
        let out = Command::new("grep")
            .arg("-r").arg("-n").arg("-E")
            .arg(&pattern).arg(search_path)
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

fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '.' | '\\' | '+' | '*' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

// ═══════════════════════════════════════════════════════════
// PythonInterpreterTool
// ═══════════════════════════════════════════════════════════

pub struct PythonInterpreterTool;

#[async_trait]
impl Tool for PythonInterpreterTool {
    fn name(&self) -> &str {
        "python_interpreter"
    }
    fn description(&self) -> &str {
        "Execute a Python script and return its output. Has a 30-second timeout. Includes a Pharmakon helper class for file access."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "code": { "type": "string", "description": "Python code to execute" },
                "timeout": { "type": "integer", "default": 30, "description": "Timeout in seconds" }
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
        let timeout = args["timeout"].as_u64().unwrap_or(30);

        let preamble = r#"
import os, sys, json, subprocess, math, statistics, datetime, re, itertools, collections

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
            res = subprocess.check_output(['grep', '-rn', query, path], text=True, timeout=10)
            return res
        except:
            return "No matches."

pharmakon = Pharmakon()
"#;

        let full_code = format!("{}\n{}", preamble, code);

        let child = tokio::process::Command::new("python3")
            .arg("-c")
            .arg(full_code)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| AgentError(format!("Failed to start Python: {}", e)))?;

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout),
            child.wait_with_output(),
        ).await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                if output.status.success() {
                    Ok(stdout)
                } else {
                    Ok(format!("Exit code: {}\nStderr: {}\nStdout: {}",
                        output.status.code().unwrap_or(-1), stderr, stdout))
                }
            }
            Ok(Err(e)) => Err(AgentError(format!("Python execution error: {}", e))),
            Err(_) => Err(AgentError(format!("Python timed out after {} seconds", timeout))),
        }
    }
}
