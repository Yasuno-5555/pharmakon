use async_trait::async_trait;
use pharmakon_common::{AgentResult, Tool, ToolCategory};
use serde_json::{json, Value};
use crate::planning_tool;
use crate::codex::utils::scan_diff_risks;

planning_tool!(
    RegretMinimizationTool,
    "regret_minimization",
    "Rank options by penalizing known regret and failure signals.",
    ToolCategory::Autonomous
);
