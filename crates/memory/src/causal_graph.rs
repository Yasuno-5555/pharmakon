//! 🟢 Causal Graph Agent Memory — Phase 8
//!
//! Implements directed causal graphs tracking planning, execution, and validation outcomes.
//! Provides probabilistic counterfactual reasoning, backward root cause analysis (RCA),
//! and persistent serialization.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "data")]
pub enum CausalNodeType {
    Planning { task: String },
    Execution { tool: String, success: bool },
    Validation { check: String, success: bool },
    Outcome { success: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalNode {
    pub id: String,
    pub node_type: CausalNodeType,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CausalEdgeType {
    Triggers,
    LeadsTo,
    Resolves,
    FailsDueTo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalEdge {
    pub source: String,
    pub target: String,
    pub edge_type: CausalEdgeType,
    pub probability: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CausalGraph {
    pub nodes: HashMap<String, CausalNode>,
    pub edges: Vec<CausalEdge>,
}

impl CausalGraph {
    pub fn load() -> Self {
        let path = dirs::home_dir().unwrap_or_default().join(".pharmakon/causal_memory.json");
        if path.exists()
            && let Ok(content) = std::fs::read_to_string(path)
                && let Ok(graph) = serde_json::from_str(&content) {
                    return graph;
                }
        Self::default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = dirs::home_dir().unwrap_or_default().join(".pharmakon/causal_memory.json");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn add_node(&mut self, id: String, node_type: CausalNodeType) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.nodes.insert(id.clone(), CausalNode { id, node_type, timestamp });
    }

    pub fn add_edge(&mut self, source: String, target: String, edge_type: CausalEdgeType, probability: f64) {
        self.edges.push(CausalEdge { source, target, edge_type, probability });
    }

    /// Performs backward walk Root Cause Analysis (RCA) starting from a failed node.
    /// Traverses backward causal paths to identify the root initiator step or tool.
    pub fn root_cause_analysis(&self, start_failed_node_id: &str) -> Vec<String> {
        let mut rca_path = Vec::new();
        let mut current_id = start_failed_node_id.to_string();

        rca_path.push(current_id.clone());

        // Breadth-First-Search or DFS backward traversal along edges
        loop {
            let mut parents = Vec::new();
            for edge in &self.edges {
                if edge.target == current_id {
                    // Causal backward relationship
                    parents.push((edge.source.clone(), edge.edge_type));
                }
            }

            if parents.is_empty() {
                break;
            }

            // Prioritize tracing back through FailsDueTo or LeadsTo
            parents.sort_by_key(|(_, kind)| match kind {
                CausalEdgeType::FailsDueTo => 0,
                CausalEdgeType::LeadsTo => 1,
                _ => 2,
            });

            let next_parent = parents[0].0.clone();
            rca_path.push(next_parent.clone());
            current_id = next_parent;

            // Simple loop prevention
            if rca_path.len() > 50 {
                break;
            }
        }

        rca_path
    }

    /// Estimates conditional success probability P(Success | Alternative Choice)
    /// based on historical probabilities of alternate routing decisions.
    pub fn counterfactual_probability(&self, task: &str, alternative_path_action: &str) -> f64 {
        let matching_planning_nodes: Vec<_> = self.nodes.values()
            .filter(|n| match &n.node_type {
                CausalNodeType::Planning { task: t } => t.contains(task),
                _ => false,
            })
            .collect();

        if matching_planning_nodes.is_empty() {
            return 0.5; // Neutral prior
        }

        let mut cumulative_prob = 0.0;
        let mut match_count = 0;

        for node in matching_planning_nodes {
            // Find execution edges matching the alternative path action name
            for edge in &self.edges {
                if edge.source == node.id
                    && let Some(target_node) = self.nodes.get(&edge.target)
                        && let CausalNodeType::Execution { tool, success } = &target_node.node_type
                            && tool.contains(alternative_path_action) {
                                cumulative_prob += if *success { edge.probability } else { 1.0 - edge.probability };
                                match_count += 1;
                            }
            }
        }

        if match_count == 0 {
            return 0.5;
        }

        cumulative_prob / (match_count as f64)
    }

    /// Recommends the highest-probability path structure based on historical outcomes.
    pub fn recommend_policy(&self, task: &str, alternative_actions: &[&str]) -> Option<String> {
        let mut best_action = None;
        let mut highest_prob = -1.0;

        for action in alternative_actions {
            let prob = self.counterfactual_probability(task, action);
            if prob > highest_prob {
                highest_prob = prob;
                best_action = Some(action.to_string());
            }
        }

        best_action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_causal_graph_construction_and_rca() {
        let mut graph = CausalGraph::default();

        // Task Planning node
        graph.add_node("node-1".to_string(), CausalNodeType::Planning { task: "fix build errors".to_string() });
        // Execution nodes
        graph.add_node("node-2".to_string(), CausalNodeType::Execution { tool: "write_file".to_string(), success: true });
        graph.add_node("node-3".to_string(), CausalNodeType::Execution { tool: "run_command".to_string(), success: false });
        // Outcome node
        graph.add_node("node-4".to_string(), CausalNodeType::Outcome { success: false });

        // Connect nodes to build a causal graph
        graph.add_edge("node-1".to_string(), "node-2".to_string(), CausalEdgeType::Triggers, 0.95);
        graph.add_edge("node-2".to_string(), "node-3".to_string(), CausalEdgeType::LeadsTo, 0.90);
        graph.add_edge("node-3".to_string(), "node-4".to_string(), CausalEdgeType::FailsDueTo, 0.99);

        // Perform Root Cause Analysis starting from the failed Outcome node-4
        let rca = graph.root_cause_analysis("node-4");
        assert_eq!(rca.len(), 4);
        assert_eq!(rca[0], "node-4");
        assert_eq!(rca[1], "node-3"); // Traced directly to the failed execution of run_command!
        assert_eq!(rca[3], "node-1"); // Traced back to planning!
    }

    #[test]
    fn test_counterfactual_reasoning() {
        let mut graph = CausalGraph::default();

        graph.add_node("p1".to_string(), CausalNodeType::Planning { task: "update library dependencies".to_string() });
        graph.add_node("e1".to_string(), CausalNodeType::Execution { tool: "cargo upgrade".to_string(), success: true });
        graph.add_node("p2".to_string(), CausalNodeType::Planning { task: "update library dependencies".to_string() });
        graph.add_node("e2".to_string(), CausalNodeType::Execution { tool: "manual edit".to_string(), success: false });

        graph.add_edge("p1".to_string(), "e1".to_string(), CausalEdgeType::Triggers, 0.95);
        graph.add_edge("p2".to_string(), "e2".to_string(), CausalEdgeType::Triggers, 0.80);

        // Counterfactual queries: "What if we use cargo upgrade vs manual edit?"
        let prob_upgrade = graph.counterfactual_probability("dependencies", "cargo upgrade");
        let prob_manual = graph.counterfactual_probability("dependencies", "manual edit");

        assert!(prob_upgrade > prob_manual);

        // Recommend the policy based on weights
        let recommendation = graph.recommend_policy("dependencies", &["cargo upgrade", "manual edit"]);
        assert_eq!(recommendation, Some("cargo upgrade".to_string()));
    }
}
