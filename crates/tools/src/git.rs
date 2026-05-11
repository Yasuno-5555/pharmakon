use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, ExecutionProfile, FilesystemScope, Reversibility, SideEffectLevel, Tool, ToolCategory};
use serde_json::{Value, json};
use std::process::Command;

/// Run a git command and return stdout. Wraps common error handling.
fn git(args: &[&str]) -> AgentResult<String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|e| AgentError(format!("git command failed: {}", e)))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(stdout.trim_end().to_string())
    } else {
        Err(AgentError(format!(
            "git {} failed:\n{}",
            args.join(" "),
            if stderr.is_empty() { stdout } else { stderr }
        )))
    }
}

// ═══════════════════════════════════════════════════════════
// GitStatusTool
// ═══════════════════════════════════════════════════════════

pub struct GitStatusTool;

#[async_trait]
impl Tool for GitStatusTool {
    fn name(&self) -> &str {
        "git_status"
    }
    fn description(&self) -> &str {
        "Show working tree status with structured output. Shows staged, unstaged, and untracked files. Use before commits to review changes."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "porcelain": { "type": "boolean", "default": false, "description": "Use machine-readable format" }
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
        let porcelain = args["porcelain"].as_bool().unwrap_or(false);

        if porcelain {
            return git(&["status", "--porcelain"]);
        }

        // Human-readable format with branch info
        let branch = git(&["branch", "--show-current"]).unwrap_or_else(|_| "(detached)".to_string());
        let status = git(&["status"]).unwrap_or_else(|_| "(empty repo)".to_string());

        // Add structured change summary
        let staged = git(&["diff", "--cached", "--stat"]).unwrap_or_default();
        let unstaged = git(&["diff", "--stat"]).unwrap_or_default();
        let untracked = git(&["ls-files", "--others", "--exclude-standard"]).unwrap_or_default();

        let untracked_count = untracked.lines().count();

        Ok(format!(
            "On branch: {}\n\n{}\n\n---\nStaged changes:\n{}\nUnstaged changes:\n{}\nUntracked files: {}\n\nUse git_diff for detailed diffs. Use git_add <file> to stage changes for commit.",
            branch, status, staged, unstaged, untracked_count
        ))
    }
}

// ═══════════════════════════════════════════════════════════
// GitDiffTool
// ═══════════════════════════════════════════════════════════

pub struct GitDiffTool;

#[async_trait]
impl Tool for GitDiffTool {
    fn name(&self) -> &str {
        "git_diff"
    }
    fn description(&self) -> &str {
        "Show detailed diff of changes. Supports staged/unstaged, specific files, and stat summaries."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Specific file or directory to diff" },
                "cached": { "type": "boolean", "default": false, "description": "Show staged (--cached) changes" },
                "stat": { "type": "boolean", "default": false, "description": "Only show stat summary, not full diff" },
                "context": { "type": "integer", "default": 3, "description": "Lines of context" }
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
        let mut cmd_args: Vec<String> = vec!["diff".to_string()];

        if args["cached"].as_bool().unwrap_or(false) {
            cmd_args.push("--cached".to_string());
        }
        if args["stat"].as_bool().unwrap_or(false) {
            cmd_args.push("--stat".to_string());
        } else {
            let ctx = args["context"].as_u64().unwrap_or(3);
            cmd_args.push(format!("-U{}", ctx));
        }

        if let Some(path) = args["path"].as_str().filter(|s| !s.is_empty()) {
            cmd_args.push("--".to_string());
            cmd_args.push(path.to_string());
        }

        let args_refs: Vec<&str> = cmd_args.iter().map(|s| s.as_str()).collect();
        git(&args_refs)
    }
}

// ═══════════════════════════════════════════════════════════
// GitAddTool — selective staging
// ═══════════════════════════════════════════════════════════

pub struct GitAddTool;

#[async_trait]
impl Tool for GitAddTool {
    fn name(&self) -> &str {
        "git_add"
    }
    fn description(&self) -> &str {
        "Stage specific files for commit. Use paths to stage selectively. Use git_add with path='.' to stage all (use with caution)."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File or directory to stage (use '.' cautiously for all)" },
                "patch": { "type": "boolean", "default": false, "description": "Interactive staging (stage hunks)" }
            },
            "required": ["path"]
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
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AgentError("Missing path. Use '.' to stage all, or a specific file path.".to_string()))?;

        // Warn about staging everything
        if path == "." {
            let staged = git(&["diff", "--cached", "--stat"]).unwrap_or_default();
            let unstaged_files = git(&["diff", "--name-only"]).unwrap_or_default();
            let untracked = git(&["ls-files", "--others", "--exclude-standard"]).unwrap_or_default();

            let mut details = String::new();
            let unstaged_count = unstaged_files.lines().count();
            let untracked_count = untracked.lines().count();

            if !staged.is_empty() {
                details.push_str(&format!("Already staged:\n{}\n", staged));
            }
            details.push_str(&format!(
                "Will stage: {} modified files, {} untracked files.\n",
                unstaged_count, untracked_count
            ));
            if untracked_count > 0 {
                details.push_str(&format!("Untracked files:\n{}\n", untracked));
            }

            if args["dry_run"].as_bool().unwrap_or(false) {
                return Ok(format!("[DRY RUN] Would stage all changes.\n{}", details));
            }
        }

        git(&["add", path])
    }
}

// ═══════════════════════════════════════════════════════════
// GitCommitTool — safe commit with NO auto-add
// ═══════════════════════════════════════════════════════════

pub struct GitCommitTool;

#[async_trait]
impl Tool for GitCommitTool {
    fn name(&self) -> &str {
        "git_commit"
    }
    fn description(&self) -> &str {
        "Create a git commit with staged changes. Use git_add to stage files first. Does NOT auto-stage — you must explicitly stage changes with git_add."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "message": { "type": "string", "description": "Commit message" },
                "amend": { "type": "boolean", "default": false, "description": "Amend last commit (use with caution)" }
            },
            "required": ["message"]
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
            requires_human_approval: true,
        }
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let message = args["message"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AgentError("Missing commit message.".to_string()))?;

        let amend = args["amend"].as_bool().unwrap_or(false);

        // Check there is something to commit
        let staged = git(&["diff", "--cached", "--name-only"]).unwrap_or_default();
        if staged.is_empty() && !amend {
            return Err(AgentError(
                "No staged changes to commit. Use git_add <file> to stage files first.".to_string()
            ));
        }

        let mut cmd: Vec<&str> = vec!["commit", "-m", message];
        if amend {
            cmd.push("--amend");
        }

        git(&cmd)
    }
}

// ═══════════════════════════════════════════════════════════
// GitLogTool
// ═══════════════════════════════════════════════════════════

pub struct GitLogTool;

#[async_trait]
impl Tool for GitLogTool {
    fn name(&self) -> &str {
        "git_log"
    }
    fn description(&self) -> &str {
        "Show commit history with one-line summaries. Supports limiting count and path filters."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "count": { "type": "integer", "default": 10, "description": "Number of commits to show" },
                "path": { "type": "string", "description": "Filter by file path" },
                "author": { "type": "string", "description": "Filter by author" },
                "oneline": { "type": "boolean", "default": true, "description": "One-line format vs detailed" }
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
        let count = args["count"].as_u64().unwrap_or(10).max(1).min(100);
        let oneline = args["oneline"].as_bool().unwrap_or(true);

        let mut cmd: Vec<String> = vec!["log".to_string(), format!("-n{}", count)];

        if oneline {
            cmd.push("--oneline".to_string());
        }

        if let Some(author) = args["author"].as_str().filter(|s| !s.is_empty()) {
            cmd.push("--author".to_string());
            cmd.push(author.to_string());
        }

        if let Some(path) = args["path"].as_str().filter(|s| !s.is_empty()) {
            cmd.push("--".to_string());
            cmd.push(path.to_string());
        }

        let args_refs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
        git(&args_refs)
    }
}

// ═══════════════════════════════════════════════════════════
// GitBranchTool
// ═══════════════════════════════════════════════════════════

pub struct GitBranchTool;

#[async_trait]
impl Tool for GitBranchTool {
    fn name(&self) -> &str {
        "git_branch"
    }
    fn description(&self) -> &str {
        "List, create, or delete branches."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["list", "create", "delete"], "default": "list", "description": "Branch operation" },
                "name": { "type": "string", "description": "Branch name (required for create/delete)" }
            }
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
        let action = args["action"].as_str().unwrap_or("list");

        match action {
            "list" => {
                let branches = git(&["branch"]).unwrap_or_default();
                let current = git(&["branch", "--show-current"]).unwrap_or_default();
                Ok(format!("Current branch: {}\n\nAll branches:\n{}", current, branches))
            }
            "create" => {
                let name = args["name"].as_str().filter(|s| !s.is_empty())
                    .ok_or_else(|| AgentError("Missing branch name for create".to_string()))?;
                git(&["checkout", "-b", name])
            }
            "delete" => {
                let name = args["name"].as_str().filter(|s| !s.is_empty())
                    .ok_or_else(|| AgentError("Missing branch name for delete".to_string()))?;
                git(&["branch", "-d", name])
            }
            _ => Err(AgentError(format!("Unknown action: {}", action))),
        }
    }
}
