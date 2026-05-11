use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::codex_utils::{state_dir, read_json, write_json, now};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TraceStep {
    pub timestamp: String,
    pub step_type: String,
    pub content: Option<String>,
    pub tool: Option<String>,
    pub args: Option<Value>,
    pub output: Option<String>,
    pub success: Option<bool>,
    pub latency_ms: Option<u64>,
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
                for entry in std::fs::read_dir(&dir).map_err(|e| AgentError(e.to_string()))? {
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
