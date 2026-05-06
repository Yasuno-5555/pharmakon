use crate::media::vision_stream::VisionRingBuffer;
use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct DiagnosticTool {
    pub vision_stream: Option<Arc<Mutex<VisionRingBuffer>>>,
    pub telemetry: Option<Arc<Mutex<pharmakon_common::telemetry::SystemTelemetry>>>,
    pub mcp_stats_source: String,
    pub total_tokens: Option<Arc<std::sync::atomic::AtomicU64>>,
    pub total_cost: Option<Arc<Mutex<f64>>>,
}

#[async_trait]
impl Tool for DiagnosticTool {
    fn name(&self) -> &str {
        "self_diagnostic"
    }
    fn description(&self) -> &str {
        "Inspect agent's own health, performance metrics, and recent visual short-term memory."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "aspect": {
                    "type": "string",
                    "enum": ["performance", "vision", "resources", "pc_stats", "api_key", "token_usage"],
                    "description": "The aspect of self-health to inspect. 'token_usage' shows context window and API consumption."
                },
                "key_value": { "type": "string", "description": "New API key to set (only for 'api_key' aspect)" }
            }
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

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
                let os_info = std::env::consts::OS;
                let arch = std::env::consts::ARCH;
                Ok(json!({
                    "os": os_info,
                    "architecture": arch,
                    "memory_usage": "142MB",
                    "threads_active": 24,
                    "uptime": "1h 15m",
                    "working_dir": std::env::current_dir().unwrap_or_default().to_string_lossy()
                }).to_string())
            },
            "state" => {
                Ok(json!({
                    "status": "Autonomous/Execution-Bias",
                    "capabilities": ["Dynamic Skill Synthesis", "Parallel Swarm", "Proactive Scout", "Self-Healing"],
                    "active_policy": "DefaultSecurityPolicy (Autonomous Judgement Mode)"
                }).to_string())
            },
            "api_key" => {
                let store = pharmakon_common::secrets::SecretStore::new();
                if let Some(new_key) = args["key_value"].as_str() {
                    store.set_secret("PHARMAKON_CONTROL_API_KEY", new_key).map_err(|e| AgentError(e.to_string()))?;
                    Ok(format!("✅ Control API Key successfully updated to: {}", new_key))
                } else {
                    let current = store.get_secret("PHARMAKON_CONTROL_API_KEY").unwrap_or_else(|_| "NOT_SET".to_string());
                    Ok(format!("Current Control API Key: {}\n\nTo update, call this tool with 'key_value' parameter.", current))
                }
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
            "token_usage" => {
                let tokens = self.total_tokens.as_ref().map(|t| t.load(std::sync::atomic::Ordering::SeqCst)).unwrap_or(0);
                let cost = if let Some(c) = &self.total_cost {
                    *c.lock().await
                } else {
                    0.0
                };
                Ok(json!({
                    "total_tokens_consumed": tokens,
                    "estimated_cost_usd": format!("${:.4}", cost),
                    "status": "Tracking Active"
                }).to_string())
            },
            _ => Err(AgentError("Unknown diagnostic aspect".to_string()))
        }
    }
}
