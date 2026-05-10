//! DSGE Integration Layer — connects cognitive_economics to the Agent loop.
//!
//! Wired injection points in agent.rs:
//!   [2] Entropy → economy.update_inflation()                 (line ~494)
//!   [3] Shadow price → economy.shadow_directive()            (line ~641)
//!   [4] Latency → economy.observe_latency()                  (line ~765)
//!   [5] Model routing → economy.select_model()               (line ~687)

use crate::orchestration::cognitive_economics::{
    CognitiveBudget, CognitiveMacroState, KnowledgeCapital,
    BellmanPlanner, ProductionFunction, RegimeSwitcher,
    model_market_quotes, ModelMarketQuote,
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
    pub latency_ema_ms: f64,
    pub rate_limits: u64,
    pub errors: u64,
}

impl ModelPerformanceTracker {
    pub fn new() -> Self { Self { entries: HashMap::new() } }

    pub fn record_success(&mut self, model_id: &str, latency_ms: u64) {
        let e = self.entries.entry(model_id.to_string()).or_default();
        e.calls += 1;
        e.successes += 1;
        e.total_latency_ms += latency_ms;
        e.last_latency_ms = latency_ms;
        if e.calls == 1 {
            e.latency_ema_ms = latency_ms as f64;
        } else {
            e.latency_ema_ms = 0.7 * e.latency_ema_ms + 0.3 * (latency_ms as f64);
        }
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

    /// Live average latency in ms (EMA based).
    pub fn avg_latency(&self, model_id: &str) -> u64 {
        self.entries.get(model_id).map(|s| if s.calls > 0 { s.latency_ema_ms.round() as u64 } else { 500 }).unwrap_or(500)
    }

    /// Rate limit probability (0.0–1.0).
    pub fn rate_limit_prob(&self, model_id: &str) -> f64 {
        self.entries.get(model_id).map(|s| if s.calls > 0 { s.rate_limits as f64 / s.calls as f64 } else { 0.05 }).unwrap_or(0.05)
    }

    /// Live ROI: success_rate / (estimated_cost) — higher is better.
    /// Lower cost + higher success = better live ROI.
    /// Discounted by latency EMA to prefer faster models when ROI is close.
    pub fn live_roi(&self, model_id: &str, input_tokens: u64, output_tokens: u64) -> f64 {
        let sr = self.success_rate(model_id);
        let rl = self.rate_limit_prob(model_id);
        let lat = self.avg_latency(model_id) as f64;
        let cost = (input_tokens as f64 * 0.00002 + output_tokens as f64 * 0.00006) / 1000.0; // rough
        if cost <= 0.0 { return sr; }
        let latency_discount = 1.0 / (1.0 + (lat / 5000.0));
        (sr / cost * (1.0 - rl * 0.5)) * latency_discount
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

/// Per-API-call observation for online production function fitting.
#[derive(Debug, Clone)]
pub struct CallObservation {
    pub tokens_spent: u64,
    pub latency_ms: u64,
    pub success: bool,
    pub model_id: String,
    /// Proxy for output quality: 1.0 = task completed, 0.5 = partial, 0.0 = failure
    pub quality_proxy: f64,
}

/// Trajectory-level telemetry for offline DSGE parameter estimation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrajectoryTelemetry {
    pub task_id: String,
    pub model_id: String,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_latency_ms: u64,
    pub retry_count: u64,
    pub tool_call_count: u64,
    pub success: bool,
    pub human_correction_needed: bool,
    pub downstream_reuse: bool,
    pub failure_category: Option<String>,
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
    /// Tracks macro regime (Normal/Congestion/Crisis/Offline) for policy decisions.
    pub regime: RegimeSwitcher,
    /// Rolling observation buffer for online production function fitting.
    pub observations: Vec<CallObservation>,
    /// Accumulated trajectory telemetry for the current task.
    pub current_telemetry: Option<TrajectoryTelemetry>,
}

impl AgentEconomy {
    pub fn new(complexity: f64) -> Self {
        Self {
            budget: CognitiveBudget::new(10_000_000, complexity),
            macro_state: CognitiveMacroState::new(),
            knowledge: KnowledgeCapital::new(),
            market_quotes: model_market_quotes(),
            model_perf: ModelPerformanceTracker::new(),
            mode: ModelMode::Auto,
            bellman: BellmanPlanner::new(0.95),
            production: ProductionFunction { alpha: 0.95, beta: 0.5, theta: complexity.max(0.1) },
            regime: RegimeSwitcher::new(),
            observations: Vec::with_capacity(64),
            current_telemetry: None,
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
    pub fn select_model(&self, task: &str, complexity: f64) -> Option<Arc<dyn AgentModel>> {
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

                // Complexity-aware: Deep→high-output, Simple→cheap
let complexity_bonus = |id: &str| -> f64 {
    if complexity > 0.7 {
        if id.contains("deepseek-v4")||id.contains("deepseek-chat") { 0.3 }
        else if id.contains("deepseek-reasoner")||id.contains("pro") { 0.2 }
        else if id.contains("gemini") { 0.1 }
        else { -0.1 }
    } else if complexity < 0.3 {
        if id.contains("groq")||id.contains("llama") { 0.2 }
        else if id.contains("flash") { 0.1 }
        else { 0.0 }
    } else { 0.0 }
};

// Score all available models by live ROI + static ROI
                let mut scored: Vec<(String, f64)> = available.iter().map(|id| {
                    let live = self.model_perf.live_roi(id, est_input, est_output);
                    let quote = self.market_quotes.iter()
                        .find(|q| q.model_id == *id)
                        .map(|q| q.expected_roi(est_input, est_output))
                        .unwrap_or(0.0);
                    let combined = live * 0.7 + quote * 0.3 + complexity_bonus(id);
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

    /// Recommend a dynamic `max_tokens` for the next LLM call.
    /// Derived from: optimal ceiling × regime policy × remaining budget × learned quality.
    /// Returns a u32 in [256, 8192] suitable for `CompletionRequest.max_tokens`.
    pub fn recommend_max_tokens(&mut self, model_id: &str) -> u32 { let _ = model_id;
        // Update regime state from current macro conditions
        let rate_limit_prob = self.model_perf.rate_limit_prob(
            &self.market_quotes.first().map(|q| q.model_id.as_str()).unwrap_or("unknown")
        );
        self.regime.update(&self.macro_state, rate_limit_prob, self.budget.llm_gated);

        let policy = self.regime.policy();
        let regime_cap = policy.max_tokens;

        // Scale optimal ceiling by learned quality: lower α → fewer tokens are effective
        let quality_scale = self.production.alpha.clamp(0.3, 1.0);
        let ceiling = (self.budget.optimal_ceiling as f64 * quality_scale) as u32;

        // Don't spend more than 25% of remaining in one call, but ensure a floor of at least 8192 to prevent early truncation
        let budget_cap = ((self.budget.remaining() / 4) as u32).max(8192);

        // Floor: never go below 256 to allow tool calls
        let model_cap = if model_id.contains("deepseek") { 16384 } else if model_id.contains("gemini") { 8192 } else { 4096 }; let recommended = ceiling.min(regime_cap).min(budget_cap).min(model_cap).max(256);

        log::info!(
            "Economy: recommend max_tokens={} (ceiling={}, regime={}, budget_cap={}, α={:.3})",
            recommended, ceiling, regime_cap, budget_cap, self.production.alpha
        );
        recommended
    }

    // ── Observation → Estimation → Feedback Loop ──

    /// Record a per-call observation for online production function fitting.
    pub fn record_observation(&mut self, obs: CallObservation) {
        if self.observations.len() >= 64 {
            self.observations.remove(0);
        }
        self.observations.push(obs);
    }

    /// Online update of production function α, β from observed data.
    /// Uses EMA-updated success rate as α (asymptotic quality ceiling)
    /// and EMA-updated quality-per-token as β (rate of quality growth).
    pub fn update_production_from_observations(&mut self) {
        if self.observations.is_empty() { return; }

        let n = self.observations.len() as f64;
        let weighted_success: f64 = self.observations.iter()
            .map(|o| o.quality_proxy)
            .sum::<f64>() / n;

        // Quality per token: higher = faster growth (β)
        let quality_per_token: f64 = self.observations.iter()
            .filter(|o| o.tokens_spent > 0)
            .map(|o| o.quality_proxy / o.tokens_spent.max(1) as f64)
            .sum::<f64>() / n.max(1.0);

        // EMA update with learning rate 0.15
        let lr = 0.15;
        self.production.alpha = (1.0 - lr) * self.production.alpha + lr * weighted_success;
        // β bounded: [0.1, 2.0] to prevent divergence from bad observations
        let new_beta = (1.0 - lr) * self.production.beta + lr * (quality_per_token * 500.0).min(2.0);
        self.production.beta = new_beta.clamp(0.1, 2.0);

        log::info!(
            "Economy: production updated α={:.3} β={:.3} from {} observations",
            self.production.alpha, self.production.beta, self.observations.len()
        );
    }

    /// Start tracking a new task's trajectory telemetry.
    pub fn start_telemetry(&mut self, task_id: &str, model_id: &str) {
        self.current_telemetry = Some(TrajectoryTelemetry {
            task_id: task_id.to_string(),
            model_id: model_id.to_string(),
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_latency_ms: 0,
            retry_count: 0,
            tool_call_count: 0,
            success: true,
            human_correction_needed: false,
            downstream_reuse: false,
            failure_category: None,
        });
    }

    /// Accumulate token/latency into current telemetry.
    pub fn accumulate_telemetry(&mut self, input_tokens: u64, output_tokens: u64, latency_ms: u64, tool_calls: u64) {
        if let Some(ref mut t) = self.current_telemetry {
            t.total_input_tokens += input_tokens;
            t.total_output_tokens += output_tokens;
            t.total_latency_ms += latency_ms;
            t.tool_call_count += tool_calls;
        }
    }

    /// Mark the current task telemetry as failed with a category.
    pub fn fail_telemetry(&mut self, category: &str) {
        if let Some(ref mut t) = self.current_telemetry {
            t.success = false;
            t.failure_category = Some(category.to_string());
        }
    }

    /// Finalize and return the current telemetry, resetting the tracker.
    pub fn emit_telemetry(&mut self) -> Option<TrajectoryTelemetry> {
        self.current_telemetry.take()
    }
}
