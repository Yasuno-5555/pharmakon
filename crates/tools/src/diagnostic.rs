use async_trait::async_trait;
use serde_json::{Value, json};
use pharmakon_common::{Tool, AgentResult, AgentError, ToolCategory};
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::media::vision_stream::VisionRingBuffer;

pub struct DiagnosticTool {
    pub vision_stream: Option<Arc<Mutex<VisionRingBuffer>>>,
    pub telemetry: Option<Arc<Mutex<pharmakon_common::telemetry::SystemTelemetry>>>,
    pub mcp_stats_source: String,
}

#[async_trait]
impl Tool for DiagnosticTool {
    fn name(&self) -> &str { "self_diagnostic" }
    fn description(&self) -> &str { "Inspect agent's own health, performance metrics, and recent visual short-term memory." }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "aspect": { 
                    "type": "string", 
                    "enum": ["performance", "vision", "resources", "pc_stats"],
                    "description": "The aspect of self-health to inspect" 
                }
            },
            "required": ["aspect"]
        })
    }

    fn category(&self) -> ToolCategory { ToolCategory::System }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let aspect = args["aspect"].as_str().unwrap_or("performance");

        match aspect {
            "performance" => {
                // Return dummy stats for now, in a real implementation we would query the metrics engine
                Ok(json!({
                    "mcp_latency": {
                        "brave_search": "450ms",
                        "shell": "12ms"
                    },
                    "average_ttft": "800ms",
                    "status": "Healthy"
                }).to_string())
            },
            "vision" => {
                if let Some(stream) = &self.vision_stream {
                    let stream_lock = stream.lock().await;
                    let frames = stream_lock.get_recent_frames();
                    let summary = frames.iter().map(|f| {
                        format!("- At {}: Focused on Window '{}'", f.captured_at, f.window_title)
                    }).collect::<Vec<_>>().join("\n");
                    
                    if summary.is_empty() {
                        Ok("No visual context captured yet.".to_string())
                    } else {
                        Ok(format!("Recent Visual History:\n{}", summary))
                    }
                } else {
                    Ok("Vision system not initialized.".to_string())
                }
            },
            "resources" => {
                Ok(json!({
                    "memory_usage": "142MB",
                    "threads_active": 24,
                    "uptime": "1h 12m"
                }).to_string())
            },
            "pc_stats" => {
                if let Some(tel) = &self.telemetry {
                    let mut tel_lock = tel.lock().await;
                    let sample = tel_lock.sample();
                    let summary = tel_lock.get_summary_24h();
                    Ok(json!({
                        "current": sample,
                        "summary": summary
                    }).to_string())
                } else {
                    Ok("PC Telemetry not initialized.".to_string())
                }
            },
            _ => Err(AgentError("Unknown diagnostic aspect".to_string()))
        }
    }
}
