//! Swarm Economy — Multi-agent DSGE resource allocation.
//!
//! When Fractal Swarm spawns sub-agents, each one competes for a slice
//! of the parent's token budget. GeneralEquilibrium clears the market.
//!
//! Flow:
//!   Parent has budget B and N sub-tasks
//!   → Each sub-agent gets AgentProfile with specialization
//!   → GeneralEquilibrium.market_clearing() computes optimal allocations
//!   → Sub-agents execute with budget caps
//!   → Results pooled, parent economy updated

use crate::orchestration::cognitive_economics::{
    AgentProfile, AgentSpecialization, GeneralEquilibrium, ModelMarketQuote,
    model_market_quotes, select_model_by_roi,
};
use crate::orchestration::dsge_integration::AgentEconomy;
use std::collections::HashMap;

/// Manages token allocation across swarm sub-agents.
pub struct SwarmEconomy {
    pub equilibrium: GeneralEquilibrium,
    pub profiles: Vec<AgentProfile>,
    pub allocations: HashMap<String, u64>, // sub_agent_id → token_budget
    pub market_quotes: Vec<ModelMarketQuote>,
    pub parent_remaining: u64,
}

impl SwarmEconomy {
    /// Create from parent economy state.
    pub fn from_parent(economy: &AgentEconomy) -> Self {
        let parent_remaining = economy.budget.remaining();
        let market_quotes = economy.market_quotes.clone();
        Self {
            equilibrium: GeneralEquilibrium::new(parent_remaining),
            profiles: Vec::new(),
            allocations: HashMap::new(),
            market_quotes,
            parent_remaining,
        }
    }

    /// Register a sub-task as an economic agent.
    pub fn register_task(&mut self, task_id: &str, description: &str, specialization: AgentSpecialization) {
        let mut profile = AgentProfile::new(task_id, specialization);
        // Adjust token share based on task description complexity
        let complexity_weight = if description.len() > 200 { 1.5 } else { 1.0 };
        profile.allocated_token_share *= complexity_weight;
        self.profiles.push(profile);
    }

    /// Run market clearing: each agent's token allocation is computed
    /// by GeneralEquilibrium based on specialization and competition.
    pub fn allocate_budgets(&mut self) -> HashMap<String, u64> {
        self.equilibrium.agents = self.profiles.clone();
        let shadow_price = self.equilibrium.clear(20);

        self.allocations.clear();
        for agent in &self.equilibrium.agents {
            let budget = agent.token_allocation(self.parent_remaining);
            self.allocations.insert(agent.id.clone(), budget);
        }

        log::info!(
            "SwarmEconomy: Market cleared at λ={:.6}. Allocated {} sub-agents, total {} tokens.",
            shadow_price,
            self.allocations.len(),
            self.allocations.values().sum::<u64>()
        );

        self.allocations.clone()
    }

    /// Select the best model for a sub-agent based on task + specialization + budget.
    pub fn select_model_for(
        &self,
        task: &str,
        specialization: &AgentSpecialization,
        budget: u64,
    ) -> Option<String> {
        let est_input = 1500;
        let est_output = budget.min(2000) / 2;

        let best = select_model_by_roi(&self.market_quotes, est_input, est_output, None);

        best.map(|q| {
            // Specialization-aware routing:
            // Deep reasoning → prefer high-capability models
            // Fast execution → prefer low-latency models
            match specialization {
                AgentSpecialization::Deep | AgentSpecialization::Planner => {
                    if q.avg_success_rate > 0.9 { q.model_id.clone() }
                    else { "gemini/gemini-2.5-flash".into() }
                }
                AgentSpecialization::Fast | AgentSpecialization::Verifier => {
                    if q.avg_latency_ms < 500 { q.model_id.clone() }
                    else { "deepseek/deepseek-chat".into() }
                }
                _ => q.model_id.clone(),
            }
        })
    }

    /// After all sub-agents complete, merge results back into parent economy.
    pub fn merge_results(
        &self,
        results: &[(String, Result<String, anyhow::Error>)],
    ) -> String {
        let mut summary = String::new();
        let mut total_spent = 0u64;
        let mut success_count = 0usize;

        for (task_id, result) in results {
            let budget = self.allocations.get(task_id).copied().unwrap_or(0);
            total_spent += budget;

            match result {
                Ok(output) => {
                    success_count += 1;
                    let truncated = if output.len() > 200 {
                        format!("{}...", &output[..197])
                    } else {
                        output.clone()
                    };
                    summary.push_str(&format!(
                        "## {}\nBudget: {} tokens | ROI: {:.2}\n{}\n\n",
                        task_id,
                        budget,
                        if budget > 0 { output.len() as f64 / budget as f64 } else { 0.0 },
                        truncated,
                    ));
                }
                Err(e) => {
                    summary.push_str(&format!(
                        "## {} (FAILED)\nBudget wasted: {} tokens\nError: {}\n\n",
                        task_id, budget, e,
                    ));
                }
            }
        }

        summary.push_str(&format!(
            "---\nSwarm Summary: {}/{} succeeded, {} total tokens spent, {:.2} tokens/result\n",
            success_count,
            results.len(),
            total_spent,
            if !results.is_empty() { total_spent as f64 / results.len() as f64 } else { 0.0 },
        ));

        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_swarm_economy_allocation() {
        let economy = AgentEconomy::new(0.5);
        let mut swarm = SwarmEconomy::from_parent(&economy);

        swarm.register_task("task-1", "grep and analyze all Rust files", AgentSpecialization::Researcher);
        swarm.register_task("task-2", "apply patches to fix compilation", AgentSpecialization::Deep);
        swarm.register_task("task-3", "quick syntax verification", AgentSpecialization::Fast);

        let budgets = swarm.allocate_budgets();
        assert_eq!(budgets.len(), 3);
        // Total allocated should be within parent budget
        let total: u64 = budgets.values().sum();
        assert!(
            total <= economy.budget.total_budget,
            "Total {} exceeds parent budget {}",
            total,
            economy.budget.total_budget
        );
    }

    #[test]
    fn test_model_selection_by_specialization() {
        let economy = AgentEconomy::new(0.5);
        let swarm = SwarmEconomy::from_parent(&economy);

        let deep_model = swarm.select_model_for("complex analysis", &AgentSpecialization::Deep, 5000);
        assert!(deep_model.is_some());

        let fast_model = swarm.select_model_for("quick check", &AgentSpecialization::Fast, 500);
        assert!(fast_model.is_some());
    }
}
