use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use crate::codex::utils::{tokenize};

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
