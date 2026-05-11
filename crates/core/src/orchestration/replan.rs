//! 🔄 Incremental Replanning Engine — Phase 8
//!
//! Isolates a failed AST node inside a hierarchical plan, matches dependencies,
//! and dynamically replaces it with a self-healing alternative sub-tree.

use crate::orchestration::world::PlanNode;

pub struct IncrementalPlanner;

impl Default for IncrementalPlanner {
    fn default() -> Self {
        Self::new()
    }
}

impl IncrementalPlanner {
    pub fn new() -> Self {
        Self
    }

    /// Recursively search for a plan step/node by target tool and swap it out with a replacement.
    pub fn replan_node(
        &self,
        root: &PlanNode,
        target_tool: &str,
        replacement: PlanNode,
    ) -> PlanNode {
        match root {
            PlanNode::Step { tool, args, dry_run_first } => {
                if tool == target_tool {
                    replacement
                } else {
                    PlanNode::Step {
                        tool: tool.clone(),
                        args: args.clone(),
                        dry_run_first: *dry_run_first,
                    }
                }
            }
            PlanNode::Sequence { nodes } => {
                let mut new_nodes = Vec::new();
                for node in nodes {
                    new_nodes.push(self.replan_node(node, target_tool, replacement.clone()));
                }
                PlanNode::Sequence { nodes: new_nodes }
            }
            PlanNode::Parallel { nodes } => {
                let mut new_nodes = Vec::new();
                for node in nodes {
                    new_nodes.push(self.replan_node(node, target_tool, replacement.clone()));
                }
                PlanNode::Parallel { nodes: new_nodes }
            }
            PlanNode::Conditional { condition_script, then_branch, else_branch } => {
                PlanNode::Conditional {
                    condition_script: condition_script.clone(),
                    then_branch: Box::new(self.replan_node(then_branch, target_tool, replacement.clone())),
                    else_branch: else_branch.as_ref().map(|b| Box::new(self.replan_node(b, target_tool, replacement.clone()))),
                }
            }
            PlanNode::Retry { node, max_attempts } => {
                PlanNode::Retry {
                    node: Box::new(self.replan_node(node, target_tool, replacement.clone())),
                    max_attempts: *max_attempts,
                }
            }
            PlanNode::Verify { node, assertion_script } => {
                PlanNode::Verify {
                    node: Box::new(self.replan_node(node, target_tool, replacement.clone())),
                    assertion_script: assertion_script.clone(),
                }
            }
            PlanNode::Gate { gate_name, node } => {
                PlanNode::Gate {
                    gate_name: gate_name.clone(),
                    node: Box::new(self.replan_node(node, target_tool, replacement.clone())),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_incremental_replan() {
        let planner = IncrementalPlanner::new();

        let original = PlanNode::Sequence {
            nodes: vec![
                PlanNode::Step {
                    tool: "grep_search".to_string(),
                    args: serde_json::json!({}),
                    dry_run_first: false,
                },
                PlanNode::Step {
                    tool: "failing_tool".to_string(),
                    args: serde_json::json!({}),
                    dry_run_first: false,
                },
            ],
        };

        let replacement = PlanNode::Step {
            tool: "recovered_tool".to_string(),
            args: serde_json::json!({}),
            dry_run_first: true,
        };

        let result = planner.replan_node(&original, "failing_tool", replacement);

        match result {
            PlanNode::Sequence { nodes } => {
                assert_eq!(nodes.len(), 2);
                match &nodes[1] {
                    PlanNode::Step { tool, dry_run_first, .. } => {
                        assert_eq!(tool, "recovered_tool");
                        assert!(*dry_run_first);
                    }
                    _ => panic!("Expected step node"),
                }
            }
            _ => panic!("Expected sequence node"),
        }
    }
}
