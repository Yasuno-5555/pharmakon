//! 🚀 Speculative Execution Engine — Phase 8
//!
//! Orchestrates the simultaneous execution of primary Plan A (real workspace)
//! and secondary Plan B (isolated sandbox or dry-run) to minimize LLM latency
//! and maximize execution velocity.

use crate::agent::Agent;
use crate::orchestration::world::{CandidatePlan, execute_node};
use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeculativeMode {
    /// Zero filesystem side-effects. Highly concurrent and token efficient.
    DryRun,
    /// Executes real tool mutations inside an isolated workspace-local directory,
    /// promoting B's modified files to production if Plan A fails.
    WorkspaceSandbox,
}

pub struct SpeculativeExecutor {
    pub mode: SpeculativeMode,
    pub sandbox_root: PathBuf,
}

impl SpeculativeExecutor {
    pub fn new(mode: SpeculativeMode, workspace_root: &Path) -> Self {
        Self {
            mode,
            sandbox_root: workspace_root.join(".pharmakon").join("sandbox"),
        }
    }

    /// Run Speculative Execution on Plan A and Plan B.
    /// If Plan A completes successfully, B's sandbox is clean-disposed.
    /// If Plan A fails, we promote Plan B's state to primary.
    pub async fn execute_speculative(
        &self,
        agent: &Agent,
        plan_a: CandidatePlan,
        plan_b: CandidatePlan,
    ) -> Result<String> {
        let session_id = agent.session_id.lock().await.clone();
        log::info!(
            "[SPECULATIVE] Dispatching speculative execution. Primary: {}, Secondary: {}",
            plan_a.description,
            plan_b.description
        );

        let workspace_root = std::env::current_dir().unwrap_or_default();

        // 1. Snapshot original workspace state (only for Sandbox mode)
        let original_snapshot = if matches!(self.mode, SpeculativeMode::WorkspaceSandbox) {
            Some(agent.snapshot_store.snapshot_dir(&workspace_root).await?)
        } else {
            None
        };

        // 2. Clone Agent for Plan B (speculative isolated execution)
        let spec_session_id = format!(
            "{}-spec-{}",
            session_id,
            &uuid::Uuid::new_v4().to_string()[..8]
        );
        let agent_b = agent.clone_for_speculative(
            matches!(self.mode, SpeculativeMode::DryRun),
            spec_session_id.clone(),
        );

        // Prepare Sandbox directory inside workspace
        let sandbox_dir = self.sandbox_root.join(format!(
            "speculative_run_{}",
            &uuid::Uuid::new_v4().to_string()[..8]
        ));
        if matches!(self.mode, SpeculativeMode::WorkspaceSandbox) {
            let _ = std::fs::create_dir_all(&sandbox_dir);
            // Restore original snapshot into the sandbox directory
            if let Some(ref snapshot) = original_snapshot {
                agent
                    .snapshot_store
                    .restore_dir(&sandbox_dir, snapshot)
                    .await?;
            }
        }

        // 3. Spawn Primary Plan Execution (A)
        let agent_a = agent.clone();
        let plan_a_clone = plan_a.clone();
        let workspace_root_clone = workspace_root.clone();
        let primary_handle = tokio::spawn(async move {
            let mut snapshotted_files = Vec::new();
            let ast = plan_a_clone.get_ast();
            let res = execute_node(
                &agent_a,
                &ast,
                &workspace_root_clone,
                &mut snapshotted_files,
            )
            .await;
            (res, snapshotted_files)
        });

        // 4. Spawn Secondary Plan Execution (B)
        let plan_b_clone = plan_b.clone();
        let sandbox_dir_clone = sandbox_dir.clone();
        let workspace_target = if matches!(self.mode, SpeculativeMode::WorkspaceSandbox) {
            sandbox_dir_clone.clone()
        } else {
            workspace_root.clone()
        };
        let secondary_handle = tokio::spawn(async move {
            let mut snapshotted_files = Vec::new();
            let ast = plan_b_clone.get_ast();
            let res = execute_node(&agent_b, &ast, &workspace_target, &mut snapshotted_files).await;
            (res, snapshotted_files)
        });

        // 5. Await Primary Plan (A)
        let (result_a, snaps_a) = match primary_handle.await {
            Ok(val) => val,
            Err(_) => (Err(anyhow!("Primary task thread panicked")), Vec::new()),
        };

        match result_a {
            Ok(res_a) => {
                log::info!(
                    "[SPECULATIVE] Primary Plan A completed successfully! Disposing Plan B."
                );
                // Cleanup sandbox
                if matches!(self.mode, SpeculativeMode::WorkspaceSandbox) {
                    let _ = std::fs::remove_dir_all(&sandbox_dir);
                }
                Ok(res_a)
            }
            Err(err_a) => {
                log::warn!(
                    "[SPECULATIVE] Primary Plan A failed ({}). Rolling back A's changes & promoting Plan B...",
                    err_a
                );

                // Rollback Plan A's dirty writes
                if matches!(self.mode, SpeculativeMode::WorkspaceSandbox) {
                    for (path, snap_id) in &snaps_a {
                        let _ = agent.snapshot_store.restore(snap_id, path).await;
                    }
                }

                // Await Secondary Plan (B)
                let (result_b, _snaps_b) = match secondary_handle.await {
                    Ok(val) => val,
                    Err(_) => (
                        Err(anyhow!("Secondary speculative thread panicked")),
                        Vec::new(),
                    ),
                };

                match result_b {
                    Ok(res_b) => {
                        log::info!(
                            "[SPECULATIVE] Plan B (Secondary) completed successfully! Promoting..."
                        );
                        if matches!(self.mode, SpeculativeMode::WorkspaceSandbox) {
                            // Collect sandbox changes and promote them directly to our live workspace root
                            let b_sandbox_snapshot =
                                agent.snapshot_store.snapshot_dir(&sandbox_dir).await?;
                            agent
                                .snapshot_store
                                .restore_dir(&workspace_root, &b_sandbox_snapshot)
                                .await?;
                            let _ = std::fs::remove_dir_all(&sandbox_dir);
                        }
                        Ok(format!("[SPECULATIVE PROMOTE B] {}", res_b))
                    }
                    Err(err_b) => {
                        // Cleanup sandbox on total failure
                        if matches!(self.mode, SpeculativeMode::WorkspaceSandbox) {
                            let _ = std::fs::remove_dir_all(&sandbox_dir);
                        }
                        Err(anyhow!(
                            "Speculative execution completely failed. Plan A error: {}. Plan B error: {}.",
                            err_a,
                            err_b
                        ))
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MockModel;
    use crate::orchestration::world::PlanNode;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_speculative_executor_dry_run() {
        let model = Arc::new(MockModel);
        let agent = Agent::new(model, "test-spec-session".to_string());

        let plan_a = CandidatePlan {
            id: "plan_a".to_string(),
            description: "Plan A".to_string(),
            estimated_tokens: 100,
            steps: Vec::new(),
            root: Some(PlanNode::Step {
                tool: "codeact".to_string(),
                args: serde_json::json!({ "script": "let x = 42; x" }),
                dry_run_first: false,
            }),
        };

        let plan_b = CandidatePlan {
            id: "plan_b".to_string(),
            description: "Plan B".to_string(),
            estimated_tokens: 100,
            steps: Vec::new(),
            root: Some(PlanNode::Step {
                tool: "codeact".to_string(),
                args: serde_json::json!({ "script": "let y = 100; y" }),
                dry_run_first: false,
            }),
        };

        let executor =
            SpeculativeExecutor::new(SpeculativeMode::DryRun, &std::env::current_dir().unwrap());
        let res = executor.execute_speculative(&agent, plan_a, plan_b).await;
        if let Err(ref e) = res {
            println!("Speculative run failed with error: {:?}", e);
        }
        assert!(res.is_ok(), "Res was Err: {:?}", res.err());
    }
}
