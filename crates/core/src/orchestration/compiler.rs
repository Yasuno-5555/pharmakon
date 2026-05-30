use crate::orchestration::world::PlanNode;
use std::collections::HashSet;

pub struct PlanCompiler;

impl Default for PlanCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl PlanCompiler {
    pub fn new() -> Self {
        Self
    }

    /// Compile and optimize the plan AST node using multiple optimization passes.
    pub fn compile(&self, node: PlanNode) -> PlanNode {
        let node = self.optimize_dead_steps(node);
        let node = self.fuse_steps(node);
        let node = self.discover_parallelism(node);

        self.place_verifications(node)
    }

    /// Pass 1: Dead Step Elimination (Dead Store Elimination)
    /// If there are consecutive file writes to the same path without intermediate reads,
    /// prune the earlier redundant write steps to optimize disk I/O and token costs.
    fn optimize_dead_steps(&self, node: PlanNode) -> PlanNode {
        match node {
            PlanNode::Sequence { nodes } => {
                let mut optimized_nodes = Vec::new();
                let mut writes_seen = std::collections::HashMap::new(); // path -> index in optimized_nodes

                for child in nodes {
                    let optimized_child = self.optimize_dead_steps(child);

                    if let PlanNode::Step {
                        ref tool, ref args, ..
                    } = optimized_child
                    {
                        if tool == "write_file" {
                            if let Some(path) = args.get("path").and_then(|p| p.as_str()) {
                                // If we've already written to this path in this sequence without an intermediate read/use,
                                // we can prune the previous write!
                                if let Some(&prev_idx) = writes_seen.get(path) {
                                    // Mark the previous node as dummy / pruned
                                    optimized_nodes[prev_idx] = PlanNode::Step {
                                        tool: "nop".to_string(),
                                        args: serde_json::Value::Null,
                                        dry_run_first: false,
                                    };
                                }
                                writes_seen.insert(path.to_string(), optimized_nodes.len());
                            }
                        } else if tool == "read_file"
                            || tool == "grep"
                            || tool == "shell"
                            || tool == "codeact"
                        {
                            // Any read or dynamic command clears our dead-store tracker for accessed paths
                            // (since they might read files written so far)
                            writes_seen.clear();
                        }
                    } else {
                        // Complex blocks break basic-block sequence optimization context
                        writes_seen.clear();
                    }
                    optimized_nodes.push(optimized_child);
                }

                // Filter out nop nodes
                let filtered_nodes: Vec<PlanNode> = optimized_nodes
                    .into_iter()
                    .filter(|n| {
                        if let PlanNode::Step { tool, .. } = n {
                            tool != "nop"
                        } else {
                            true
                        }
                    })
                    .collect();

                if filtered_nodes.len() == 1 {
                    filtered_nodes[0].clone()
                } else {
                    PlanNode::Sequence {
                        nodes: filtered_nodes,
                    }
                }
            }
            PlanNode::Parallel { nodes } => {
                let optimized_nodes = nodes
                    .into_iter()
                    .map(|n| self.optimize_dead_steps(n))
                    .collect();
                PlanNode::Parallel {
                    nodes: optimized_nodes,
                }
            }
            PlanNode::Conditional {
                condition,
                then_branch,
                else_branch,
            } => PlanNode::Conditional {
                condition,
                then_branch: Box::new(self.optimize_dead_steps(*then_branch)),
                else_branch: else_branch.map(|e| Box::new(self.optimize_dead_steps(*e))),
            },
            PlanNode::Retry { node, max_attempts } => PlanNode::Retry {
                node: Box::new(self.optimize_dead_steps(*node)),
                max_attempts,
            },
            PlanNode::Verify {
                node,
                assertion_script,
            } => PlanNode::Verify {
                node: Box::new(self.optimize_dead_steps(*node)),
                assertion_script,
            },
            PlanNode::Gate { gate_name, node } => PlanNode::Gate {
                gate_name,
                node: Box::new(self.optimize_dead_steps(*node)),
            },
            _ => node,
        }
    }

    /// Pass 2: Step Fusion
    /// Fuses sequential actions such as reading a file and then grepping it, or sequential scripts,
    /// into a single composite step to reduce planning overhead and roundtrips.
    fn fuse_steps(&self, node: PlanNode) -> PlanNode {
        match node {
            PlanNode::Sequence { nodes } => {
                let mut fused_nodes = Vec::new();
                let mut iter = nodes.into_iter().peekable();

                while let Some(current) = iter.next() {
                    let optimized_current = self.fuse_steps(current);

                    if let Some(next) = iter.peek()
                        && let (
                            PlanNode::Step {
                                tool: t1, args: a1, ..
                            },
                            PlanNode::Step {
                                tool: t2, args: a2, ..
                            },
                        ) = (&optimized_current, next)
                    {
                        // Rule: consecutive read_file and grep on the same file -> fuse to grep directly
                        if t1 == "read_file"
                            && t2 == "grep"
                            && let (Some(p1), Some(p2)) = (a1.get("path"), a2.get("path"))
                            && p1 == p2
                        {
                            // Consume peeked
                            let next_node = iter.next().unwrap();
                            fused_nodes.push(self.fuse_steps(next_node));
                            continue;
                        }
                    }
                    fused_nodes.push(optimized_current);
                }

                if fused_nodes.len() == 1 {
                    fused_nodes[0].clone()
                } else {
                    PlanNode::Sequence { nodes: fused_nodes }
                }
            }
            PlanNode::Parallel { nodes } => {
                let optimized_nodes = nodes.into_iter().map(|n| self.fuse_steps(n)).collect();
                PlanNode::Parallel {
                    nodes: optimized_nodes,
                }
            }
            PlanNode::Conditional {
                condition,
                then_branch,
                else_branch,
            } => PlanNode::Conditional {
                condition,
                then_branch: Box::new(self.fuse_steps(*then_branch)),
                else_branch: else_branch.map(|e| Box::new(self.fuse_steps(*e))),
            },
            PlanNode::Retry { node, max_attempts } => PlanNode::Retry {
                node: Box::new(self.fuse_steps(*node)),
                max_attempts,
            },
            PlanNode::Verify {
                node,
                assertion_script,
            } => PlanNode::Verify {
                node: Box::new(self.fuse_steps(*node)),
                assertion_script,
            },
            PlanNode::Gate { gate_name, node } => PlanNode::Gate {
                gate_name,
                node: Box::new(self.fuse_steps(*node)),
            },
            _ => node,
        }
    }

    /// Pass 3: Parallel Discovery (Concurrency Grouping)
    /// Scans sequences of independent steps (operating on disjoint file scopes) and groups them
    /// into parallelized nodes to allow async parallel tool execution.
    fn discover_parallelism(&self, node: PlanNode) -> PlanNode {
        match node {
            PlanNode::Sequence { nodes } => {
                let mut optimized_nodes = Vec::new();
                let mut current_parallel_group: Vec<PlanNode> = Vec::new();
                let mut current_paths_locked = HashSet::new();

                for child in nodes {
                    let optimized_child = self.discover_parallelism(child);

                    if let PlanNode::Step {
                        ref tool, ref args, ..
                    } = optimized_child
                    {
                        // Extract target file path if applicable
                        let target_path = args
                            .get("path")
                            .and_then(|p| p.as_str())
                            .map(|s| s.to_string());

                        if let Some(path) = target_path {
                            if current_paths_locked.contains(&path) {
                                // Dependency collision! Flush existing parallel group to sequence
                                if !current_parallel_group.is_empty() {
                                    if current_parallel_group.len() == 1 {
                                        optimized_nodes.push(current_parallel_group[0].clone());
                                    } else {
                                        optimized_nodes.push(PlanNode::Parallel {
                                            nodes: current_parallel_group.clone(),
                                        });
                                    }
                                    current_parallel_group.clear();
                                    current_paths_locked.clear();
                                }
                            }
                            current_paths_locked.insert(path);
                            current_parallel_group.push(PlanNode::Step {
                                tool: tool.clone(),
                                args: args.clone(),
                                dry_run_first: false,
                            });
                        } else {
                            // Dynamic steps (shell, codeact) lock everything. Flush existing parallel group
                            if !current_parallel_group.is_empty() {
                                if current_parallel_group.len() == 1 {
                                    optimized_nodes.push(current_parallel_group[0].clone());
                                } else {
                                    optimized_nodes.push(PlanNode::Parallel {
                                        nodes: current_parallel_group.clone(),
                                    });
                                }
                                current_parallel_group.clear();
                                current_paths_locked.clear();
                            }
                            optimized_nodes.push(optimized_child);
                        }
                    } else {
                        // Complex constructs flush block sequence
                        if !current_parallel_group.is_empty() {
                            if current_parallel_group.len() == 1 {
                                optimized_nodes.push(current_parallel_group[0].clone());
                            } else {
                                optimized_nodes.push(PlanNode::Parallel {
                                    nodes: current_parallel_group.clone(),
                                });
                            }
                            current_parallel_group.clear();
                            current_paths_locked.clear();
                        }
                        optimized_nodes.push(optimized_child);
                    }
                }

                // Flush final parallel group
                if !current_parallel_group.is_empty() {
                    if current_parallel_group.len() == 1 {
                        optimized_nodes.push(current_parallel_group[0].clone());
                    } else {
                        optimized_nodes.push(PlanNode::Parallel {
                            nodes: current_parallel_group,
                        });
                    }
                }

                if optimized_nodes.len() == 1 {
                    optimized_nodes[0].clone()
                } else {
                    PlanNode::Sequence {
                        nodes: optimized_nodes,
                    }
                }
            }
            PlanNode::Parallel { nodes } => {
                let optimized_nodes = nodes
                    .into_iter()
                    .map(|n| self.discover_parallelism(n))
                    .collect();
                PlanNode::Parallel {
                    nodes: optimized_nodes,
                }
            }
            _ => node,
        }
    }

    /// Pass 4: Verify Placement (Strategic Self-Healing Safety Guards)
    /// Automatically inserts logical checks (compilation and tests) after file updates,
    /// protecting against compile breaks or invalid syntax.
    fn place_verifications(&self, node: PlanNode) -> PlanNode {
        match node {
            PlanNode::Sequence { nodes } => {
                let mut optimized_nodes = Vec::new();

                for child in nodes {
                    let optimized_child = self.place_verifications(child);
                    optimized_nodes.push(optimized_child.clone());

                    if let PlanNode::Step {
                        ref tool, ref args, ..
                    } = optimized_child
                        && (tool == "apply_patch" || tool == "write_file")
                    {
                        let path = args
                            .get("path")
                            .and_then(|p| p.as_str())
                            .unwrap_or_default();
                        // If we update Cargo.toml or critical code files, immediately compile check!
                        if path.contains("Cargo.toml") || path.contains(".rs") {
                            optimized_nodes.push(PlanNode::Verify {
                                node: Box::new(PlanNode::Step {
                                    tool: "shell".to_string(),
                                    args: serde_json::json!({ "command": "cargo check" }),
                                    dry_run_first: false,
                                }),
                                assertion_script: "cargo_success".to_string(),
                            });
                        }
                    }
                }

                if optimized_nodes.len() == 1 {
                    optimized_nodes[0].clone()
                } else {
                    PlanNode::Sequence {
                        nodes: optimized_nodes,
                    }
                }
            }
            PlanNode::Parallel { nodes } => {
                let optimized_nodes = nodes
                    .into_iter()
                    .map(|n| self.place_verifications(n))
                    .collect();
                PlanNode::Parallel {
                    nodes: optimized_nodes,
                }
            }
            _ => node,
        }
    }

    /// Helper to statically estimate token cost of executing the CompiledPlan AST node
    pub fn estimate_token_cost(&self, node: &PlanNode) -> u64 {
        match node {
            PlanNode::Script { .. } => 800,
            PlanNode::Step { tool, .. } => match tool.as_str() {
                "codeact" => 800,
                "shell" => 400,
                "write_file" => 300,
                _ => 150,
            },
            PlanNode::Sequence { nodes } | PlanNode::Parallel { nodes } => {
                nodes.iter().map(|n| self.estimate_token_cost(n)).sum()
            }
            PlanNode::Conditional {
                then_branch,
                else_branch,
                ..
            } => {
                let mut sum = self.estimate_token_cost(then_branch);
                if let Some(else_b) = else_branch {
                    sum += self.estimate_token_cost(else_b);
                }
                sum
            }
            PlanNode::Retry { node, max_attempts } => {
                self.estimate_token_cost(node) * (*max_attempts as u64)
            }
            PlanNode::Verify { node, .. } => self.estimate_token_cost(node) + 200,
            PlanNode::Gate { node, .. } => self.estimate_token_cost(node),
        }
    }
}
