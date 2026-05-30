use crate::codex_utils::{now, read_json, state_dir, write_json};
use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;

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
