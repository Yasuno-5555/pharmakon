use async_trait::async_trait;
use pharmakon_common::visual_primitives::CanvasPrimitive;
use pharmakon_common::{AgentError, AgentResult, Event, Tool};
use serde_json::{Value, json};
use tokio::sync::broadcast;

pub struct CanvasTool {
    event_tx: broadcast::Sender<Event>,
}

impl CanvasTool {
    pub fn new(event_tx: broadcast::Sender<Event>) -> Self {
        Self { event_tx }
    }
}

#[async_trait]
impl Tool for CanvasTool {
    fn name(&self) -> &str {
        "canvas"
    }
    fn description(&self) -> &str {
        "Draw on a shared canvas. Use this to visualize data or draw diagrams."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["draw", "clear"] },
                "primitive": {
                    "type": "object",
                    "properties": {
                        "type": { "type": "string", "enum": ["Rectangle", "Circle", "Text", "Line"] },
                        "x": { "type": "number" },
                        "y": { "type": "number" },
                        "width": { "type": "number" },
                        "height": { "type": "number" },
                        "radius": { "type": "number" },
                        "x1": { "type": "number" },
                        "y1": { "type": "number" },
                        "x2": { "type": "number" },
                        "y2": { "type": "number" },
                        "content": { "type": "string" },
                        "size": { "type": "number" },
                        "color": { "type": "string" }
                    }
                }
            },
            "required": ["action"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let action = args["action"]
            .as_str()
            .ok_or_else(|| AgentError("Missing action".to_string()))?;

        match action {
            "draw" => {
                let primitive_val = args["primitive"].clone();
                let primitive: CanvasPrimitive =
                    serde_json::from_value(primitive_val).map_err(|e| AgentError(e.to_string()))?;

                let _ = self.event_tx.send(Event::CanvasUpdate { primitive });
                Ok("Drawing primitive...".to_string())
            }
            "clear" => {
                let _ = self.event_tx.send(Event::CanvasClear);
                Ok("Canvas cleared.".to_string())
            }
            _ => Err(AgentError("Unknown action".to_string())),
        }
    }
}
