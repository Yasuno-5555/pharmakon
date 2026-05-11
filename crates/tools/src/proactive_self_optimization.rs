use async_trait::async_trait;
use pharmakon_common::{AgentResult, Tool, ToolCategory};
use serde_json::{json, Value};
use std::fs;
use crate::codex_utils::scan_diff_risks;

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
