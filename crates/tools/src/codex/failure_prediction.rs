use async_trait::async_trait;
use pharmakon_common::{AgentResult, Tool, ToolCategory};
use serde_json::{json, Value};
use crate::planning_tool;
use crate::codex::utils::scan_diff_risks;

planning_tool!(
    FailurePredictionTool,
    "failure_prediction",
    "Predict likely execution failures before running a command or patch.",
    ToolCategory::System
);
