//! DSGE Integration Layer — connects cognitive_economics to the Agent loop.
//!
//! Wired injection points in agent.rs:
//!   [2] Entropy → economy.update_inflation()                 (line ~494)
//!   [3] Shadow price → economy.shadow_directive()            (line ~641)
//!   [4] Latency → economy.observe_latency()                  (line ~765)
//!   [5] Model routing → economy.select_model()               (line ~687)

use crate::orchestration::cognitive_economics::{
    CognitiveBudget, CognitiveMacroState, KnowledgeCapital,
    BellmanPlanner, ProductionFunction,
    model_market_quotes, select_model_by_roi, ModelMarketQuote,
};
use crate::model::AgentModel;
use std::collections::HashMap;
use std::sync::Arc;

/// Real-time model performance tracking per provider+model.
#[derive(Debug, Clone)]
pub struct ModelPerformanceTracker {
    pub entries: HashMap<String, ModelStats>,
}

#[derive(Debug, Clone, Default)]
pub struct ModelStats {
    pub calls: u64,
    pub successes: u64,
    pub total_latency_ms: u64,
    pub last_latency_ms: u64,
    pub rate_limits: u64,
    pub errors: u64,
}

impl ModelPerformanceTracker {
    pub fn new() -> Self { Self { entries: HashMap::new() } }

    pub fn record_success(&mut self, model_id: &str, latency_ms: u64) {
        let e = self.entries.entry(model_id.to_string()).or_default();
        e.calls += 1; e.successes += 1; e.total_latency_ms += latency_ms; e.last_latency_ms = latency_ms;
    }

    pub fn record_error(&mut self, model_id: &str, is_rate_limit: bool) {
        let e = self.entries.entry(model_id.to_string()).or_default();
        e.calls += 1; e.errors += 1;
        if is_rate_limit { e.rate_limits += 1; }
    }

    /// Live success rate (0.0–1.0) for a model.
    pub fn success_rate(&self, model_id: &str) -> f64 {
        self.entries.get(model_id).map(|s| if s.calls > 0 { s.successes as f64 / s.calls as f64 } else { 0.9 }).unwrap_or(0.9)
    }

    /// Live average latency in ms.
    pub fn avg_latency(&self, model_id: &str) -> u64 {
        self.entries.get(model_id).map(|s| if s.calls > 0 { s.total_latency_ms / s.calls } else { 500 }).unwrap_or(500)
    }

    /// Rate limit probability (0.0–1.0).
    pub fn rate_limit_prob(&self, model_id: &str) -> f64 {
        self.entries.get(model_id).map(|s| if s.calls > 0 { s.rate_limits as f64 / s.calls as f64 } else { 0.05 }).unwrap_or(0.05)
    }

    /// Live ROI: success_rate / (estimated_cost) — higher is better.
    /// Lower cost + higher success = better live ROI.
    pub fn live_roi(&self, model_id: &str, input_tokens: u64, output_tokens: u64) -> f64 {
        let sr = self.success_rate(model_id);
        let rl = self.rate_limit_prob(model_id);
        let cost = (input_tokens as f64 * 0.00002 + output_tokens as f64 * 0.00006) / 1000.0; // rough
        if cost <= 0.0 { return sr; }
        sr / cost * (1.0 - rl * 0.5)
    }
}


/// Model selection mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelMode {
    /// Economy routes to best model by live ROI.
    Auto,
    /// User explicitly picked this model.
    Manual(String),
}

impl Default for ModelMode {
    fn default() -> Self { ModelMode::Auto }
}

pub struct AgentEconomy {
    pub budget: CognitiveBudget,
    pub macro_state: CognitiveMacroState,
    pub knowledge: KnowledgeCapital,
    pub market_quotes: Vec<ModelMarketQuote>,
    pub model_perf: ModelPerformanceTracker,
    pub mode: ModelMode,
    pub bellman: BellmanPlanner,
    pub production: ProductionFunction,
}

impl AgentEconomy {
    pub fn new(complexity: f64) -> Self {
        Self {
            budget: CognitiveBudget::new(100_000, complexity),
            macro_state: CognitiveMacroState::new(),
            knowledge: KnowledgeCapital::new(),
            market_quotes: model_market_quotes(),
            model_perf: ModelPerformanceTracker::new(),
            mode: ModelMode::Auto,
            bellman: BellmanPlanner::new(0.95),
            production: ProductionFunction { alpha: 0.95, beta: 0.5, theta: complexity.max(0.1) },
        }
    }

    pub fn record_token_usage(&mut self, input_tokens: u64, output_tokens: u64) {
        let total = input_tokens + output_tokens;
        self.budget.spend(total);
        self.macro_state.token_reserves = self.budget.remaining() as f64;
        self.knowledge.invest_research(total as f64, 0.005);
    }

    pub fn update_inflation(&mut self, context_tokens: u64, optimal_context: u64) {
        self.macro_state.update_inflation(context_tokens, optimal_context);
        self.macro_state.average_entropy = self.macro_state.average_entropy * 0.9 + self.macro_state.context_inflation.max(0.0) * 0.1;
        self.macro_state.detect_crisis(0.1, self.budget.llm_gated);
    }

    pub fn observe_latency(&mut self, latency_ms: u64) {
        let liq = 1.0 - (latency_ms as f64 / 30000.0).min(1.0);
        self.macro_state.model_liquidity = self.macro_state.model_liquidity * 0.8 + liq * 0.2;
    }

    /// Record model call result for live performance tracking.
    pub fn record_model_result(&mut self, model_id: &str, latency_ms: u64, success: bool, is_rate_limit: bool) {
        if success { self.model_perf.record_success(model_id, latency_ms); }
        else { self.model_perf.record_error(model_id, is_rate_limit); }
    }

    pub fn shadow_directive(&self) -> String {
        let rem = self.budget.remaining();
        let liq = self.macro_state.model_liquidity;
        if liq < 0.3 {
            return "// ⚡ High API latency. NO deep reasoning. Use cached skills.\n".into();
        }
        if liq < 0.6 {
            let base = if rem > 2000 { String::new() }
                else if rem > 500 { format!("\n// ⚠ {} tokens left. Be concise.\n", rem) }
                else { format!("\n// 🚨 {} tokens left. MAX concise.\n", rem) };
            return format!("// 🌊 Moderate latency. Prefer cached.{}", base);
        }
        self.budget.shadow_directive()
    }

    /// Model selection: Manual mode returns user-chosen model, Auto mode
    /// uses live performance-weighted ROI routing across all available providers.
    pub fn select_model(&self, task: &str) -> Option<Arc<dyn AgentModel>> {
        match &self.mode {
            ModelMode::Manual(model_id) => {
                crate::providers::registry::ModelRegistry::get_model(model_id)
            }
            ModelMode::Auto => {
                let est_input = 2000u64;
                let est_output = (self.budget.complexity * 500.0) as u64;

                // Try live performance first, then static quotes
                let available = crate::providers::registry::ModelRegistry::list_available_models();
                if available.len() <= 1 {
                    return available.first()
                        .and_then(|id| crate::providers::registry::ModelRegistry::get_model(id));
                }

                // Score all available models by live ROI + static ROI
                let mut scored: Vec<(String, f64)> = available.iter().map(|id| {
                    let live = self.model_perf.live_roi(id, est_input, est_output);
                    let quote = self.market_quotes.iter()
                        .find(|q| q.model_id == *id)
                        .map(|q| q.expected_roi(est_input, est_output))
                        .unwrap_or(0.0);
                    let combined = live * 0.7 + quote * 0.3;
                    (id.clone(), combined)
                }).collect();

                scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

                log::info!(
                    "Auto model routing: {}",
                    scored.iter().map(|(id, s)| format!("{}={:.2}", id, s)).collect::<Vec<_>>().join(", ")
                );

                scored.first()
                    .and_then(|(id, _)| crate::providers::registry::ModelRegistry::get_model(id))
            }
        }
    }

    /// Switch to manual mode with a specific model.
    pub fn set_manual(&mut self, model_id: &str) {
        self.mode = ModelMode::Manual(model_id.to_string());
    }

    /// Switch to auto mode.
    pub fn set_auto(&mut self) { self.mode = ModelMode::Auto; }

    pub fn compute_optimal_budget(&mut self, complexity: f64) -> u64 {
        self.production.theta = complexity.max(0.1);
        let budget = self.budget.remaining();
        let optimal = self.bellman.bellman_iteration(budget, complexity, &self.production);
        (budget as f64 * (1.0 - 1.0 / (1.0 + optimal))).min(budget as f64) as u64
    }
}
