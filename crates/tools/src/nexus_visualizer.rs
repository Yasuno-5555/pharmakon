use crate::codex_utils::state_dir;
use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;

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
