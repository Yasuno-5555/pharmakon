use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde_json::{json, Value};
use crate::codex::execution_trace::TraceStep;
use crate::codex::utils::{state_dir, read_json};

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
