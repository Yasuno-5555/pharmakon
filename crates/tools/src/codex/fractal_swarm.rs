use async_trait::async_trait;
use pharmakon_common::{AgentResult, Tool, ToolCategory};
use serde_json::{json, Value};
use crate::planning_tool;
use crate::codex::utils::scan_diff_risks;

planning_tool!(
    FractalSwarmTool,
    "fractal_swarm",
    "Decompose a task into nested micro-agent work packets without spawning processes.",
    ToolCategory::Autonomous
);
