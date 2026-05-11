//! Cognitive Economics Engine — Token allocation as constrained optimization.
//!
//! Core model:
//!   Q = f(T; θ)  — concave production function (diminishing returns)
//!   max Σ[V(Q_i) - p·T_i]  s.t.  Σ T_i ≤ B
//!
//! Key interventions:
//!   1. Production function estimation from SkillGenome data
//!   2. Shadow price λ injection into system prompt
//!   3. EVPI-based LLM call gating (skip when cached quality ≥ threshold)
//!   4. Optimal token ceiling per task type
//!   5. Crystallization priority by total token cost avoided (AUC)

use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════
// Core Types
// ═══════════════════════════════════════════════════════

/// Cognitive budget for a single task session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveBudget {
    /// Total token budget allocated to this session.
    pub total_budget: u64,
    /// Tokens consumed so far.
    pub spent: u64,
    /// Current shadow price λ (rises as budget depletes).
    pub shadow_price: f64,
    /// Task complexity parameter θ.
    pub complexity: f64,
    /// Estimated optimal token ceiling T* for this complexity.
    pub optimal_ceiling: u64,
    /// Whether LLM calls are currently gated (EVPI too low).
    pub llm_gated: bool,
}

impl CognitiveBudget {
    pub fn new(total_budget: u64, complexity: f64) -> Self {
        // Initial shadow price: low when budget is abundant
        let shadow_price = 0.01 / total_budget as f64;
        // Optimal ceiling from estimated production function
        let optimal_ceiling = estimate_optimal_tokens(total_budget, complexity);
        Self {
            total_budget, spent: 0, shadow_price,
            complexity, optimal_ceiling, llm_gated: false,
        }
    }

    /// Spend tokens and recalculate shadow price.
    pub fn spend(&mut self, tokens: u64) {
        self.spent += tokens;
        let remaining = self.total_budget.saturating_sub(self.spent) as f64;
        self.shadow_price = if remaining > 0.0 { 1.0 / remaining } else { 10.0 };
        // Gate LLM calls when budget is critically low
        self.llm_gated = remaining < self.total_budget as f64 * 0.05;
    }

    /// Remaining budget.
    pub fn remaining(&self) -> u64 { self.total_budget.saturating_sub(self.spent) }

    /// Build shadow price directive for system prompt.
    /// Sets explicit token constraint that LLMs have been shown to obey.
    pub fn shadow_directive(&self) -> String {
        let remaining = self.remaining();
        if remaining > 2000 {
            String::new()
        } else if remaining > 500 {
            format!("\n// ⚠ Token budget remaining: {}. Be concise.\n", remaining)
        } else {
            format!("\n// 🚨 Critical token budget: {}. MAXIMALLY concise. Output ONLY the script, no explanation.\n", remaining)
        }
    }

    /// Check if LLM call is economically justified via EVPI.
    /// Returns true if we should call LLM, false if cached skill suffices.
    pub fn should_call_llm(&self, evpi: f64, estimated_tokens: u64) -> bool {
        if self.llm_gated { return false; }
        evpi > self.shadow_price * estimated_tokens as f64
    }
}

// ═══════════════════════════════════════════════════════
// Production Function Estimation
// ═══════════════════════════════════════════════════════

/// Estimated production function parameters for a given task complexity θ.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductionFunction {
    /// α parameter: asymptotic quality ceiling.
    pub alpha: f64,
    /// β parameter: rate of quality growth.
    pub beta: f64,
    /// θ parameter: task complexity (higher θ = slower growth).
    pub theta: f64,
}

impl ProductionFunction {
    /// Q = α * (1 - e^(-β*T/θ))
    pub fn quality(&self, tokens: u64) -> f64 {
        self.alpha * (1.0 - (-self.beta * tokens as f64 / self.theta.max(0.1)).exp())
    }

    /// Marginal quality: dQ/dT = α*β/θ * e^(-β*T/θ)
    pub fn marginal_quality(&self, tokens: u64) -> f64 {
        self.alpha * self.beta / self.theta.max(0.1)
            * (-self.beta * tokens as f64 / self.theta.max(0.1)).exp()
    }

    /// Estimate from SkillGenome data.
    /// alpha = max observed success_rate for this complexity
    /// beta  = average composability_score of successful entries
    pub fn estimate_from_genome(capability: &str, max_success_rate: f32, avg_composability: f32) -> Self {
        Self {
            alpha: max_success_rate as f64,
            beta: 0.5 + avg_composability as f64 * 0.5,
            theta: match capability {
                "grep" | "list_dir" => 0.3,   // simple
                "read_file" | "write_file" => 0.5, // moderate
                "shell" => 0.8,                // complex
                _ => 0.6,
            },
        }
    }
}

/// Estimate optimal token ceiling T* where marginal quality = shadow price.
/// Solves: α*β/θ * e^(-β*T/θ) = p + λ
pub fn estimate_optimal_tokens(total_budget: u64, complexity: f64) -> u64 {
    // Heuristic: allocate proportionally to complexity
    // Simple tasks: 20% of budget, cap at 500
    // Medium tasks: 40% of budget, cap at 2000
    // Deep tasks: 60% of budget, cap at 8000
    let share = if complexity < 0.3 { 0.2 } else if complexity < 0.6 { 0.4 } else { 0.6 };
    let raw = (total_budget as f64 * share) as u64;
    let cap = if complexity < 0.3 { 500 } else if complexity < 0.6 { 2000 } else { 8000 };
    raw.min(cap).max(50)
}

// ═══════════════════════════════════════════════════════
// EVPI (Expected Value of Perfect Information) Estimator
// ═══════════════════════════════════════════════════════

/// Estimate EVPI for a task given genome similarity score.
/// High similarity → low EVPI (cached skill is good enough).
/// Low similarity → high EVPI (LLM call adds value).
#[derive(Debug, Clone)]
pub struct EvpiEstimator {
    /// Quality achieved by best cached skill (proxy: genome similarity).
    pub cached_quality: f64,
    /// Expected quality from LLM call (proxy: 1.0 - complexity-based risk).
    pub expected_llm_quality: f64,
}

impl EvpiEstimator {
    pub fn new(genome_similarity: f64, complexity: f64, cached_success_rate: f32) -> Self {
        let cached_quality = genome_similarity * cached_success_rate as f64;
        let expected_llm_quality = 0.95 - complexity * 0.3; // higher complexity = more risk
        Self { cached_quality, expected_llm_quality }
    }

    /// EVPI = E[Q_llm] - Q_cached
    /// If EVPI < token_cost, skip LLM call.
    pub fn evpi(&self) -> f64 {
        (self.expected_llm_quality - self.cached_quality).max(0.0)
    }

    /// Should we call LLM?
    pub fn should_call(&self, estimated_token_cost: f64, shadow_price: f64) -> bool {
        self.evpi() > shadow_price * estimated_token_cost
    }
}

// ═══════════════════════════════════════════════════════
// Crystallization Priority (AUC-based)
// ═══════════════════════════════════════════════════════

/// Crystallization priority based on total token cost avoided.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrystallizationPriority {
    pub skill_id: String,
    /// Total input tokens this skill consumed over its lifetime.
    pub total_input_tokens: u64,
    /// Total output tokens consumed.
    pub total_output_tokens: u64,
    /// Estimated annual usage frequency.
    pub annual_frequency: f64,
    /// AUC: Area Under the Curve — total token cost if not crystallized.
    pub total_cost_avoided: f64,
    /// Priority score (higher = crystallize first).
    pub priority_score: f64,
}

impl CrystallizationPriority {
    /// Compute priority: total_token_cost * annual_frequency * decay
    pub fn compute(
        skill_id: &str,
        usage_count: usize,
        avg_token_cost: usize,
        input_cost_per_1k: f64,
        output_cost_per_1k: f64,
    ) -> Self {
        let total_tokens = usage_count as u64 * avg_token_cost as u64;
        let input_tokens = total_tokens * 3 / 4; // rough estimate: 75% input
        let output_tokens = total_tokens / 4;
        let annual_freq = usage_count as f64 / 30.0; // rough: usage per month
        let cost_avoided = total_tokens as f64 * (input_cost_per_1k + output_cost_per_1k * 3.0) / 1000.0;
        // Future value: cost * frequency, with time decay
        let priority = cost_avoided * annual_freq * 0.8; // 0.8 decay factor

        Self {
            skill_id: skill_id.to_string(),
            total_input_tokens: input_tokens,
            total_output_tokens: output_tokens,
            annual_frequency: annual_freq,
            total_cost_avoided: cost_avoided,
            priority_score: priority,
        }
    }
}

// ═══════════════════════════════════════════════════════
// Model Market (Dynamic Provider Routing by ROI)
// ═══════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct ModelMarketQuote {
    pub model_id: String,
    pub input_cost_per_1k: f64,
    pub output_cost_per_1k: f64,
    pub avg_success_rate: f64,
    pub avg_latency_ms: u64,
    pub domain_strengths: Vec<String>,
}

impl ModelMarketQuote {
    /// Expected ROI: success_rate / (input_cost + output_cost)
    pub fn expected_roi(&self, estimated_input_tokens: u64, estimated_output_tokens: u64) -> f64 {
        let total_cost = (estimated_input_tokens as f64 * self.input_cost_per_1k
            + estimated_output_tokens as f64 * self.output_cost_per_1k * 3.0) / 1000.0;
        if total_cost <= 0.0 { return self.avg_success_rate; }
        self.avg_success_rate / total_cost
    }
}

/// Pre-computed model market quotes (updated periodically).
pub fn model_market_quotes() -> Vec<ModelMarketQuote> {
    vec![
        ModelMarketQuote {
            model_id: "deepseek/deepseek-v4-flash".into(),
            input_cost_per_1k: 0.000014,   // $0.014/M input
            output_cost_per_1k: 0.000028,  // $0.028/M output
            avg_success_rate: 0.88,
            avg_latency_ms: 800,
            domain_strengths: vec!["code".into(), "translation".into()],
        },
        ModelMarketQuote {
            model_id: "gemini/gemini-2.5-flash".into(),
            input_cost_per_1k: 0.00015,
            output_cost_per_1k: 0.00060,
            avg_success_rate: 0.92,
            avg_latency_ms: 500,
            domain_strengths: vec!["reasoning".into(), "multimodal".into()],
        },
        ModelMarketQuote {
            model_id: "groq/llama-3.3-70b-versatile".into(),
            input_cost_per_1k: 0.000059,
            output_cost_per_1k: 0.000079,
            avg_success_rate: 0.85,
            avg_latency_ms: 200,
            domain_strengths: vec!["code".into(), "fast".into()],
        },
    ]
}

/// Select best model by ROI given task characteristics.
pub fn select_model_by_roi<'a>(
    quotes: &'a [ModelMarketQuote],
    estimated_input_tokens: u64,
    estimated_output_tokens: u64,
    _required_domain: Option<&str>,
) -> Option<&'a ModelMarketQuote> {
    quotes.iter()
        .max_by(|a, b| {
            a.expected_roi(estimated_input_tokens, estimated_output_tokens)
                .partial_cmp(&b.expected_roi(estimated_input_tokens, estimated_output_tokens))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

// ═══════════════════════════════════════════════════════
// Context Optimal Stopping (Marginal Info Gain)
// ═══════════════════════════════════════════════════════

/// Decide whether to load another context chunk.
/// Stops when marginal value < marginal cost.
pub struct ContextLoader {
    pub loaded_token_count: u64,
    pub cumulative_quality: f64,
    pub production_fn: ProductionFunction,
}

impl ContextLoader {
    pub fn new(production_fn: ProductionFunction) -> Self {
        Self { loaded_token_count: 0, cumulative_quality: 0.0, production_fn }
    }

    /// Should we load another chunk of `chunk_tokens` size?
    /// Returns true if marginal quality gain > token cost.
    pub fn should_load(&self, chunk_tokens: u64, token_cost_per_1k: f64) -> bool {
        let marginal_q = self.production_fn.marginal_quality(self.loaded_token_count);
        let marginal_cost = chunk_tokens as f64 * token_cost_per_1k / 1000.0;
        // Scale: quality is 0-1, cost is dollars. Use shadow price as scaling factor.
        marginal_q > marginal_cost * 100.0 // heuristic scaling
    }

    pub fn load_chunk(&mut self, chunk_tokens: u64) {
        self.loaded_token_count += chunk_tokens;
        self.cumulative_quality = self.production_fn.quality(self.loaded_token_count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_production_function_diminishing_returns() {
        let pf = ProductionFunction { alpha: 1.0, beta: 0.5, theta: 0.5 };
        let q100 = pf.marginal_quality(100);
        let q1000 = pf.marginal_quality(1000);
        assert!(q100 > q1000, "Marginal quality should diminish");
    }

    #[test]
    fn test_budget_shadow_price_rises() {
        let mut budget = CognitiveBudget::new(1000, 0.5);
        let initial_lambda = budget.shadow_price;
        budget.spend(900);
        assert!(budget.shadow_price > initial_lambda);
    }

    #[test]
    fn test_evpi_gating() {
        let evpi = EvpiEstimator::new(0.92, 0.3, 0.90);
        // evpi = max(0.95-0.3*0.3 - 0.92*0.90, 0) = max(0.032, 0) = 0.032
        // High similarity → low EVPI → skip LLM if cost > EVPI
        // threshold(5.0, 0.01) = 0.05 > 0.032 → should skip
        assert!(!evpi.should_call(5.0, 0.01));

        let evpi2 = EvpiEstimator::new(0.3, 0.8, 0.7);
        // evpi = max(0.95-0.8*0.3 - 0.3*0.7, 0) = max(0.71-0.21, 0) = 0.50
        // Low similarity → high EVPI → call LLM
        // threshold(3.0, 0.01) = 0.03 < 0.50 → should call
        assert!(evpi2.should_call(3.0, 0.01));
    }

    #[test]
    fn test_crystallization_priority() {
        let cp = CrystallizationPriority::compute("skill-1", 500, 200, 0.00015, 0.00060);
        assert!(cp.priority_score > 0.0);
        assert_eq!(cp.skill_id, "skill-1");
    }

    #[test]
    fn test_model_roi_selection() {
        let quotes = model_market_quotes();
        let best = select_model_by_roi(&quotes, 1000, 200, None);
        assert!(best.is_some());
        // DeepSeek should have highest ROI due to low cost
        assert_eq!(best.unwrap().model_id, "deepseek/deepseek-v4-flash");
    }
}


// ═══════════════════════════════════════════════════════
// Dynamic General Equilibrium Extensions
// ═══════════════════════════════════════════════════════

pub struct TokenReserve { pub total_liquidity: u64, pub reserve_ratio: f64, pub emergency_buffer: u64, pub burst_quota: u64, pub recession_mode: bool }
impl TokenReserve {
    pub fn new(total: u64) -> Self { Self { total_liquidity: total, reserve_ratio: 0.15, emergency_buffer: total/10, burst_quota: total/20, recession_mode: false } }
    pub fn withdraw(&mut self, req: u64, prio: f64) -> u64 { let avail = self.total_liquidity.saturating_sub(self.emergency_buffer); let g = if self.recession_mode { (req as f64*0.5) as u64 } else { (req as f64*prio.min(1.0)) as u64 }; let a = g.min(avail); self.total_liquidity = self.total_liquidity.saturating_sub(a); if self.total_liquidity < self.emergency_buffer { self.recession_mode = true; } a }
    pub fn deposit(&mut self, u: u64) { self.total_liquidity += u; }
}

pub struct RecursiveEvpi { pub base_evpi: f64, pub branching_factor: f64, pub expected_retries: f64, pub hallucination_risk: f64 }
impl RecursiveEvpi {
    pub fn new(base: f64, complexity: f64, success_rate: f64) -> Self { let fr = 1.0-success_rate; Self { base_evpi: base, branching_factor: 1.0+complexity*2.0, expected_retries: fr*3.0, hallucination_risk: complexity*0.2*fr } }
    pub fn adjusted_evpi(&self, retry_cost: f64, hall_cost: f64) -> f64 { (self.base_evpi - self.expected_retries*retry_cost - self.hallucination_risk*hall_cost).max(0.0) }
}

pub struct ContextTax { pub scores: Vec<f64>, pub rate: f64 }
impl ContextTax {
    pub fn new(rate: f64) -> Self { Self { scores: vec![], rate } }
    pub fn tax_value(&self, voi: f64, pollution: f64) -> f64 { (voi - self.rate*pollution).max(0.0) }
    pub fn record(&mut self, p: f64) { self.scores.push(p); }
    pub fn total_pollution(&self) -> f64 { self.scores.iter().sum() }
}

pub fn crystallization_variance_adjusted(_skill_id: &str, avg_saved: f64, failure_rate: f32, retry_cost: f64, risk_aversion: f64) -> f64 { (avg_saved - risk_aversion*failure_rate as f64*retry_cost).max(0.0) }

pub struct AdaptiveMarketMaker { pub quotes: Vec<ModelMarketQuote>, pub rate_limit_probs: std::collections::HashMap<String,f64>, pub latency_drifts: std::collections::HashMap<String,f64>, pub instability_penalties: std::collections::HashMap<String,f64> }
impl AdaptiveMarketMaker {
    pub fn new(q: Vec<ModelMarketQuote>) -> Self { let mut r=std::collections::HashMap::new(); let mut l=std::collections::HashMap::new(); let mut i=std::collections::HashMap::new(); for qq in &q { r.insert(qq.model_id.clone(),0.05); l.insert(qq.model_id.clone(),1.0); i.insert(qq.model_id.clone(),0.0); } Self { quotes: q, rate_limit_probs: r, latency_drifts: l, instability_penalties: i } }
    pub fn effective_cost(&self, mid: &str, base: f64) -> f64 { let rl=self.rate_limit_probs.get(mid).copied().unwrap_or(0.0); let ld=self.latency_drifts.get(mid).copied().unwrap_or(1.0); let ins=self.instability_penalties.get(mid).copied().unwrap_or(0.0); base*(1.0+rl)*ld+ins }
    pub fn speculative_parallelism_value(&self, cheap: &str, expensive: &str, it: u64, ot: u64) -> bool { let cr = self.quotes.iter().find(|q|q.model_id==cheap).map(|q|q.expected_roi(it,ot)).unwrap_or(0.0); let er = self.quotes.iter().find(|q|q.model_id==expensive).map(|q|q.expected_roi(it,ot)).unwrap_or(0.0); cr*5.0*0.7 > er }
}


// ═══════════════════════════════════════════════════════
// DSGE Macroeconomic Layer
// ═══════════════════════════════════════════════════════

/// System-wide cognitive macro state — the "economy" of the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveMacroState {
    pub token_reserves: f64,
    pub context_inflation: f64,
    pub average_entropy: f64,
    pub model_liquidity: f64,
    pub hallucination_rate: f64,
    pub memory_capital_stock: f64,
    pub memory_depreciation_rate: f64,
    pub active_cognitive_debt: f64,
    pub productivity_index: f64,
    pub crisis_mode: bool,
    /// Host OS CPU usage percentage (0.0 to 100.0)
    pub system_cpu_usage: f64,
    /// Host OS free memory in MB
    pub system_memory_free_mb: u64,
    /// Host OS total memory in MB
    pub system_memory_total_mb: u64,
}

impl CognitiveMacroState {
    pub fn new() -> Self {
        Self {
            token_reserves: 100_000.0, context_inflation: 0.0,
            average_entropy: 0.3, model_liquidity: 1.0,
            hallucination_rate: 0.05, memory_capital_stock: 0.0,
            memory_depreciation_rate: 0.02, active_cognitive_debt: 0.0,
            productivity_index: 1.0, crisis_mode: false,
            system_cpu_usage: 0.0,
            system_memory_free_mb: 4096,
            system_memory_total_mb: 8192,
        }
    }

    /// Context inflation: π = (C - C*) / C*
    /// C*: optimal context capacity (estimated)
    pub fn update_inflation(&mut self, current_context_tokens: u64, optimal_context: u64) {
        self.context_inflation = if optimal_context > 0 {
            (current_context_tokens as f64 - optimal_context as f64) / optimal_context as f64
        } else { 0.0 };
        self.context_inflation = self.context_inflation.clamp(-1.0, 5.0);
    }

    /// Memory capital accumulation: K_{t+1} = (1-δ)·K_t + I_t
    pub fn accumulate_memory(&mut self, investment: f64) {
        self.memory_capital_stock = (1.0 - self.memory_depreciation_rate) * self.memory_capital_stock + investment;
    }

    /// Endogenous productivity: A_{t+1} = A_t + φ·R_t (Romer-type)
    pub fn grow_productivity(&mut self, reasoning_investment: f64, learning_phi: f64) {
        self.productivity_index += learning_phi * reasoning_investment;
        self.productivity_index = self.productivity_index.max(0.5);
    }

    /// Cognitive Taylor Rule: reasoning_rate = base + φ_entropy·entropy + φ_inflation·inflation
    pub fn taylor_rule(&self, base_rate: f64, entropy_weight: f64, inflation_weight: f64) -> f64 {
        let entropy_pressure = entropy_weight * self.average_entropy;
        let inflation_pressure = inflation_weight * self.context_inflation.max(0.0);
        (base_rate + entropy_pressure + inflation_pressure).clamp(0.1, 5.0)
    }

    /// Crisis regime detection: API outage, rate limit, extreme inflation, or host OS resource exhaustion
    pub fn detect_crisis(&mut self, rate_limit_prob: f64, api_unavailable: bool) {
        // Trigger crisis_mode if host CPU is saturated (> 85%) or free memory is critically low (< 512 MB)
        let system_saturated = self.system_cpu_usage > 85.0 || (self.system_memory_free_mb < 512 && self.system_memory_total_mb > 1024);

        self.crisis_mode = api_unavailable
            || rate_limit_prob > 0.7
            || self.context_inflation > 2.0
            || self.active_cognitive_debt > 2.0
            || system_saturated;
        if self.crisis_mode { self.model_liquidity *= 0.5; }
    }

    /// Emergency lender of last resort: local SLM fallback, offline heuristics
    pub fn crisis_response(&self) -> CrisisPolicy {
        if !self.crisis_mode { return CrisisPolicy::Normal; }
        if self.token_reserves < 1000.0 { return CrisisPolicy::FullOffline; }
        if self.context_inflation > 1.5 { return CrisisPolicy::AggressiveCompression; }
        CrisisPolicy::CheapModelOnly
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrisisPolicy {
    Normal,
    CheapModelOnly,
    AggressiveCompression,
    FullOffline,
}

/// Heterogeneous agent economy: each sub-agent has different characteristics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    pub id: String,
    pub risk_tolerance: f64,
    pub reasoning_efficiency: f64,
    pub hallucination_tendency: f64,
    pub specialization: AgentSpecialization,
    pub allocated_token_share: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentSpecialization {
    Fast, Deep, Verifier, Researcher, Planner,
}

impl AgentProfile {
    pub fn new(id: &str, spec: AgentSpecialization) -> Self {
        let (risk, eff, hall, share) = match spec {
            AgentSpecialization::Fast => (0.3, 0.9, 0.08, 0.15),
            AgentSpecialization::Deep => (0.8, 0.5, 0.03, 0.35),
            AgentSpecialization::Verifier => (0.2, 0.7, 0.02, 0.10),
            AgentSpecialization::Researcher => (0.6, 0.4, 0.10, 0.25),
            AgentSpecialization::Planner => (0.5, 0.6, 0.05, 0.15),
        };
        Self { id: id.to_string(), risk_tolerance: risk, reasoning_efficiency: eff, hallucination_tendency: hall, specialization: spec, allocated_token_share: share }
    }

    /// Compute token allocation for this agent given total budget.
    pub fn token_allocation(&self, total_budget: u64) -> u64 {
        (total_budget as f64 * self.allocated_token_share) as u64
    }
}

/// Forward-looking planning: estimate future token value.
pub struct ForwardPlanner {
    pub expected_future_demand: f64,
    pub expected_rate_limit_prob: f64,
    pub expected_provider_volatility: f64,
    pub discount_factor: f64,
}

impl ForwardPlanner {
    pub fn new(discount_factor: f64) -> Self {
        Self { expected_future_demand: 0.3, expected_rate_limit_prob: 0.1, expected_provider_volatility: 0.05, discount_factor }
    }

    /// Bayesian update of expected future demand.
    pub fn observe(&mut self, recent_token_usage: u64, max_capacity: u64) {
        let usage_ratio = recent_token_usage as f64 / max_capacity.max(1) as f64;
        // Exponential moving average
        self.expected_future_demand = 0.7 * self.expected_future_demand + 0.3 * usage_ratio;
    }

    /// Intertemporal shadow price: λ_t = β · E_t[∂V_{t+1}/∂B_{t+1}]
    pub fn intertemporal_shadow_price(&self, current_shadow_price: f64) -> f64 {
        current_shadow_price * (1.0 + self.expected_future_demand * self.discount_factor)
    }

    /// Forward guidance for system prompt.
    pub fn guidance(&self) -> String {
        if self.expected_rate_limit_prob > 0.5 {
            "// ⚡ High rate limit risk — prefer cached skills.".into()
        } else if self.expected_provider_volatility > 0.3 {
            "// 🌊 Provider instability — use deterministic fallbacks.".into()
        } else { String::new() }
    }
}


// ═══════════════════════════════════════════════════════
// Bellman Dynamic Optimization
// ═══════════════════════════════════════════════════════

pub struct BellmanPlanner { pub discount_factor: f64, pub value_table: std::collections::HashMap<u64,f64> }
impl BellmanPlanner {
    pub fn new(discount_factor: f64) -> Self { Self { discount_factor, value_table: std::collections::HashMap::new() } }
    pub fn endogenous_shadow_price(&self, reserves: f64, expected_future_demand: f64, crisis_prob: f64, productivity_growth: f64) -> f64 {
        let precautionary = expected_future_demand * crisis_prob * 2.0;
        let investment_return = productivity_growth * 0.5;
        (1.0 / reserves.max(1.0)) + precautionary - investment_return
    }
    pub fn bellman_iteration(&mut self, budget: u64, _complexity: f64, production: &ProductionFunction) -> f64 {
        if let Some(v) = self.value_table.get(&budget) { return *v; }
        let limit = budget.min(5000);
        let mut dp = vec![0.0f64; limit as usize + 1];
        for b in 1..=limit {
            let mut best = 0.0f64;
            for t in 0..=b.min(200) {
                let q = production.quality(t);
                let future_v = self.discount_factor * dp[(b - t) as usize];
                best = best.max(q + future_v);
            }
            dp[b as usize] = best;
        }
        let result = dp[limit as usize];
        self.value_table.insert(budget, result);
        result
    }
}

pub struct KnowledgeCapital {
    pub reusable_trajectories: f64, pub crystallized_skills: f64,
    pub memory_embedding_quality: f64, pub policy_compression_ratio: f64,
    pub depreciation_rate: f64, pub cumulative_investment: f64,
}
impl KnowledgeCapital {
    pub fn new() -> Self { Self { reusable_trajectories: 0.0, crystallized_skills: 0.0, memory_embedding_quality: 0.5, policy_compression_ratio: 0.3, depreciation_rate: 0.01, cumulative_investment: 0.0 } }
    pub fn total(&self) -> f64 { self.reusable_trajectories + self.crystallized_skills + self.memory_embedding_quality + self.policy_compression_ratio }
    pub fn future_token_savings(&self, annual_token_budget: f64) -> f64 { annual_token_budget * self.total() * 0.5 }
    pub fn invest_research(&mut self, tokens: f64, learning_phi: f64) { self.cumulative_investment += tokens; self.reusable_trajectories += learning_phi * tokens * 0.001; self.crystallized_skills += learning_phi * tokens * 0.0005; }
    pub fn depreciate(&mut self) { let r = 1.0 - self.depreciation_rate; self.reusable_trajectories *= r; self.crystallized_skills *= r; self.memory_embedding_quality *= r; self.policy_compression_ratio *= r; }
}

pub struct NonlinearContextInflation {
    pub current_load: f64, pub optimal_capacity: f64, pub sigmoid_steepness: f64,
}
impl NonlinearContextInflation {
    pub fn new(optimal_capacity: f64) -> Self { Self { current_load: 0.0, optimal_capacity, sigmoid_steepness: 0.01 } }
    pub fn update(&mut self, current_tokens: u64) { self.current_load = current_tokens as f64; }
    pub fn inflation(&self) -> f64 { let x = self.current_load - self.optimal_capacity; -0.2 + 1.0 / (1.0 + (-self.sigmoid_steepness * x).exp()) }
    pub fn collapse_risk(&self) -> f64 { let inf = self.inflation(); if inf > 0.8 { 0.9 } else if inf > 0.5 { 0.5 } else { 0.05 } }
}

pub struct ProviderPortfolio {
    pub allocations: Vec<(String, f64)>, pub correlation_matrix: std::collections::HashMap<(String,String),f64>,
}
impl ProviderPortfolio {
    pub fn new() -> Self { Self { allocations: Vec::new(), correlation_matrix: std::collections::HashMap::new() } }
    pub fn markowitz_allocation(&self, risk_aversion: f64) -> Vec<(String, f64)> {
        let n = self.allocations.len();
        if n == 0 { return Vec::new(); }
        let mut weights = vec![1.0 / n as f64; n];
        for _ in 0..20 {
            let mut grad = vec![0.0; n];
            for i in 0..n {
                let mut cov_term = 0.0;
                for j in 0..n {
                    let corr = if i == j { 1.0 } else { self.correlated_providers(&self.allocations[i].0, &self.allocations[j].0) };
                    cov_term += corr * weights[j] * 0.01;
                }
                grad[i] = self.allocations[i].1 - risk_aversion * cov_term;
            }
            let old_weights = weights.clone(); for (wi, w) in weights.iter_mut().enumerate() { *w = (*w + 0.1 * grad[wi] * old_weights[wi]).clamp(0.01, 1.0); }
            let sum: f64 = weights.iter().sum();
            for w in &mut weights { *w /= sum; }
        }
        self.allocations.iter().enumerate().map(|(i, (id, _))| (id.clone(), weights[i])).collect()
    }
    pub fn correlated_providers(&self, a: &str, b: &str) -> f64 { self.correlation_matrix.get(&(a.to_string(),b.to_string())).copied().unwrap_or(0.3) }
}

pub struct CognitiveLoan { pub principal_tokens: u64, pub expected_future_savings: f64, pub default_probability: f64, pub term_turns: u64 }
impl CognitiveLoan {
    pub fn new(principal: u64, savings: f64, default_prob: f64) -> Self { Self { principal_tokens: principal, expected_future_savings: savings, default_probability: default_prob, term_turns: 10 } }
    pub fn expected_value(&self) -> f64 { self.expected_future_savings * (1.0 - self.default_probability) - self.principal_tokens as f64 }
    pub fn approve(&self) -> bool { self.expected_value() > 0.0 && self.default_probability < 0.3 }
}

pub struct ExpectationSimulator { pub future_token_prices: Vec<f64>, pub volatility_estimate: f64, pub regime_probabilities: std::collections::HashMap<String,f64>, pub rng: rand::rngs::StdRng }
impl ExpectationSimulator {
    pub fn new() -> Self { use rand::SeedableRng; Self { future_token_prices: Vec::new(), volatility_estimate: 0.1, regime_probabilities: std::collections::HashMap::new(), rng: rand::rngs::StdRng::seed_from_u64(42) } }
    /// Create with a specific seed for reproducible simulations.
    pub fn with_seed(seed: u64) -> Self { use rand::SeedableRng; Self { future_token_prices: Vec::new(), volatility_estimate: 0.1, regime_probabilities: std::collections::HashMap::new(), rng: rand::rngs::StdRng::seed_from_u64(seed) } }
    pub fn simulate(&mut self, current_price: f64, steps: usize) -> Vec<f64> {
        use rand::Rng;
        self.future_token_prices.clear();
        let mut price = current_price;
        for _ in 0..steps {
            let r: f64 = self.rng.gen_range(0.0..1.0);
            let shock = (r - 0.5) * self.volatility_estimate;
            price = (price + shock).max(0.001);
            self.future_token_prices.push(price);
        }
        self.future_token_prices.clone()
    }
    pub fn expected_future_price(&self, horizon: usize) -> f64 {
        if self.future_token_prices.is_empty() { return 0.01; }
        let n = horizon.min(self.future_token_prices.len());
        self.future_token_prices.iter().take(n).sum::<f64>() / n as f64
    }
}



// ═══════════════════════════════════════════════════════
// Regime Switching + General Equilibrium
// ═══════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MacroRegime { Normal, Congestion, Crisis, Offline }

pub struct RegimeSwitcher {
    pub current: MacroRegime,
    pub transition_matrix: [[f64; 4]; 4],
    pub regime_durations: std::collections::HashMap<MacroRegime, u64>,
}
impl RegimeSwitcher {
    pub fn new() -> Self {
        let mut tm = [[0.0; 4]; 4];
        tm[0] = [0.80, 0.15, 0.04, 0.01];
        tm[1] = [0.30, 0.50, 0.15, 0.05];
        tm[2] = [0.10, 0.30, 0.50, 0.10];
        tm[3] = [0.20, 0.10, 0.30, 0.40];
        Self { current: MacroRegime::Normal, transition_matrix: tm, regime_durations: std::collections::HashMap::new() }
    }
    pub fn update(&mut self, macro_state: &CognitiveMacroState, rate_limit_prob: f64, api_unavailable: bool) {
        let row = self.transition_matrix[self.current as usize];
        let mut adjusted = row;
        if macro_state.context_inflation > 0.5 { adjusted[1] += 0.2; adjusted[0] -= 0.2; }
        if macro_state.crisis_mode || rate_limit_prob > 0.5 { adjusted[2] += 0.3; adjusted[0] -= 0.3; }
        if api_unavailable { adjusted[3] += 0.5; adjusted[0] -= 0.5; }
        for v in &mut adjusted { *v = v.clamp(0.0, 1.0); }
        let r: f64 = rand::random::<f64>();
        let mut cum = 0.0;
        for (i, p) in adjusted.iter().enumerate() {
            cum += p;
            if r < cum { self.current = match i { 0=>MacroRegime::Normal, 1=>MacroRegime::Congestion, 2=>MacroRegime::Crisis, _=>MacroRegime::Offline }; break; }
        }
        *self.regime_durations.entry(self.current).or_insert(0) += 1;
    }
    pub fn policy(&self) -> RegimePolicy {
        match self.current {
            MacroRegime::Normal => RegimePolicy { max_tokens: 8192, max_retries: 3, prefer_cache: false },
            MacroRegime::Congestion => RegimePolicy { max_tokens: 4096, max_retries: 2, prefer_cache: true },
            MacroRegime::Crisis => RegimePolicy { max_tokens: 2048, max_retries: 1, prefer_cache: true },
            MacroRegime::Offline => RegimePolicy { max_tokens: 512, max_retries: 0, prefer_cache: true },
        }
    }
}
pub struct RegimePolicy { pub max_tokens: u32, pub max_retries: u32, pub prefer_cache: bool }

pub struct EndogenousHallucinationModel { pub base_rate: f64, pub inflation_sensitivity: f64, pub entropy_sensitivity: f64, pub fatigue_factor: f64 }
impl EndogenousHallucinationModel {
    pub fn new() -> Self { Self { base_rate: 0.05, inflation_sensitivity: 0.15, entropy_sensitivity: 0.10, fatigue_factor: 0.02 } }
    pub fn rate(&self, inflation: f64, entropy: f64, turns: u64) -> f64 { (self.base_rate + self.inflation_sensitivity * inflation.max(0.0) + self.entropy_sensitivity * entropy + self.fatigue_factor * turns as f64).clamp(0.0, 0.95) }
}

pub struct GeneralEquilibrium { pub agents: Vec<AgentProfile>, pub total_supply: u64, pub shadow_price: f64 }
impl GeneralEquilibrium {
    pub fn new(supply: u64) -> Self { Self { agents: Vec::new(), total_supply: supply, shadow_price: 0.01 } }
    pub fn add(&mut self, a: AgentProfile) { self.agents.push(a); }
    pub fn clear(&mut self, iters: usize) -> f64 {
        for _ in 0..iters {
            // Bounded linear demand: each agent demands share * supply adjusted by urgency
            // When p < 1.0 (tokens cheap): demand rises with risk_tolerance * elasticity
            // When p > 1.0 (tokens expensive): demand falls toward 0
            let demand: f64 = self.agents.iter().map(|a| {
                let base = a.allocated_token_share * self.total_supply as f64;
                let price_gap = 1.0 - self.shadow_price;
                let elasticity = a.risk_tolerance * 0.5;
                (base + elasticity * price_gap * base).max(0.0).min(self.total_supply as f64 * 2.0)
            }).sum();
            let excess = demand - self.total_supply as f64;
            // Damped Walrasian tâtonnement
            let step = 0.05 * excess / self.total_supply.max(1) as f64;
            self.shadow_price = (self.shadow_price + step).clamp(0.001, 10.0);
            if excess.abs() < 1.0 { break; }
        }
        self.shadow_price
    }
}

pub struct TokenDerivative { pub kind: DerivativeKind, pub strike_price: f64, pub quantity: u64, pub expiry: u64 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DerivativeKind { Future, Insurance, Option }
impl TokenDerivative {
    pub fn new(kind: DerivativeKind, strike: f64, qty: u64, expiry: u64) -> Self { Self { kind, strike_price: strike, quantity: qty, expiry } }
    pub fn payoff(&self, current_price: f64) -> f64 {
        match self.kind {
            DerivativeKind::Future => (current_price - self.strike_price) * self.quantity as f64,
            DerivativeKind::Insurance => if current_price > self.strike_price { current_price * self.quantity as f64 } else { 0.0 },
            DerivativeKind::Option => ((current_price - self.strike_price).max(0.0)) * self.quantity as f64,
        }
    }
}
