//! Tool Scheduler, Budgeting, and Policy Enforcement Engine.
//! 
//! This module decouples high-level agentic intents from raw filesystem executions,
//! implements automatic pre-flight redirection gates (CodeActGate), enforces rigorous 
//! exploration budgets and tool policies, computes file-level attention scores, and
//! hosts an asynchronous Virtual File System & Indexing Daemon (VFS / Living Graph)
//! for instant project topology queries.

use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Instant, Duration};
use serde::{Serialize, Deserialize};
use serde_json::{Value, json};
use anyhow::{Result, anyhow};

// ═══════════════════════════════════════════════════════
// 1. Tool Intent and Intent Routing
// ═══════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolIntent {
    ExploreDirectory { path: String },
    ReadFile { path: String, start_line: Option<usize>, end_line: Option<usize> },
    SearchSymbol { query: String },
    CompileCheck,
    ModifyCode { path: String, patch: String, reasoning: String },
}

// ═══════════════════════════════════════════════════════
// 2. Exploration Budget
// ═══════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplorationBudget {
    pub max_files: usize,
    pub max_depth: usize,
    pub max_tokens: usize,
    pub max_parallel_tasks: usize,
    
    pub files_explored: usize,
    pub tokens_consumed: usize,
    pub current_depth: usize,
}

impl Default for ExplorationBudget {
    fn default() -> Self {
        Self {
            max_files: 100,
            max_depth: 3,
            max_tokens: 500_000,
            max_parallel_tasks: 4,
            files_explored: 0,
            tokens_consumed: 0,
            current_depth: 0,
        }
    }
}

impl ExplorationBudget {
    pub fn enforce_limit(&self) -> Result<()> {
        if self.files_explored > self.max_files {
            return Err(anyhow!(
                "Exploration budget exceeded: file count limit ({} / {}) reached. Returning to planning phase.",
                self.files_explored, self.max_files
            ));
        }
        if self.tokens_consumed > self.max_tokens {
            return Err(anyhow!(
                "Exploration budget exceeded: token consumption limit ({} / {}) reached. Returning to planning phase.",
                self.tokens_consumed, self.max_tokens
            ));
        }
        if self.current_depth > self.max_depth {
            return Err(anyhow!(
                "Exploration budget exceeded: traversal depth ({} / {}) exceeded. Returning to planning phase.",
                self.current_depth, self.max_depth
            ));
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════
// 3. Tool Use Policy Engine
// ═══════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPolicy {
    pub cooldown: Duration,
    pub max_depth: usize,
    pub max_matches: usize,
    pub truncate_after_lines: usize,
    pub require_reason: bool,
}

impl Default for ToolPolicy {
    fn default() -> Self {
        Self {
            cooldown: Duration::from_secs(5),
            max_depth: 2,
            max_matches: 50,
            truncate_after_lines: 300,
            require_reason: true,
        }
    }
}

pub struct ToolPolicyEngine {
    pub policies: HashMap<String, ToolPolicy>,
    last_executed: Mutex<HashMap<String, Instant>>,
}

impl Default for ToolPolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolPolicyEngine {
    pub fn new() -> Self {
        let mut policies = HashMap::new();
        
        policies.insert("list_dir".to_string(), ToolPolicy {
            cooldown: Duration::from_secs(10),
            max_depth: 2,
            max_matches: 100,
            truncate_after_lines: 100,
            require_reason: true,
        });

        policies.insert("grep_search".to_string(), ToolPolicy {
            cooldown: Duration::from_secs(5),
            max_depth: 0,
            max_matches: 50,
            truncate_after_lines: 200,
            require_reason: true,
        });

        policies.insert("view_file".to_string(), ToolPolicy {
            cooldown: Duration::from_secs(2),
            max_depth: 0,
            max_matches: 0,
            truncate_after_lines: 300,
            require_reason: false,
        });

        Self {
            policies,
            last_executed: Mutex::new(HashMap::new()),
        }
    }

    pub fn enforce(&self, tool: &str, args: &Value) -> Result<()> {
        let now = Instant::now();

        // Cooldown check — key by tool:path to avoid independent operations blocking each other
        if let Some(policy) = self.policies.get(tool) {
            let mut last_exec = self.last_executed.lock().unwrap();
            let key = format!("{}:{}", tool, args.get("path").and_then(|p| p.as_str()).unwrap_or("*"));
            if let Some(last) = last_exec.get(&key)
                && now.duration_since(*last) < policy.cooldown {
                    let wait_needed = policy.cooldown - now.duration_since(*last);
                    return Err(anyhow!(
                        "Tool '{}' for this path is on cooldown. Please wait {:.1}s or consolidate calls.",
                        tool, wait_needed.as_secs_f32()
                    ));
                }
            last_exec.insert(key, now);

            // Require reason check
            if policy.require_reason {
                let has_reason = args.get("reasoning").or_else(|| args.get("reason")).is_some();
                if !has_reason && tool != "view_file" {
                    return Err(anyhow!(
                        "Tool '{}' requires a 'reasoning' field explaining why this action is necessary.",
                        tool
                    ));
                }
            }
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════
// 4. Attention Scheduler
// ═══════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttentionScore {
    pub path: PathBuf,
    pub score: f32,
    pub relevance: f32,
    pub uncertainty_reduction: f32,
    pub token_cost: f32,
    pub recency: f32,
}

pub struct AttentionScheduler {
    pub scores: Mutex<HashMap<PathBuf, AttentionScore>>,
    pub touched_history: Mutex<Vec<PathBuf>>,
}

impl Default for AttentionScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl AttentionScheduler {
    pub fn new() -> Self {
        Self {
            scores: Mutex::new(HashMap::new()),
            touched_history: Mutex::new(Vec::new()),
        }
    }

    /// Calculate and update attention scores across files
    pub fn update_score(&self, path: &Path, relevance: f32, uncertainty_reduction: f32, token_cost: f32) -> f32 {
        let path_buf = path.to_path_buf();
        let history = self.touched_history.lock().unwrap();
        
        // Recency calculation
        let recency = if let Some(pos) = history.iter().position(|p| p == &path_buf) {
            1.0 - (pos as f32 / history.len().max(1) as f32)
        } else {
            0.1
        };

        let calculated_score = (relevance * uncertainty_reduction) / (token_cost * recency).max(0.01);
        
        let score_entry = AttentionScore {
            path: path_buf.clone(),
            score: calculated_score,
            relevance,
            uncertainty_reduction,
            token_cost,
            recency,
        };

        let mut scores = self.scores.lock().unwrap();
        scores.insert(path_buf, score_entry);
        calculated_score
    }

    /// Track a file as touched to update recency
    pub fn touch_file(&self, path: &Path) {
        let path_buf = path.to_path_buf();
        let mut history = self.touched_history.lock().unwrap();
        history.retain(|p| p != &path_buf);
        history.push(path_buf);
    }

    /// Prune low relevance files from a planned batch read
    pub fn filter_high_attention(&self, paths: &[PathBuf], threshold: f32) -> Vec<PathBuf> {
        let scores = self.scores.lock().unwrap();
        paths.iter()
            .filter(|p| {
                if let Some(score) = scores.get(*p) {
                    score.score >= threshold
                } else {
                    true // Keep un-scored files by default as potential discoveries
                }
            })
            .cloned()
            .collect()
    }
}

// ═══════════════════════════════════════════════════════
// 5. Directory Indexing Daemon (VFS & Living Graph)
// ═══════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolDefinition {
    pub name: String,
    pub kind: String,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub symbols: Vec<SymbolDefinition>,
    pub dependencies: Vec<String>,
}

pub struct DirectoryIndexingDaemon {
    pub workspace_root: PathBuf,
    pub index: Arc<Mutex<HashMap<PathBuf, FileMetadata>>>,
    pub is_building: Arc<AtomicBool>,
    pub cancel_flag: Arc<AtomicBool>,
}

impl DirectoryIndexingDaemon {
    pub fn new(workspace_root: PathBuf) -> Self {
        let daemon = Self {
            workspace_root,
            index: Arc::new(Mutex::new(HashMap::new())),
            is_building: Arc::new(AtomicBool::new(false)),
            cancel_flag: Arc::new(AtomicBool::new(false)),
        };
        // Non-blocking background initialization
        daemon.rebuild_async();
        daemon
    }

    pub fn rebuild_async(&self) {
        // Cancel any previous rebuild in progress
        self.cancel_flag.store(true, Ordering::SeqCst);

        let is_building = self.is_building.clone();
        let cancel_flag = self.cancel_flag.clone();
        let index = self.index.clone();
        let root = self.workspace_root.clone();

        // Use spawn_blocking for synchronous I/O to avoid blocking tokio threads
        tokio::task::spawn_blocking(move || {
            if is_building.swap(true, Ordering::SeqCst) {
                log::debug!("IndexingDaemon: rebuild already in progress, skipping");
                return;
            }
            // Reset cancel flag for the new build
            cancel_flag.store(false, Ordering::SeqCst);

            log::info!("IndexingDaemon: Rebuilding index for {}...", root.display());
            let mut local_index = HashMap::new();

            if let Ok(entries) = walk_dir(&root) {
                for path in entries {
                    // Check cancellation at each file
                    if cancel_flag.load(Ordering::SeqCst) {
                        log::info!("IndexingDaemon: rebuild cancelled");
                        is_building.store(false, Ordering::SeqCst);
                        return;
                    }
                    if let Ok(meta) = extract_metadata(&path) {
                        local_index.insert(path.strip_prefix(&root).unwrap_or(&path).to_path_buf(), meta);
                    }
                }
            }

            {
                let mut index_lock = index.lock().unwrap();
                *index_lock = local_index;
            }

            is_building.store(false, Ordering::SeqCst);
            log::info!("IndexingDaemon: indexed {} files", {
                let l = index.lock().unwrap();
                l.len()
            });
        });
    }

    pub fn query_tree(&self) -> Value {
        let index = self.index.lock().unwrap();
        let mut files = Vec::new();
        for (rel_path, meta) in index.iter() {
            files.push(json!({
                "path": rel_path.to_string_lossy(),
                "size": meta.size_bytes,
                "symbols_count": meta.symbols.len(),
            }));
        }
        json!({ "files": files })
    }

    pub fn search_symbols(&self, query: &str) -> Vec<(PathBuf, SymbolDefinition)> {
        let index = self.index.lock().unwrap();
        let mut results = Vec::new();
        let query_lower = query.to_lowercase();
        
        for (rel_path, meta) in index.iter() {
            for sym in &meta.symbols {
                if sym.name.to_lowercase().contains(&query_lower) {
                    results.push((rel_path.clone(), sym.clone()));
                }
            }
        }
        results
    }
}

fn walk_dir(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
            if let Ok(file_type) = entry.file_type()
                && file_type.is_symlink() {
                    continue;
                }
            if path.is_dir() {
                paths.extend(walk_dir(&path)?);
            } else if path.is_file() {
                paths.push(path);
            }
        }
    }
    Ok(paths)
}

fn extract_metadata(path: &Path) -> Result<FileMetadata> {
    let fs_meta = std::fs::metadata(path)?;
    let mut symbols = Vec::new();
    let mut dependencies = Vec::new();

    let content = std::fs::read_to_string(path).unwrap_or_default();
    
    // Quick symbol / import heuristic parser to avoid blocking tree-sitter issues
    if path.extension().and_then(|s| s.to_str()) == Some("rs") {
        for (line_no, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("pub fn ") || trimmed.starts_with("fn ") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() > 2 {
                    let name = parts[2].split('(').next().unwrap_or("unknown").to_string();
                    symbols.push(SymbolDefinition { name, kind: "function".to_string(), line: line_no + 1 });
                }
            } else if trimmed.starts_with("pub struct ") || trimmed.starts_with("struct ") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() > 1 {
                    let name = parts[1].split('{').next().unwrap_or("unknown").to_string();
                    symbols.push(SymbolDefinition { name, kind: "struct".to_string(), line: line_no + 1 });
                }
            } else if trimmed.starts_with("use ") {
                dependencies.push(trimmed.to_string());
            }
        }
    }

    Ok(FileMetadata {
        path: path.to_path_buf(),
        size_bytes: fs_meta.len(),
        symbols,
        dependencies,
    })
}

// ═══════════════════════════════════════════════════════
// 6. CodeAct Gate and Interceptors
// ═══════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub enum RedirectTarget {
    ListDir { path: String },
    ReadFile { path: String },
    GrepFiles { query: String, path: String },
}

pub struct CodeActGate;

impl CodeActGate {
    /// Extract the base command and args from a shell command string.
    /// Handles `ls -la src/`, `cat -n file.rs`, `grep -r pattern .` etc.
    fn parse_shell_command(cmd: &str) -> Option<(&str, Vec<&str>)> {
        let trimmed = cmd.trim();
        if trimmed.is_empty() {
            return None;
        }
        // Simple space-based split: first token is the command, rest are args
        let mut parts = trimmed.split_whitespace();
        let command = parts.next()?;
        let args: Vec<&str> = parts.collect();
        Some((command, args))
    }

    pub fn should_redirect(tool: &str, args: &Value) -> Option<RedirectTarget> {
        if tool == "shell" {
            let cmd = args.get("command").and_then(|s| s.as_str()).unwrap_or("").trim();
            let (base_cmd, cmd_args) = Self::parse_shell_command(cmd)?;

            match base_cmd {
                "ls" | "find" | "eza" | "exa" => {
                    // Extract first non-flag argument as the path
                    let path = cmd_args.iter().find(|a| !a.starts_with('-')).unwrap_or(&".").to_string();
                    Some(RedirectTarget::ListDir { path })
                }
                "cat" | "head" | "tail" | "less" | "more" | "nl" => {
                    let path = cmd_args.iter().find(|a| !a.starts_with('-')).unwrap_or(&"").to_string();
                    if path.is_empty() { None } else { Some(RedirectTarget::ReadFile { path }) }
                }
                "grep" | "rg" | "ripgrep" | "ag" | "ack" => {
                    // Find the first non-flag argument as the pattern
                    let query = cmd_args.iter().find(|a| !a.starts_with('-')).unwrap_or(&"").to_string();
                    if query.is_empty() { None } else { Some(RedirectTarget::GrepFiles { query, path: ".".to_string() }) }
                }
                _ => None,
            }
        } else if tool == "codeact" {
            let script = args.get("script").and_then(|s| s.as_str()).unwrap_or("").trim();
            
            // Check if it's a simple single-line list_dir/read_file statement in Rhai/Python
            if (script.contains("list_dir") || script.contains("std::fs::read_dir")) 
               && script.lines().count() <= 2 {
                return Some(RedirectTarget::ListDir { path: ".".to_string() });
            }
            if (script.contains("read_file") || script.contains("std::fs::read_to_string"))
               && script.lines().count() <= 2 {
                return Some(RedirectTarget::ReadFile { path: ".".to_string() });
            }
            None
        } else {
            None
        }
    }
}

// ═══════════════════════════════════════════════════════
// 7. Tool Scheduler Orchestrator
// ═══════════════════════════════════════════════════════

pub struct ToolScheduler {
    pub budget: Arc<Mutex<ExplorationBudget>>,
    pub policy_engine: Arc<ToolPolicyEngine>,
    pub attention_scheduler: Arc<AttentionScheduler>,
    pub daemon: Arc<DirectoryIndexingDaemon>,
}

impl ToolScheduler {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            budget: Arc::new(Mutex::new(ExplorationBudget::default())),
            policy_engine: Arc::new(ToolPolicyEngine::new()),
            attention_scheduler: Arc::new(AttentionScheduler::new()),
            daemon: Arc::new(DirectoryIndexingDaemon::new(workspace_root)),
        }
    }

    /// Pre-flight guard checks and executes tool actions safely, enforcing budgets and policies.
    pub fn pre_execute(&self, tool: &str, args: &Value) -> Result<()> {
        // Enforce tool policy (cooldowns, require reasons)
        self.policy_engine.enforce(tool, args)?;

        // Update budget count
        let mut budget = self.budget.lock().unwrap();
        if tool == "list_dir" || tool == "grep_search" || tool == "view_file" {
            budget.files_explored += 1;
        }
        
        // Enforce strict exploration constraints
        budget.enforce_limit()?;
        Ok(())
    }
}
