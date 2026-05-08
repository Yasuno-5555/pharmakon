use async_trait::async_trait;
use pharmakon_common::{AgentResult, Tool, ToolCategory};
use serde_json::{json, Value};
use crate::planning_tool;
use crate::codex::utils::scan_diff_risks;

planning_tool!(
    CounterfactualSimulatorTool,
    "counterfactual_simulator",
    "Compare alternative branches such as Tool A vs Tool B or Patch X vs Patch Y.",
    ToolCategory::Autonomous
);
