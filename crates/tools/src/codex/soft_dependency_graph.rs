use async_trait::async_trait;
use pharmakon_common::{AgentResult, Tool, ToolCategory};
use serde_json::{json, Value};
use crate::planning_tool;
use crate::codex::utils::scan_diff_risks;

planning_tool!(
    SoftDependencyGraphTool,
    "soft_dependency_graph",
    "Represent probable relationships as weighted soft dependencies instead of brittle hard edges.",
    ToolCategory::System
);
