//! DSGE Integration Layer — connects cognitive_economics to the Agent loop.
//!
//! 5 injection points in chat_on_session():
//!
//!   [1] Budget creation    → CognitiveBudget::new(budget_tokens, complexity)
//!   [2] Entropy monitoring → MacroState.update_inflation(context_tokens, optimal)
//!   [3] System prompt       → budget.shadow_directive() appended to messages[0].content
//!   [4] Model selection     → select_model_by_roi() picks provider by cost/success
//!   [5] Reflection cycle    → KnowledgeCapital.invest_research(tokens_spent, learning_phi)

use crate::orchestration::cognitive_economics::{
    CognitiveBudget, CognitiveMacroState, KnowledgeCapital,
    model_market_quotes, select_model_by_roi, ModelMarketQuote,
};
use crate::model::AgentModel;
use std::sync::Arc;

/// Wraps DSGE state for a single Agent session.
pub struct AgentEconomy {
    pub budget: CognitiveBudget,
    pub macro_state: CognitiveMacroState,
    pub knowledge: KnowledgeCapital,
    pub market_quotes: Vec<ModelMarketQuote>,
}

impl AgentEconomy {
    /// Create for a new session with 100k default budget.
    pub fn new(complexity: f64) -> Self {
        Self {
            budget: CognitiveBudget::new(100_000, complexity),
            macro_state: CognitiveMacroState::new(),
            knowledge: KnowledgeCapital::new(),
            market_quotes: model_market_quotes(),
        }
    }

    /// [1] Record token usage and update shadow price.
    pub fn record_token_usage(&mut self, input_tokens: u64, output_tokens: u64) {
        let total = input_tokens + output_tokens;
        self.budget.spend(total);
        self.macro_state.token_reserves = self.budget.remaining() as f64;
        self.knowledge.invest_research(total as f64, 0.005);
    }

    /// [2] Update context inflation from current context size.
    pub fn update_inflation(&mut self, context_tokens: u64, optimal_context: u64) {
        self.macro_state.update_inflation(context_tokens, optimal_context);
        self.macro_state.average_entropy = self.macro_state.average_entropy * 0.9 + self.macro_state.context_inflation.max(0.0) * 0.1;
        self.macro_state.detect_crisis(0.1, self.budget.llm_gated);
    }

    /// [3] Shadow price directive for system prompt.
    pub fn shadow_directive(&self) -> String {
        self.budget.shadow_directive()
    }

    /// [4] Select best model by ROI.
    pub async fn select_model(&self, task: &str) -> Option<Arc<dyn AgentModel>> {
        let est_input = 2000;
        let est_output = (self.budget.complexity * 500.0) as u64;

        let best = select_model_by_roi(&self.market_quotes, est_input, est_output.max(100), None);

        best.and_then(|q| crate::providers::registry::ModelRegistry::get_model(&q.model_id))
    }
}
