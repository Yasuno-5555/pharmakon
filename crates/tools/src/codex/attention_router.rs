use async_trait::async_trait;
use pharmakon_common::{AgentResult, Tool, ToolCategory};
use serde_json::{json, Value};
use crate::planning_tool;
use crate::codex::utils::scan_diff_risks;

planning_tool!(
    AttentionRouterTool,
    "attention_router",
    "Score information by relevance, novelty, and reliability to decide what deserves attention.",
    ToolCategory::System
);
