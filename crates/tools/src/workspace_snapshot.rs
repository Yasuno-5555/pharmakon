use crate::codex_utils::{
    is_probably_binary, metadata_modified_secs, now, read_json, state_dir, write_json,
};
use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

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
