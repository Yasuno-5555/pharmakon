use async_trait::async_trait;
use pharmakon_common::{AgentResult, Tool, ToolCategory};
use serde_json::{json, Value};
use crate::planning_tool;
use crate::codex::utils::scan_diff_risks;

planning_tool!(
    EphemeralRedTeamTool,
    "ephemeral_red_team",
    "Generate adversarial tests and abuse cases against a proposed change.",
    ToolCategory::Autonomous
);
