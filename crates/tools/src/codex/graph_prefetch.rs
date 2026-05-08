use async_trait::async_trait;
use pharmakon_common::{AgentResult, Tool, ToolCategory};
use serde_json::{json, Value};
use crate::planning_tool;
use crate::codex::utils::scan_diff_risks;

planning_tool!(
    GraphPrefetchTool,
    "graph_prefetch",
    "Suggest likely next context nodes from weighted graph edges.",
    ToolCategory::System
);

