use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, UNIX_EPOCH};

fn state_dir(name: &str) -> AgentResult<PathBuf> {
    let base = dirs::home_dir()
        .ok_or_else(|| AgentError("Could not find home directory".to_string()))?
        .join(".pharmakon")
        .join(name);
    fs::create_dir_all(&base)
        .map_err(|e| AgentError(format!("Failed to create state dir: {}", e)))?;
    Ok(base)
}

fn read_json<T: DeserializeOwned + Default>(path: &Path) -> AgentResult<T> {
    if !path.exists() {
        return Ok(T::default());
    }
    let data = fs::read_to_string(path)
        .map_err(|e| AgentError(format!("Failed to read {}: {}", path.display(), e)))?;
    serde_json::from_str(&data)
        .map_err(|e| AgentError(format!("Failed to parse {}: {}", path.display(), e)))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> AgentResult<()> {
    let data = serde_json::to_string_pretty(value)
        .map_err(|e| AgentError(format!("Failed to serialize json: {}", e)))?;
    fs::write(path, data)
        .map_err(|e| AgentError(format!("Failed to write {}: {}", path.display(), e)))
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn short_hash(input: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    input.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn estimate_tokens(text: &str) -> usize {
    (text.chars().count() / 4).max(1)
}

fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|s| s.len() > 1)
        .map(|s| s.to_ascii_lowercase())
        .collect()
}

fn is_probably_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(1024).any(|b| *b == 0)
}

fn metadata_modified_secs(path: &Path) -> u64 {
    path.metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

fn scan_diff_risks(text: &str) -> Vec<String> {
    let mut risks = Vec::new();
    let lower = text.to_ascii_lowercase();
    let secret_markers = [
        "api_key",
        "apikey",
        "secret",
        "private_key",
        "access_token",
        "bearer ",
        "password",
        "BEGIN RSA PRIVATE KEY",
        "BEGIN OPENSSH PRIVATE KEY",
    ];
    for marker in secret_markers {
        if lower.contains(&marker.to_ascii_lowercase()) {
            risks.push(format!(
                "Possible secret material or credential marker: {}",
                marker
            ));
        }
    }
    if lower.contains("chmod 777") {
        risks.push("World-writable permission change detected".to_string());
    }
    if lower.contains("rm -rf /") || lower.contains("rm -rf *") {
        risks.push("Dangerous recursive removal command detected".to_string());
    }
    if lower.contains("unsafe {") {
        risks.push("Rust unsafe block introduced".to_string());
    }
    if lower.contains("select ") && lower.contains("format!(") {
        risks.push("Possible SQL construction through string formatting".to_string());
    }
    if lower.contains("std::env::set_var") {
        risks.push("Process environment mutation detected".to_string());
    }
    risks
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct TraceStep {
    timestamp: String,
    step_type: String,
    content: Option<String>,
    tool: Option<String>,
    args: Option<Value>,
    output: Option<String>,
    success: Option<bool>,
    latency_ms: Option<u64>,
}

pub struct ExecutionTraceTool;

#[async_trait]
impl Tool for ExecutionTraceTool {
    fn name(&self) -> &str {
        "execution_trace"
    }

    fn description(&self) -> &str {
        "Record, list, and read structured execution traces for agent thoughts, tool calls, and tool results."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["record", "list", "read"] },
                "trace_id": { "type": "string" },
                "step_type": { "type": "string", "enum": ["thought", "tool_call", "tool_result", "observation", "response"] },
                "content": { "type": "string" },
                "tool": { "type": "string" },
                "args": { "type": "object" },
                "output": { "type": "string" },
                "success": { "type": "boolean" },
                "latency_ms": { "type": "integer" }
            },
            "required": ["action"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let action = args["action"].as_str().unwrap_or("list");
        let dir = state_dir("traces")?;
        match action {
            "record" => {
                let trace_id = args["trace_id"]
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| {
                        format!("trace-{}", chrono::Utc::now().format("%Y%m%d%H%M%S"))
                    });
                let path = dir.join(format!("{}.json", trace_id));
                let mut steps: Vec<TraceStep> = read_json(&path)?;
                steps.push(TraceStep {
                    timestamp: now(),
                    step_type: args["step_type"]
                        .as_str()
                        .unwrap_or("observation")
                        .to_string(),
                    content: args["content"].as_str().map(|s| s.to_string()),
                    tool: args["tool"].as_str().map(|s| s.to_string()),
                    args: args.get("args").cloned(),
                    output: args["output"].as_str().map(|s| s.to_string()),
                    success: args["success"].as_bool(),
                    latency_ms: args["latency_ms"].as_u64(),
                });
                write_json(&path, &steps)?;
                Ok(json!({ "trace_id": trace_id, "steps": steps.len(), "path": path }).to_string())
            }
            "read" => {
                let trace_id = args["trace_id"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing trace_id".to_string()))?;
                let path = dir.join(format!("{}.json", trace_id));
                let steps: Vec<TraceStep> = read_json(&path)?;
                Ok(serde_json::to_string_pretty(&steps).unwrap_or_default())
            }
            "list" => {
                let mut traces = Vec::new();
                for entry in fs::read_dir(&dir).map_err(|e| AgentError(e.to_string()))? {
                    let entry = entry.map_err(|e| AgentError(e.to_string()))?;
                    if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
                        traces.push(entry.file_name().to_string_lossy().replace(".json", ""));
                    }
                }
                traces.sort();
                Ok(json!({ "traces": traces }).to_string())
            }
            _ => Err(AgentError("Unknown execution_trace action".to_string())),
        }
    }
}

pub struct DeterministicReplayTool;

#[async_trait]
impl Tool for DeterministicReplayTool {
    fn name(&self) -> &str {
        "deterministic_replay"
    }

    fn description(&self) -> &str {
        "Replay an execution trace using recorded tool results instead of re-running side effects."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "trace_id": { "type": "string" },
                "mode": { "type": "string", "enum": ["summary", "script", "assert"], "default": "summary" }
            },
            "required": ["trace_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let trace_id = args["trace_id"]
            .as_str()
            .ok_or_else(|| AgentError("Missing trace_id".to_string()))?;
        let mode = args["mode"].as_str().unwrap_or("summary");
        let path = state_dir("traces")?.join(format!("{}.json", trace_id));
        let steps: Vec<TraceStep> = read_json(&path)?;
        let tool_calls = steps.iter().filter(|s| s.step_type == "tool_call").count();
        let failures = steps.iter().filter(|s| s.success == Some(false)).count();
        let replay_script: Vec<Value> = steps
            .iter()
            .map(|s| {
                json!({
                    "at": s.timestamp,
                    "kind": s.step_type,
                    "tool": s.tool,
                    "args": s.args,
                    "mock_output": s.output,
                    "success": s.success
                })
            })
            .collect();

        match mode {
            "script" => Ok(serde_json::to_string_pretty(&replay_script).unwrap_or_default()),
            "assert" => Ok(json!({
                "trace_id": trace_id,
                "deterministic": true,
                "reason": "Replay uses recorded observations and does not call external tools.",
                "steps": steps.len(),
                "tool_calls": tool_calls,
                "recorded_failures": failures
            })
            .to_string()),
            _ => Ok(json!({
                "trace_id": trace_id,
                "steps": steps.len(),
                "tool_calls": tool_calls,
                "recorded_failures": failures,
                "first_step": steps.first(),
                "last_step": steps.last()
            })
            .to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ReliabilityStats {
    successes: u64,
    failures: u64,
    total_latency_ms: u64,
    last_error: Option<String>,
    last_seen: Option<String>,
}

pub struct ToolReliabilityScoringTool;

#[async_trait]
impl Tool for ToolReliabilityScoringTool {
    fn name(&self) -> &str {
        "tool_reliability"
    }

    fn description(&self) -> &str {
        "Track and report tool success rate, failure rate, and average latency for tool selection."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["record", "report"], "default": "report" },
                "tool": { "type": "string" },
                "success": { "type": "boolean" },
                "latency_ms": { "type": "integer" },
                "error": { "type": "string" }
            }
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let path = state_dir("metrics")?.join("tool_reliability.json");
        let mut stats: HashMap<String, ReliabilityStats> = read_json(&path)?;
        if args["action"].as_str().unwrap_or("report") == "record" {
            let tool = args["tool"]
                .as_str()
                .ok_or_else(|| AgentError("Missing tool".to_string()))?
                .to_string();
            let entry = stats.entry(tool.clone()).or_default();
            if args["success"].as_bool().unwrap_or(false) {
                entry.successes += 1;
            } else {
                entry.failures += 1;
                entry.last_error = args["error"].as_str().map(|s| s.to_string());
            }
            entry.total_latency_ms += args["latency_ms"].as_u64().unwrap_or_default();
            entry.last_seen = Some(now());
            write_json(&path, &stats)?;
            return Ok(json!({ "recorded": tool, "path": path }).to_string());
        }

        let mut report = Vec::new();
        for (tool, s) in stats {
            let total = s.successes + s.failures;
            let success_rate = if total == 0 {
                0.0
            } else {
                s.successes as f64 / total as f64
            };
            let avg_latency = if total == 0 {
                0
            } else {
                s.total_latency_ms / total
            };
            report.push(json!({
                "tool": tool,
                "success_rate": success_rate,
                "successes": s.successes,
                "failures": s.failures,
                "avg_latency_ms": avg_latency,
                "last_error": s.last_error,
                "last_seen": s.last_seen
            }));
        }
        report.sort_by(|a, b| {
            b["success_rate"]
                .as_f64()
                .unwrap_or(0.0)
                .partial_cmp(&a["success_rate"].as_f64().unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(serde_json::to_string_pretty(&report).unwrap_or_default())
    }
}

pub struct ContextBudgetOptimizerTool;

#[async_trait]
impl Tool for ContextBudgetOptimizerTool {
    fn name(&self) -> &str {
        "context_budget_optimizer"
    }

    fn description(&self) -> &str {
        "Select the highest-value context items within a token budget using relevance, recency, importance, reliability, and pinned state."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "budget_tokens": { "type": "integer", "default": 4096 },
                "items": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string" },
                            "content": { "type": "string" },
                            "tokens": { "type": "integer" },
                            "relevance": { "type": "number" },
                            "recency": { "type": "number" },
                            "importance": { "type": "number" },
                            "reliability": { "type": "number" },
                            "pinned": { "type": "boolean" }
                        }
                    }
                }
            },
            "required": ["items"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let budget = args["budget_tokens"].as_u64().unwrap_or(4096) as usize;
        let items = args["items"]
            .as_array()
            .ok_or_else(|| AgentError("Missing items".to_string()))?;
        let mut scored = Vec::new();
        for (idx, item) in items.iter().enumerate() {
            let content = item["content"].as_str().unwrap_or_default();
            let tokens = item["tokens"]
                .as_u64()
                .map(|n| n as usize)
                .unwrap_or_else(|| estimate_tokens(content));
            let relevance = item["relevance"].as_f64().unwrap_or(0.5);
            let recency = item["recency"].as_f64().unwrap_or(0.5);
            let importance = item["importance"].as_f64().unwrap_or(0.5);
            let reliability = item["reliability"].as_f64().unwrap_or(0.7);
            let pinned = item["pinned"].as_bool().unwrap_or(false);
            let score = if pinned {
                10_000.0 + importance
            } else {
                (0.45 * relevance + 0.25 * importance + 0.20 * recency + 0.10 * reliability)
                    / (tokens.max(1) as f64).sqrt()
            };
            scored.push((idx, tokens, score, item.clone()));
        }
        scored.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        let mut used = 0usize;
        let mut selected = Vec::new();
        let mut rejected = Vec::new();
        for (idx, tokens, score, item) in scored {
            let id = item["id"]
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| idx.to_string());
            if used + tokens <= budget || item["pinned"].as_bool().unwrap_or(false) {
                used += tokens;
                selected.push(json!({ "id": id, "tokens": tokens, "score": score, "item": item }));
            } else {
                rejected.push(json!({ "id": id, "tokens": tokens, "score": score }));
            }
        }
        Ok(json!({ "budget_tokens": budget, "used_tokens": used, "selected": selected, "rejected": rejected }).to_string())
    }
}

pub struct DryRunTool;

#[async_trait]
impl Tool for DryRunTool {
    fn name(&self) -> &str {
        "dry_run"
    }

    fn description(&self) -> &str {
        "Simulate shell commands, patches, or API calls without performing side effects."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "kind": { "type": "string", "enum": ["shell", "patch", "api"] },
                "command": { "type": "string" },
                "path": { "type": "string" },
                "patch": { "type": "string" },
                "method": { "type": "string" },
                "url": { "type": "string" },
                "body": { "type": "object" }
            },
            "required": ["kind"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        match args["kind"].as_str().unwrap_or("shell") {
            "shell" => {
                let command = args["command"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing command".to_string()))?;
                let risks = scan_diff_risks(command);
                let syntax_check = if command.contains('\n') {
                    Command::new("sh")
                        .arg("-n")
                        .arg("-c")
                        .arg(command)
                        .output()
                        .ok()
                        .map(|o| {
                            json!({
                                "ok": o.status.success(),
                                "stderr": String::from_utf8_lossy(&o.stderr).to_string()
                            })
                        })
                } else {
                    None
                };
                Ok(json!({ "would_execute": command, "side_effects": "not executed", "risks": risks, "syntax_check": syntax_check }).to_string())
            }
            "patch" => {
                let path = args["path"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing path".to_string()))?;
                let patch_str = args["patch"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing patch".to_string()))?;
                let original = fs::read_to_string(path)
                    .map_err(|e| AgentError(format!("Failed to read {}: {}", path, e)))?;
                let patch = diffy::Patch::from_str(patch_str)
                    .map_err(|e| AgentError(format!("Invalid patch: {}", e)))?;
                let patched = diffy::apply(&original, &patch)
                    .map_err(|e| AgentError(format!("Patch would fail: {}", e)))?;
                Ok(json!({
                    "path": path,
                    "applicable": true,
                    "original_bytes": original.len(),
                    "patched_bytes": patched.len(),
                    "risks": scan_diff_risks(patch_str)
                })
                .to_string())
            }
            "api" => Ok(json!({
                "method": args["method"].as_str().unwrap_or("GET"),
                "url": args["url"].as_str().unwrap_or_default(),
                "body_preview": args.get("body"),
                "side_effects": "request not sent"
            })
            .to_string()),
            _ => Err(AgentError("Unknown dry_run kind".to_string())),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SnapshotFile {
    path: String,
    content: String,
    modified_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct WorkspaceSnapshot {
    id: String,
    root: String,
    created_at: String,
    files: Vec<SnapshotFile>,
}

pub struct WorkspaceSnapshotTool;

#[async_trait]
impl Tool for WorkspaceSnapshotTool {
    fn name(&self) -> &str {
        "workspace_snapshot"
    }

    fn description(&self) -> &str {
        "Create, list, inspect, and optionally restore text-file workspace snapshots for long tasks."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["create", "list", "inspect", "restore"] },
                "id": { "type": "string" },
                "root": { "type": "string", "default": "." },
                "max_files": { "type": "integer", "default": 2000 },
                "max_file_bytes": { "type": "integer", "default": 200000 },
                "dry_run": { "type": "boolean", "default": true }
            },
            "required": ["action"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let dir = state_dir("snapshots")?;
        match args["action"].as_str().unwrap_or("list") {
            "create" => {
                let root = args["root"].as_str().unwrap_or(".");
                let root_path = Path::new(root)
                    .canonicalize()
                    .map_err(|e| AgentError(format!("Invalid root {}: {}", root, e)))?;
                let max_files = args["max_files"].as_u64().unwrap_or(2000) as usize;
                let max_file_bytes = args["max_file_bytes"].as_u64().unwrap_or(200000);
                let id = args["id"]
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| {
                        format!("snapshot-{}", chrono::Utc::now().format("%Y%m%d%H%M%S"))
                    });
                let mut files = Vec::new();
                for result in ignore::WalkBuilder::new(&root_path)
                    .hidden(false)
                    .filter_entry(|e| {
                        let name = e.file_name().to_string_lossy();
                        name != ".git" && name != "target" && name != "node_modules"
                    })
                    .build()
                {
                    if files.len() >= max_files {
                        break;
                    }
                    let entry = match result {
                        Ok(e) => e,
                        Err(_) => continue,
                    };
                    if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                        continue;
                    }
                    let meta = match entry.metadata() {
                        Ok(m) => m,
                        Err(_) => continue,
                    };
                    if meta.len() > max_file_bytes {
                        continue;
                    }
                    let bytes = match fs::read(entry.path()) {
                        Ok(b) => b,
                        Err(_) => continue,
                    };
                    if is_probably_binary(&bytes) {
                        continue;
                    }
                    let content = match String::from_utf8(bytes) {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    let rel = entry
                        .path()
                        .strip_prefix(&root_path)
                        .unwrap_or(entry.path())
                        .to_string_lossy()
                        .to_string();
                    files.push(SnapshotFile {
                        path: rel,
                        content,
                        modified_secs: metadata_modified_secs(entry.path()),
                    });
                }
                let snapshot = WorkspaceSnapshot {
                    id: id.clone(),
                    root: root_path.to_string_lossy().to_string(),
                    created_at: now(),
                    files,
                };
                let path = dir.join(format!("{}.json", id));
                write_json(&path, &snapshot)?;
                Ok(json!({ "id": id, "files": snapshot.files.len(), "path": path }).to_string())
            }
            "list" => {
                let mut snapshots = Vec::new();
                for entry in fs::read_dir(&dir).map_err(|e| AgentError(e.to_string()))? {
                    let entry = entry.map_err(|e| AgentError(e.to_string()))?;
                    if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
                        snapshots.push(entry.file_name().to_string_lossy().replace(".json", ""));
                    }
                }
                snapshots.sort();
                Ok(json!({ "snapshots": snapshots }).to_string())
            }
            "inspect" => {
                let id = args["id"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing id".to_string()))?;
                let snapshot: WorkspaceSnapshot = read_json(&dir.join(format!("{}.json", id)))?;
                let files: Vec<Value> = snapshot
                    .files
                    .iter()
                    .map(|f| {
                        json!({
                            "path": f.path,
                            "bytes": f.content.len(),
                            "modified_secs": f.modified_secs
                        })
                    })
                    .collect();
                Ok(json!({ "id": snapshot.id, "root": snapshot.root, "created_at": snapshot.created_at, "files": files }).to_string())
            }
            "restore" => {
                let id = args["id"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing id".to_string()))?;
                let dry_run = args["dry_run"].as_bool().unwrap_or(true);
                let snapshot: WorkspaceSnapshot = read_json(&dir.join(format!("{}.json", id)))?;
                let mut restored = Vec::new();
                for file in &snapshot.files {
                    let path = Path::new(&snapshot.root).join(&file.path);
                    restored.push(path.to_string_lossy().to_string());
                    if !dry_run {
                        if let Some(parent) = path.parent() {
                            fs::create_dir_all(parent).map_err(|e| AgentError(e.to_string()))?;
                        }
                        fs::write(&path, &file.content).map_err(|e| AgentError(e.to_string()))?;
                    }
                }
                Ok(json!({ "id": id, "dry_run": dry_run, "files": restored.len(), "paths": restored }).to_string())
            }
            _ => Err(AgentError("Unknown workspace_snapshot action".to_string())),
        }
    }
}

pub struct SemanticGrepTool;

#[async_trait]
impl Tool for SemanticGrepTool {
    fn name(&self) -> &str {
        "semantic_grep"
    }

    fn description(&self) -> &str {
        "Search code by exact text plus token-overlap semantic scoring. Useful when regular grep is too literal."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "path": { "type": "string", "default": "." },
                "max_results": { "type": "integer", "default": 20 }
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
        let root = args["path"].as_str().unwrap_or(".");
        let max_results = args["max_results"].as_u64().unwrap_or(20) as usize;
        let q_tokens: HashSet<String> = tokenize(query).into_iter().collect();
        let q_lower = query.to_ascii_lowercase();
        let mut matches = Vec::new();
        for result in ignore::WalkBuilder::new(root)
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                name != ".git" && name != "target" && name != "node_modules"
            })
            .build()
        {
            let entry = match result {
                Ok(e) => e,
                Err(_) => continue,
            };
            if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                continue;
            }
            let content = match fs::read_to_string(entry.path()) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let path_text = entry.path().to_string_lossy().to_string();
            let mut best_line = None;
            let mut best_score = 0.0;
            for (idx, line) in content.lines().enumerate() {
                let lower = line.to_ascii_lowercase();
                let line_tokens: HashSet<String> = tokenize(line).into_iter().collect();
                let overlap = q_tokens.intersection(&line_tokens).count() as f64;
                let exact = if lower.contains(&q_lower) { 4.0 } else { 0.0 };
                let filename = if path_text.to_ascii_lowercase().contains(&q_lower) {
                    1.0
                } else {
                    0.0
                };
                let score = exact + filename + overlap / q_tokens.len().max(1) as f64;
                if score > best_score {
                    best_score = score;
                    best_line = Some((idx + 1, line.to_string()));
                }
            }
            if let Some((line, preview)) = best_line
                && best_score > 0.0 {
                    matches.push(json!({
                        "path": path_text,
                        "line": line,
                        "score": best_score,
                        "preview": preview.trim()
                    }));
                }
        }
        matches.sort_by(|a, b| {
            b["score"]
                .as_f64()
                .unwrap_or(0.0)
                .partial_cmp(&a["score"].as_f64().unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        matches.truncate(max_results);
        Ok(serde_json::to_string_pretty(&matches).unwrap_or_default())
    }
}

pub struct WebTaskTool;

#[async_trait]
impl Tool for WebTaskTool {
    fn name(&self) -> &str {
        "web_task"
    }

    fn description(&self) -> &str {
        "One-shot web task: search or fetch a page and return a compact summary with sources."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "url": { "type": "string" },
                "max_chars": { "type": "integer", "default": 4000 }
            }
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Network
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let max_chars = args["max_chars"].as_u64().unwrap_or(4000) as usize;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| AgentError(e.to_string()))?;
        if let Some(url) = args["url"].as_str() {
            let body = client
                .get(url)
                .send()
                .await
                .map_err(|e| AgentError(e.to_string()))?
                .text()
                .await
                .map_err(|e| AgentError(e.to_string()))?;
            let text = scraper::Html::parse_document(&body)
                .root_element()
                .text()
                .collect::<Vec<_>>()
                .join(" ");
            let summary = text
                .split_whitespace()
                .take(max_chars / 6)
                .collect::<Vec<_>>()
                .join(" ");
            return Ok(json!({ "url": url, "summary": summary, "chars": text.len() }).to_string());
        }
        let query = args["query"]
            .as_str()
            .ok_or_else(|| AgentError("Missing query or url".to_string()))?;
        if let Ok(api_key) = std::env::var("BRAVE_API_KEY") {
            let body: Value = client
                .get("https://api.search.brave.com/res/v1/web/search")
                .header("Accept", "application/json")
                .header("X-Subscription-Token", api_key)
                .query(&[("q", query), ("count", "5")])
                .send()
                .await
                .map_err(|e| AgentError(e.to_string()))?
                .json()
                .await
                .map_err(|e| AgentError(e.to_string()))?;
            return Ok(serde_json::to_string_pretty(&body["web"]["results"]).unwrap_or_default());
        }
        let body: Value = client
            .get("https://api.duckduckgo.com/")
            .query(&[("q", query), ("format", "json"), ("no_html", "1")])
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?
            .json()
            .await
            .map_err(|e| AgentError(e.to_string()))?;
        Ok(json!({
            "query": query,
            "abstract": body["AbstractText"],
            "source": body["AbstractURL"],
            "note": "Set BRAVE_API_KEY for richer search results."
        })
        .to_string())
    }
}

pub struct LocalModelRouterTool;

#[async_trait]
impl Tool for LocalModelRouterTool {
    fn name(&self) -> &str {
        "local_model_router"
    }

    fn description(&self) -> &str {
        "Recommend a local-first model route, preferring Ollama when suitable and external providers for harder tasks."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task": { "type": "string" },
                "complexity": { "type": "string", "enum": ["low", "medium", "high"], "default": "medium" },
                "requires_vision": { "type": "boolean", "default": false },
                "requires_current_web": { "type": "boolean", "default": false }
            }
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|e| AgentError(e.to_string()))?;
        let ollama_tags = match client.get("http://localhost:11434/api/tags").send().await {
            Ok(response) => response.json::<Value>().await.ok(),
            Err(_) => None,
        };
        let complexity = args["complexity"].as_str().unwrap_or("medium");
        let requires_vision = args["requires_vision"].as_bool().unwrap_or(false);
        let requires_web = args["requires_current_web"].as_bool().unwrap_or(false);
        let route = if requires_vision || requires_web || complexity == "high" {
            "external_frontier"
        } else if ollama_tags.is_some() {
            "ollama_local"
        } else {
            "configured_default"
        };
        Ok(json!({
            "route": route,
            "reason": {
                "complexity": complexity,
                "requires_vision": requires_vision,
                "requires_current_web": requires_web,
                "ollama_available": ollama_tags.is_some()
            },
            "ollama": ollama_tags
        })
        .to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SkillRecipe {
    name: String,
    created_at: String,
    tools: Vec<String>,
    steps: Vec<String>,
}

pub struct SkillCompositionTool;

#[async_trait]
impl Tool for SkillCompositionTool {
    fn name(&self) -> &str {
        "skill_composition"
    }

    fn description(&self) -> &str {
        "Compose existing tools into reusable recipes such as search -> fetch -> summarize."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["create", "list", "show"] },
                "name": { "type": "string" },
                "tools": { "type": "array", "items": { "type": "string" } },
                "steps": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["action"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Autonomous
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let path = state_dir("skills")?.join("compositions.json");
        let mut recipes: Vec<SkillRecipe> = read_json(&path)?;
        match args["action"].as_str().unwrap_or("list") {
            "create" => {
                let name = args["name"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing name".to_string()))?
                    .to_string();
                let tools = args["tools"]
                    .as_array()
                    .unwrap_or(&Vec::new())
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                let steps = args["steps"]
                    .as_array()
                    .unwrap_or(&Vec::new())
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                recipes.retain(|r| r.name != name);
                recipes.push(SkillRecipe {
                    name: name.clone(),
                    created_at: now(),
                    tools,
                    steps,
                });
                write_json(&path, &recipes)?;
                Ok(json!({ "created": name, "path": path }).to_string())
            }
            "show" => {
                let name = args["name"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing name".to_string()))?;
                let recipe = recipes
                    .into_iter()
                    .find(|r| r.name == name)
                    .ok_or_else(|| AgentError(format!("Recipe not found: {}", name)))?;
                Ok(serde_json::to_string_pretty(&recipe).unwrap_or_default())
            }
            "list" => Ok(serde_json::to_string_pretty(&recipes).unwrap_or_default()),
            _ => Err(AgentError("Unknown skill_composition action".to_string())),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct FailureRecord {
    id: String,
    created_at: String,
    task: String,
    failed_approach: String,
    symptom: String,
    avoidance: String,
}

pub struct FailureMemoryTool;

#[async_trait]
impl Tool for FailureMemoryTool {
    fn name(&self) -> &str {
        "failure_memory"
    }

    fn description(&self) -> &str {
        "Record and search failed approaches so future agents can avoid repeating them."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["record", "search", "list"] },
                "task": { "type": "string" },
                "failed_approach": { "type": "string" },
                "symptom": { "type": "string" },
                "avoidance": { "type": "string" },
                "query": { "type": "string" }
            },
            "required": ["action"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let path = state_dir("memory")?.join("failures.json");
        let mut records: Vec<FailureRecord> = read_json(&path)?;
        match args["action"].as_str().unwrap_or("list") {
            "record" => {
                let task = args["task"].as_str().unwrap_or_default().to_string();
                let failed_approach = args["failed_approach"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let symptom = args["symptom"].as_str().unwrap_or_default().to_string();
                let avoidance = args["avoidance"].as_str().unwrap_or_default().to_string();
                let id = short_hash(&(task.clone() + &failed_approach + &symptom));
                records.retain(|r| r.id != id);
                records.push(FailureRecord {
                    id: id.clone(),
                    created_at: now(),
                    task,
                    failed_approach,
                    symptom,
                    avoidance,
                });
                write_json(&path, &records)?;
                Ok(json!({ "recorded": id, "path": path }).to_string())
            }
            "search" => {
                let query = args["query"]
                    .as_str()
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                let found: Vec<_> = records
                    .into_iter()
                    .filter(|r| {
                        format!(
                            "{} {} {} {}",
                            r.task, r.failed_approach, r.symptom, r.avoidance
                        )
                        .to_ascii_lowercase()
                        .contains(&query)
                    })
                    .collect();
                Ok(serde_json::to_string_pretty(&found).unwrap_or_default())
            }
            "list" => Ok(serde_json::to_string_pretty(&records).unwrap_or_default()),
            _ => Err(AgentError("Unknown failure_memory action".to_string())),
        }
    }
}

pub struct ProactiveInterventionTool;

#[async_trait]
impl Tool for ProactiveInterventionTool {
    fn name(&self) -> &str {
        "proactive_intervention"
    }

    fn description(&self) -> &str {
        "Evaluate planned commands, diffs, or task state and produce prioritized stop/warn/continue interventions."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "content": { "type": "string" },
                "kind": { "type": "string", "default": "plan" }
            },
            "required": ["content"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let content = args["content"].as_str().unwrap_or_default();
        let mut interventions = Vec::new();
        for risk in scan_diff_risks(content) {
            interventions
                .push(json!({ "priority": 9, "action": "stop_and_review", "reason": risk }));
        }
        if content.contains("cargo install") && !content.contains("cargo check") {
            interventions.push(json!({ "priority": 5, "action": "run_build_first", "reason": "Install requested before an explicit build/check step." }));
        }
        if interventions.is_empty() {
            interventions.push(json!({ "priority": 1, "action": "continue", "reason": "No high-confidence intervention triggers fired." }));
        }
        Ok(json!({ "kind": args["kind"].as_str().unwrap_or("plan"), "interventions": interventions }).to_string())
    }
}

pub struct CognitiveMirrorTool;

#[async_trait]
impl Tool for CognitiveMirrorTool {
    fn name(&self) -> &str {
        "cognitive_mirror"
    }

    fn description(&self) -> &str {
        "Compress an agent state into human-readable Goal, Risk, Confidence, and Reason fields."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "goal": { "type": "string" },
                "plan": { "type": "string" },
                "evidence": { "type": "string" },
                "risk_signals": { "type": "array", "items": { "type": "string" } }
            }
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let plan = args["plan"].as_str().unwrap_or_default();
        let risk_count = args["risk_signals"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or_default()
            + scan_diff_risks(plan).len();
        let risk = if risk_count >= 3 {
            "high"
        } else if risk_count > 0 {
            "medium"
        } else {
            "low"
        };
        let confidence = match risk {
            "low" => 0.82,
            "medium" => 0.64,
            _ => 0.42,
        };
        Ok(json!({
            "goal": args["goal"].as_str().unwrap_or("unspecified"),
            "risk": risk,
            "confidence": confidence,
            "reason": if risk_count > 0 { "Risk signals or security patterns were detected." } else { "No major risk signal was detected in the supplied state." },
            "evidence": args["evidence"].as_str().unwrap_or_default()
        }).to_string())
    }
}

pub struct IntentCompilerTool;

#[async_trait]
impl Tool for IntentCompilerTool {
    fn name(&self) -> &str {
        "intent_compiler"
    }

    fn description(&self) -> &str {
        "Compile an ambiguous natural-language instruction into an executable, constraint-aware plan."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "request": { "type": "string" },
                "autonomy_level": { "type": "integer", "default": 2 },
                "constraints": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["request"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Autonomous
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let request = args["request"].as_str().unwrap_or_default();
        let lower = request.to_ascii_lowercase();
        let mut steps = vec![
            "Inspect repository state and documentation".to_string(),
            "Identify the smallest safe implementation path".to_string(),
            "Apply scoped changes".to_string(),
            "Run format, build, and relevant tests".to_string(),
            "Summarize risks and follow-ups".to_string(),
        ];
        if lower.contains("review") || lower.contains("architecture") {
            steps.insert(
                1,
                "Produce architecture review findings before broad edits".to_string(),
            );
        }
        if lower.contains("install") {
            steps.push("Run the requested install command after build passes".to_string());
        }
        Ok(json!({
            "goal": request,
            "autonomy_level": args["autonomy_level"].as_u64().unwrap_or(2),
            "constraints": args["constraints"],
            "steps": steps,
            "exit_criteria": ["cargo check/test succeeds", "requested command succeeds", "review findings are reported"]
        }).to_string())
    }
}

pub struct DiffSecurityAuditorTool;

#[async_trait]
impl Tool for DiffSecurityAuditorTool {
    fn name(&self) -> &str {
        "diff_security_auditor"
    }

    fn description(&self) -> &str {
        "Audit a diff or patch for likely security regressions before applying it."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "diff": { "type": "string" }
            },
            "required": ["diff"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let diff = args["diff"].as_str().unwrap_or_default();
        let risks = scan_diff_risks(diff);
        Ok(json!({
            "approved": risks.is_empty(),
            "risk_count": risks.len(),
            "risks": risks
        })
        .to_string())
    }
}

fn find_rust_function_span(content: &str, name: &str) -> Option<(usize, usize, usize, usize)> {
    let needle = format!("fn {}", name);
    let fn_pos = content.find(&needle)?;
    let brace_start = content[fn_pos..].find('{')? + fn_pos;
    let bytes = content.as_bytes();
    let mut depth = 0i32;
    let mut end = None;
    for i in brace_start..bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    end.map(|e| (fn_pos, brace_start, brace_start + 1, e))
}

pub struct AstNativeMutationTool;

#[async_trait]
impl Tool for AstNativeMutationTool {
    fn name(&self) -> &str {
        "mutate_ast"
    }

    fn description(&self) -> &str {
        "Perform structured Rust mutations such as replacing a function body or whole function, then optionally run rustfmt."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["replace_body", "replace_function"] },
                "path": { "type": "string" },
                "target_node": { "type": "string", "description": "Example: function:calculate_total" },
                "new_code": { "type": "string" },
                "rustfmt": { "type": "boolean", "default": true }
            },
            "required": ["action", "path", "target_node", "new_code"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let action = args["action"].as_str().unwrap_or("replace_body");
        let path = args["path"]
            .as_str()
            .ok_or_else(|| AgentError("Missing path".to_string()))?;
        let target = args["target_node"]
            .as_str()
            .ok_or_else(|| AgentError("Missing target_node".to_string()))?;
        let (_, name) = target
            .split_once(':')
            .ok_or_else(|| AgentError("target_node must look like function:name".to_string()))?;
        let new_code = args["new_code"]
            .as_str()
            .ok_or_else(|| AgentError("Missing new_code".to_string()))?;
        let mut content = fs::read_to_string(path).map_err(|e| AgentError(e.to_string()))?;
        let (fn_start, _brace_start, body_start, fn_end) = find_rust_function_span(&content, name)
            .ok_or_else(|| AgentError(format!("Function not found: {}", name)))?;
        match action {
            "replace_function" => content.replace_range(fn_start..fn_end, new_code),
            "replace_body" => content.replace_range(body_start..fn_end - 1, new_code),
            _ => return Err(AgentError("Unknown mutate_ast action".to_string())),
        }
        fs::write(path, content).map_err(|e| AgentError(e.to_string()))?;
        let rustfmt = args["rustfmt"].as_bool().unwrap_or(true);
        let mut rustfmt_status = None;
        if rustfmt {
            rustfmt_status = Command::new("rustfmt").arg(path).output().ok().map(|o| {
                json!({
                    "success": o.status.success(),
                    "stderr": String::from_utf8_lossy(&o.stderr).to_string()
                })
            });
        }
        Ok(json!({ "path": path, "target_node": target, "action": action, "rustfmt": rustfmt_status }).to_string())
    }
}

pub struct SpecFirstTestTool;

#[async_trait]
impl Tool for SpecFirstTestTool {
    fn name(&self) -> &str {
        "spec_first_test"
    }

    fn description(&self) -> &str {
        "Run a spec-first verification command, usually cargo test/check, and return structured compiler feedback."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "default": "cargo test" },
                "cwd": { "type": "string", "default": "." }
            }
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let command = args["command"].as_str().unwrap_or("cargo test");
        let cwd = args["cwd"].as_str().unwrap_or(".");
        let output = Command::new("sh")
            .arg("-lc")
            .arg(command)
            .current_dir(cwd)
            .output()
            .map_err(|e| AgentError(e.to_string()))?;
        Ok(json!({
            "command": command,
            "cwd": cwd,
            "success": output.status.success(),
            "stdout": String::from_utf8_lossy(&output.stdout).chars().take(6000).collect::<String>(),
            "stderr": String::from_utf8_lossy(&output.stderr).chars().take(6000).collect::<String>()
        }).to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct DecisionRecord {
    id: String,
    created_at: String,
    git_head: String,
    target: String,
    chosen: String,
    rejected: Vec<String>,
    rationale: String,
}

pub struct TimeTravelDebuggerTool;

#[async_trait]
impl Tool for TimeTravelDebuggerTool {
    fn name(&self) -> &str {
        "time_travel_debugger"
    }

    fn description(&self) -> &str {
        "Record and inspect design intent tied to the current git commit for later semantic debugging."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["record", "list", "show"] },
                "id": { "type": "string" },
                "target": { "type": "string" },
                "chosen": { "type": "string" },
                "rejected": { "type": "array", "items": { "type": "string" } },
                "rationale": { "type": "string" }
            },
            "required": ["action"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let path = state_dir("time_travel")?.join("decisions.json");
        let mut records: Vec<DecisionRecord> = read_json(&path)?;
        match args["action"].as_str().unwrap_or("list") {
            "record" => {
                let target = args["target"].as_str().unwrap_or_default().to_string();
                let chosen = args["chosen"].as_str().unwrap_or_default().to_string();
                let rationale = args["rationale"].as_str().unwrap_or_default().to_string();
                let rejected = args["rejected"]
                    .as_array()
                    .unwrap_or(&Vec::new())
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>();
                let git_head = Command::new("git")
                    .args(["rev-parse", "HEAD"])
                    .output()
                    .ok()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "unknown".to_string());
                let id = args["id"]
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| short_hash(&(target.clone() + &chosen + &rationale)));
                records.retain(|r| r.id != id);
                records.push(DecisionRecord {
                    id: id.clone(),
                    created_at: now(),
                    git_head,
                    target,
                    chosen,
                    rejected,
                    rationale,
                });
                write_json(&path, &records)?;
                Ok(json!({ "recorded": id, "path": path }).to_string())
            }
            "show" => {
                let id = args["id"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing id".to_string()))?;
                let record = records
                    .into_iter()
                    .find(|r| r.id == id)
                    .ok_or_else(|| AgentError(format!("Decision not found: {}", id)))?;
                Ok(serde_json::to_string_pretty(&record).unwrap_or_default())
            }
            "list" => Ok(serde_json::to_string_pretty(&records).unwrap_or_default()),
            _ => Err(AgentError(
                "Unknown time_travel_debugger action".to_string(),
            )),
        }
    }
}

pub struct NexusVisualizerTool;

#[async_trait]
impl Tool for NexusVisualizerTool {
    fn name(&self) -> &str {
        "nexus_visualizer"
    }

    fn description(&self) -> &str {
        "Render a lightweight local HTML view of supplied Knowledge Nexus nodes and edges."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "nodes": { "type": "array" },
                "edges": { "type": "array" },
                "output": { "type": "string" }
            }
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let output = args["output"]
            .as_str()
            .map(PathBuf::from)
            .unwrap_or(state_dir("visualizer")?.join("nexus.html"));
        let nodes = args.get("nodes").cloned().unwrap_or_else(|| json!([]));
        let edges = args.get("edges").cloned().unwrap_or_else(|| json!([]));
        let html = format!(
            r#"<!doctype html>
<html><head><meta charset="utf-8"><title>Pharmakon Nexus</title>
<style>body{{font-family:system-ui;margin:20px}}pre{{white-space:pre-wrap}}.grid{{display:grid;grid-template-columns:1fr 1fr;gap:16px}}</style></head>
<body><h1>Pharmakon Knowledge Nexus</h1><div class="grid"><section><h2>Nodes</h2><pre id="nodes"></pre></section><section><h2>Edges</h2><pre id="edges"></pre></section></div>
<script>document.getElementById('nodes').textContent = JSON.stringify({nodes}, null, 2); document.getElementById('edges').textContent = JSON.stringify({edges}, null, 2);</script>
</body></html>"#
        );
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|e| AgentError(e.to_string()))?;
        }
        fs::write(&output, html).map_err(|e| AgentError(e.to_string()))?;
        Ok(json!({ "output": output }).to_string())
    }
}

pub struct SemanticConflictResolutionTool;

#[async_trait]
impl Tool for SemanticConflictResolutionTool {
    fn name(&self) -> &str {
        "semantic_conflict_resolution"
    }

    fn description(&self) -> &str {
        "Resolve conflicting beliefs by preferring source-code truth, explicit authority, and newer evidence."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "beliefs": { "type": "array", "items": { "type": "object" } }
            },
            "required": ["beliefs"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let beliefs = args["beliefs"]
            .as_array()
            .ok_or_else(|| AgentError("Missing beliefs".to_string()))?;
        let mut ranked = Vec::new();
        for belief in beliefs {
            let source = belief["source"].as_str().unwrap_or("note");
            let authority = belief["authority"].as_f64().unwrap_or(0.5);
            let source_boost = match source {
                "source_code" | "code" => 1.0,
                "test" | "compiler" => 0.9,
                "docs" => 0.6,
                _ => 0.4,
            };
            let updated = belief["updated_at"].as_str().unwrap_or("");
            ranked.push(json!({
                "belief": belief,
                "score": authority + source_boost + if updated.is_empty() { 0.0 } else { 0.1 }
            }));
        }
        ranked.sort_by(|a, b| {
            b["score"]
                .as_f64()
                .unwrap_or(0.0)
                .partial_cmp(&a["score"].as_f64().unwrap_or(0.0))
                .unwrap()
        });
        Ok(json!({
            "winner": ranked.first(),
            "deprecated": ranked.iter().skip(1).collect::<Vec<_>>(),
            "policy": "source_code > compiler/test > docs > notes; newer evidence breaks ties"
        })
        .to_string())
    }
}

pub struct ProactiveSelfOptimizationTool;

#[async_trait]
impl Tool for ProactiveSelfOptimizationTool {
    fn name(&self) -> &str {
        "proactive_self_optimization"
    }

    fn description(&self) -> &str {
        "Scan the repository for low-risk improvement opportunities during idle time."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "root": { "type": "string", "default": "." },
                "max_findings": { "type": "integer", "default": 30 }
            }
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Autonomous
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let root = args["root"].as_str().unwrap_or(".");
        let max_findings = args["max_findings"].as_u64().unwrap_or(30) as usize;
        let mut findings = Vec::new();
        for result in ignore::WalkBuilder::new(root)
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                name != ".git" && name != "target" && name != "node_modules"
            })
            .build()
        {
            if findings.len() >= max_findings {
                break;
            }
            let entry = match result {
                Ok(e) => e,
                Err(_) => continue,
            };
            if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                continue;
            }
            let content = match fs::read_to_string(entry.path()) {
                Ok(c) => c,
                Err(_) => continue,
            };
            for (idx, line) in content.lines().enumerate() {
                if line.contains("TODO") || line.contains("unwrap()") || line.contains("expect(") {
                    findings.push(
                        json!({ "path": entry.path(), "line": idx + 1, "signal": line.trim() }),
                    );
                    if findings.len() >= max_findings {
                        break;
                    }
                }
            }
        }
        Ok(json!({ "findings": findings }).to_string())
    }
}

macro_rules! planning_tool {
    ($struct_name:ident, $tool_name:expr, $desc:expr, $category:expr) => {
        pub struct $struct_name;

        #[async_trait]
        impl Tool for $struct_name {
            fn name(&self) -> &str { $tool_name }
            fn description(&self) -> &str { $desc }
            fn parameters(&self) -> Value {
                json!({
                    "type": "object",
                    "properties": {
                        "goal": { "type": "string" },
                        "options": { "type": "array" },
                        "items": { "type": "array" },
                        "context": { "type": "string" },
                        "top_k": { "type": "integer", "default": 5 }
                    }
                })
            }
            fn category(&self) -> ToolCategory { $category }
            async fn call(&self, args: Value) -> AgentResult<String> {
                let goal = args["goal"].as_str().unwrap_or("unspecified");
                let top_k = args["top_k"].as_u64().unwrap_or(5) as usize;
                let options = args["options"].as_array().cloned().unwrap_or_default();
                let mut ranked = Vec::new();
                for (idx, option) in options.iter().enumerate() {
                    let text = option.as_str().map(|s| s.to_string()).unwrap_or_else(|| option.to_string());
                    let risk = scan_diff_risks(&text).len() as f64;
                    ranked.push(json!({
                        "option": option,
                        "score": (1.0 / ((idx + 1) as f64)) - (risk * 0.2),
                        "risk_signals": risk
                    }));
                }
                ranked.truncate(top_k);
                Ok(json!({
                    "tool": $tool_name,
                    "goal": goal,
                    "status": "analysis_ready",
                    "ranked": ranked,
                    "next_step": "Use the ranked output as decision support; this tool does not mutate workspace state."
                }).to_string())
            }
        }
    };
}

planning_tool!(
    RegretMinimizationTool,
    "regret_minimization",
    "Rank options by penalizing known regret and failure signals.",
    ToolCategory::Autonomous
);
planning_tool!(
    CounterfactualSimulatorTool,
    "counterfactual_simulator",
    "Compare alternative branches such as Tool A vs Tool B or Patch X vs Patch Y.",
    ToolCategory::Autonomous
);
planning_tool!(
    AttentionRouterTool,
    "attention_router",
    "Score information by relevance, novelty, and reliability to decide what deserves attention.",
    ToolCategory::System
);
planning_tool!(
    TemporalAwarenessTool,
    "temporal_awareness",
    "Classify information by age and prioritize recent, still-valid evidence.",
    ToolCategory::System
);
planning_tool!(
    SoftDependencyGraphTool,
    "soft_dependency_graph",
    "Represent probable relationships as weighted soft dependencies instead of brittle hard edges.",
    ToolCategory::System
);
planning_tool!(
    FailurePredictionTool,
    "failure_prediction",
    "Predict likely execution failures before running a command or patch.",
    ToolCategory::System
);
planning_tool!(
    MctsSimulatorTool,
    "mcts_simulator",
    "Run lightweight Monte-Carlo-style branch scoring for engineering choices.",
    ToolCategory::Autonomous
);
planning_tool!(
    GraphPrefetchTool,
    "graph_prefetch",
    "Suggest likely next context nodes from weighted graph edges.",
    ToolCategory::System
);
planning_tool!(
    RlfcTool,
    "rlfc",
    "Capture compiler feedback as local reinforcement-learning style improvement signals.",
    ToolCategory::Autonomous
);
planning_tool!(
    EphemeralRedTeamTool,
    "ephemeral_red_team",
    "Generate adversarial tests and abuse cases against a proposed change.",
    ToolCategory::Autonomous
);
planning_tool!(
    FractalSwarmTool,
    "fractal_swarm",
    "Decompose a task into nested micro-agent work packets without spawning processes.",
    ToolCategory::Autonomous
);

pub struct AutonomyDialTool;

#[async_trait]
impl Tool for AutonomyDialTool {
    fn name(&self) -> &str {
        "autonomy_dial"
    }

    fn description(&self) -> &str {
        "Get or set the agent autonomy level: 0 propose, 1 light work, 2 edit allowed, 3 full autonomous."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["get", "set"], "default": "get" },
                "level": { "type": "integer", "minimum": 0, "maximum": 3 }
            }
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let path = state_dir("settings")?.join("autonomy.json");
        let mut value: Value = read_json(&path)?;
        if value.is_null() {
            value = json!({ "level": 2, "updated_at": now() });
        }
        if args["action"].as_str().unwrap_or("get") == "set" {
            let level = args["level"].as_u64().unwrap_or(2).min(3);
            value = json!({ "level": level, "updated_at": now() });
            write_json(&path, &value)?;
        }
        Ok(value.to_string())
    }
}

pub struct MemoryActorStatusTool;

#[async_trait]
impl Tool for MemoryActorStatusTool {
    fn name(&self) -> &str {
        "memory_actor_status"
    }

    fn description(&self) -> &str {
        "Expose the event-sourcing contract for a single Memory Manager Actor and record lightweight memory events."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["status", "append"], "default": "status" },
                "event": { "type": "object" }
            }
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let path = state_dir("memory")?.join("actor_events.json");
        let mut events: Vec<Value> = read_json(&path)?;
        if args["action"].as_str().unwrap_or("status") == "append" {
            let mut event = args.get("event").cloned().unwrap_or_else(|| json!({}));
            event["timestamp"] = json!(now());
            events.push(event);
            write_json(&path, &events)?;
        }
        Ok(json!({
            "actor": "single_writer_memory_manager",
            "events_recorded": events.len(),
            "contract": ["FactDiscovered", "MemoryAccessed", "DecayTriggered", "ConflictResolved", "SnapshotCreated"]
        }).to_string())
    }
}

pub struct AstLspBridgeTool;

#[async_trait]
impl Tool for AstLspBridgeTool {
    fn name(&self) -> &str {
        "ast_lsp_bridge"
    }

    fn description(&self) -> &str {
        "Bridge AST-level intent to rust-analyzer LSP queries for definitions, references, and hover type data."
    }

    fn parameters(&self) -> Value {
        crate::lsp::LspTool::new().parameters()
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        crate::lsp::LspTool::new().call(args).await
    }
}

pub struct NodeReplTool;

#[async_trait]
impl Tool for NodeReplTool {
    fn name(&self) -> &str {
        "node_repl"
    }

    fn description(&self) -> &str {
        "Run a small JavaScript snippet through local node, similar to Codex node_repl for deterministic scripting."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "code": { "type": "string" } },
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
        let output = Command::new("node")
            .arg("-e")
            .arg(code)
            .output()
            .map_err(|e| AgentError(format!("node execution failed: {}", e)))?;
        Ok(json!({
            "success": output.status.success(),
            "stdout": String::from_utf8_lossy(&output.stdout).to_string(),
            "stderr": String::from_utf8_lossy(&output.stderr).to_string()
        })
        .to_string())
    }
}

pub struct CodexAutomationTool;

#[async_trait]
impl Tool for CodexAutomationTool {
    fn name(&self) -> &str {
        "automation"
    }

    fn description(&self) -> &str {
        "Local automation registry for reminders, recurring checks, and thread wakeup-style jobs."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["create", "list", "delete"] },
                "id": { "type": "string" },
                "name": { "type": "string" },
                "schedule": { "type": "string" },
                "prompt": { "type": "string" }
            },
            "required": ["action"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let path = state_dir("automations")?.join("automations.json");
        let mut jobs: Vec<Value> = read_json(&path)?;
        match args["action"].as_str().unwrap_or("list") {
            "create" => {
                let id = args["id"]
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| short_hash(&(args["name"].to_string() + &now())));
                jobs.retain(|j| j["id"].as_str() != Some(&id));
                jobs.push(json!({
                    "id": id,
                    "name": args["name"].as_str().unwrap_or("automation"),
                    "schedule": args["schedule"].as_str().unwrap_or("manual"),
                    "prompt": args["prompt"].as_str().unwrap_or_default(),
                    "created_at": now()
                }));
                write_json(&path, &jobs)?;
                Ok(json!({ "created": id, "path": path }).to_string())
            }
            "delete" => {
                let id = args["id"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing id".to_string()))?;
                jobs.retain(|j| j["id"].as_str() != Some(id));
                write_json(&path, &jobs)?;
                Ok(json!({ "deleted": id }).to_string())
            }
            "list" => Ok(serde_json::to_string_pretty(&jobs).unwrap_or_default()),
            _ => Err(AgentError("Unknown automation action".to_string())),
        }
    }
}

pub struct CurrentTimeTool;

#[async_trait]
impl Tool for CurrentTimeTool {
    fn name(&self) -> &str {
        "current_time"
    }

    fn description(&self) -> &str {
        "Return the current UTC time and a fixed-offset local time."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "utc_offset_hours": { "type": "integer", "default": 0 } }
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let offset = args["utc_offset_hours"].as_i64().unwrap_or(0);
        let utc = chrono::Utc::now();
        let local = utc + chrono::Duration::hours(offset);
        Ok(
            json!({ "utc": utc.to_rfc3339(), "offset_hours": offset, "local": local.to_rfc3339() })
                .to_string(),
        )
    }
}

pub struct WeatherLookupTool;

#[async_trait]
impl Tool for WeatherLookupTool {
    fn name(&self) -> &str {
        "weather_lookup"
    }

    fn description(&self) -> &str {
        "Fetch a compact weather report from wttr.in for a location."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "location": { "type": "string" } },
            "required": ["location"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Network
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let location = args["location"]
            .as_str()
            .ok_or_else(|| AgentError("Missing location".to_string()))?;
        let url = format!("https://wttr.in/{}?format=j1", location.replace(' ', "+"));
        let body: Value = reqwest::get(&url)
            .await
            .map_err(|e| AgentError(e.to_string()))?
            .json()
            .await
            .map_err(|e| AgentError(e.to_string()))?;
        Ok(serde_json::to_string_pretty(&body["current_condition"]).unwrap_or_default())
    }
}

pub struct FinanceLookupTool;

#[async_trait]
impl Tool for FinanceLookupTool {
    fn name(&self) -> &str {
        "finance_lookup"
    }

    fn description(&self) -> &str {
        "Fetch a lightweight public quote CSV from Stooq for a ticker symbol."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "ticker": { "type": "string" } },
            "required": ["ticker"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Network
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let ticker = args["ticker"]
            .as_str()
            .ok_or_else(|| AgentError("Missing ticker".to_string()))?;
        let symbol = if ticker.contains('.') {
            ticker.to_ascii_lowercase()
        } else {
            format!("{}.us", ticker.to_ascii_lowercase())
        };
        let url = format!("https://stooq.com/q/l/?s={}&f=sd2t2ohlcv&h&e=csv", symbol);
        let text = reqwest::get(&url)
            .await
            .map_err(|e| AgentError(e.to_string()))?
            .text()
            .await
            .map_err(|e| AgentError(e.to_string()))?;
        Ok(text)
    }
}

pub struct SportsLookupTool;

#[async_trait]
impl Tool for SportsLookupTool {
    fn name(&self) -> &str {
        "sports_lookup"
    }

    fn description(&self) -> &str {
        "Create a normalized sports lookup request. Connect a sports data MCP/API for live schedules."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "league": { "type": "string" },
                "team": { "type": "string" },
                "date_from": { "type": "string" },
                "date_to": { "type": "string" }
            },
            "required": ["league"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Network
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        Ok(json!({
            "request": args,
            "status": "connector_required",
            "suggested_connector": "MCP sports data server or provider-specific API"
        })
        .to_string())
    }
}

pub struct CodexCatalogTool;

#[async_trait]
impl Tool for CodexCatalogTool {
    fn name(&self) -> &str {
        "codex_tool_catalog"
    }

    fn description(&self) -> &str {
        "List the Pharmakon tools that map to Codex-style capabilities and architecture proposals."
    }

    fn parameters(&self) -> Value {
        json!({ "type": "object", "properties": {} })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, _args: Value) -> AgentResult<String> {
        Ok(json!({
            "filesystem": ["read_file", "write_file", "view_file", "list_dir", "grep_search", "semantic_grep", "apply_patch", "mutate_ast", "workspace_snapshot"],
            "execution": ["shell", "terminal", "background_run", "process_status", "python_interpreter", "node_repl", "dry_run", "spec_first_test"],
            "web": ["web_fetch", "web_search", "web_task", "weather_lookup", "finance_lookup", "sports_lookup", "current_time"],
            "agent_os": ["execution_trace", "deterministic_replay", "tool_reliability", "context_budget_optimizer", "failure_memory", "proactive_intervention", "cognitive_mirror", "intent_compiler", "autonomy_dial"],
            "knowledge_nexus": ["ingest_ast_knowledge", "ast_lsp_bridge", "semantic_conflict_resolution", "time_travel_debugger", "nexus_visualizer", "graph_prefetch", "memory_actor_status"],
            "evolution": ["skill_composition", "regret_minimization", "counterfactual_simulator", "mcts_simulator", "rlfc", "ephemeral_red_team", "fractal_swarm", "proactive_self_optimization"]
        }).to_string())
    }
}
