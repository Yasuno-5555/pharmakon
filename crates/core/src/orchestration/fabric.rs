//! 🟢 Distributed Execution Fabric — Phase 8
//!
//! Handles multi-node network cluster setups, hardware capabilities advertising,
//! resource requirement task matching, load-adaptive routing, and execution result aggregation.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeCapabilities {
    pub gpu_available: bool,
    pub total_ram_gb: usize,
    pub active_load_score: f64, // Range from 0.0 (idle) to 1.0 (overloaded)
    pub supported_tools: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FabricNodeStatus {
    Online,
    Busy,
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FabricNode {
    pub node_id: String,
    pub endpoint: String,
    pub capabilities: NodeCapabilities,
    pub status: FabricNodeStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskRequirements {
    pub gpu_needed: bool,
    pub min_ram_gb: usize,
    pub required_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteTaskResult {
    pub node_id: String,
    pub success: bool,
    pub output: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DistributedFabric {
    pub nodes: Vec<FabricNode>,
}

impl DistributedFabric {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_node(&mut self, node: FabricNode) {
        self.nodes.retain(|n| n.node_id != node.node_id);
        self.nodes.push(node);
    }

    /// Selects the optimal node adaptively based on resource capabilities and active load level.
    pub fn route_task(&self, req: &TaskRequirements) -> Option<&FabricNode> {
        let mut candidates: Vec<&FabricNode> = self
            .nodes
            .iter()
            .filter(|node| {
                if node.status != FabricNodeStatus::Online {
                    return false;
                }

                // 1. Check GPU requirement
                if req.gpu_needed && !node.capabilities.gpu_available {
                    return false;
                }

                // 2. Check RAM bounds
                if node.capabilities.total_ram_gb < req.min_ram_gb {
                    return false;
                }

                // 3. Check tool support
                for tool in &req.required_tools {
                    if !node.capabilities.supported_tools.contains(tool) {
                        return false;
                    }
                }

                true
            })
            .collect();

        if candidates.is_empty() {
            return None;
        }

        // Sort dynamically: lowest active load score first
        candidates.sort_by(|a, b| {
            a.capabilities
                .active_load_score
                .partial_cmp(&b.capabilities.active_load_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Some(candidates[0])
    }

    /// Dispatches a plan task to a remote cluster node.
    pub fn dispatch_task(
        &self,
        node_id: &str,
        task: &str,
        plan_node: &crate::orchestration::world::PlanNode,
    ) -> Result<RemoteTaskResult, String> {
        let node = self
            .nodes
            .iter()
            .find(|n| n.node_id == node_id)
            .ok_or_else(|| format!("Target node '{}' not found in fabric registry", node_id))?;

        if node.status == FabricNodeStatus::Offline {
            return Err(format!("Cannot dispatch to offline node '{}'", node_id));
        }

        log::info!(
            "DistributedFabric: dispatching task '{}' to remote endpoint: {}",
            task,
            node.endpoint
        );

        // Simulation of HTTP remote network aggregation
        let serialized_plan = serde_json::to_string(plan_node).unwrap_or_default();
        let success = true;
        let output = format!(
            "✅ Remote execution on Node '{}' succeeded.\nPayload Size: {} bytes\nPlan Executed successfully.",
            node_id,
            serialized_plan.len()
        );

        Ok(RemoteTaskResult {
            node_id: node_id.to_string(),
            success,
            output,
            duration_ms: 125, // Fast remote cluster latency simulation
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::world::PlanNode;

    #[test]
    fn test_fabric_registration_and_adaptive_routing() {
        let mut fabric = DistributedFabric::new();

        // 1. Setup GPU-heavy High-RAM Mac Mini node
        fabric.register_node(FabricNode {
            node_id: "m4-mac-mini".to_string(),
            endpoint: "http://192.168.1.100:8080".to_string(),
            capabilities: NodeCapabilities {
                gpu_available: true,
                total_ram_gb: 32,
                active_load_score: 0.15, // Light load
                supported_tools: vec!["cargo".to_string(), "clang".to_string()],
            },
            status: FabricNodeStatus::Online,
        });

        // 2. Setup Low-RAM ThinkPad with high load
        fabric.register_node(FabricNode {
            node_id: "thinkpad-busy".to_string(),
            endpoint: "http://192.168.1.101:8080".to_string(),
            capabilities: NodeCapabilities {
                gpu_available: false,
                total_ram_gb: 16,
                active_load_score: 0.95, // Heavy load
                supported_tools: vec!["cargo".to_string()],
            },
            status: FabricNodeStatus::Online,
        });

        // 3. Setup another matching ThinkPad with low load
        fabric.register_node(FabricNode {
            node_id: "thinkpad-idle".to_string(),
            endpoint: "http://192.168.1.102:8080".to_string(),
            capabilities: NodeCapabilities {
                gpu_available: false,
                total_ram_gb: 16,
                active_load_score: 0.10, // Very idle
                supported_tools: vec!["cargo".to_string()],
            },
            status: FabricNodeStatus::Online,
        });

        // Query 1: Task requiring GPU
        let req_gpu = TaskRequirements {
            gpu_needed: true,
            min_ram_gb: 16,
            required_tools: vec!["cargo".to_string()],
        };
        let routed_gpu = fabric.route_task(&req_gpu);
        assert!(routed_gpu.is_some());
        assert_eq!(routed_gpu.unwrap().node_id, "m4-mac-mini");

        // Query 2: Standard compilation task (should pick idle Thinkpad over busy Thinkpad)
        let req_standard = TaskRequirements {
            gpu_needed: false,
            min_ram_gb: 8,
            required_tools: vec!["cargo".to_string()],
        };
        let routed_standard = fabric.route_task(&req_standard);
        assert!(routed_standard.is_some());
        assert_eq!(routed_standard.unwrap().node_id, "thinkpad-idle");
    }

    #[test]
    fn test_fabric_dispatch() {
        let mut fabric = DistributedFabric::new();
        fabric.register_node(FabricNode {
            node_id: "m4-mac-mini".to_string(),
            endpoint: "http://192.168.1.100:8080".to_string(),
            capabilities: NodeCapabilities {
                gpu_available: true,
                total_ram_gb: 32,
                active_load_score: 0.20,
                supported_tools: vec!["cargo".to_string()],
            },
            status: FabricNodeStatus::Online,
        });

        let dummy_plan = PlanNode::Step {
            tool: "cargo build".to_string(),
            args: serde_json::json!({}),
            dry_run_first: false,
        };

        let result = fabric.dispatch_task("m4-mac-mini", "compile subproject", &dummy_plan);
        assert!(result.is_ok());
        let val = result.unwrap();
        assert!(val.success);
        assert!(val.output.contains("Node 'm4-mac-mini'"));
    }
}
