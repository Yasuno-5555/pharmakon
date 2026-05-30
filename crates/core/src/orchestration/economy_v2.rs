//! Token Economy v2 — Bank of Pharmakon (Phase 8)
//!
//! Cognitive ROI-based budget allocation with multi-layer reserve system.
//! Investment tracking for R&D spend (crystallization, indexing, Dream Mode).
//!
//! Layers:
//!   operating_budget → Day-to-day task execution
//!   emergency_reserve → API outage / rate limit survival
//!   burst_quota       → High-priority surge capacity
//!   research_budget   → Crystallization, indexing, Dream Mode

use serde::{Deserialize, Serialize};

/// Cognitive ROI: capability gain per unit cost.
/// Higher is better. Negative means net loss.
pub fn cognitive_roi(
    estimated_quality_gain: f64, // 0.0–1.0: expected task quality improvement
    token_cost: u64,
    latency_ms: u64,
) -> f64 {
    let cost = token_cost as f64 * 0.0001   // token cost weight
             + latency_ms as f64 * 0.0005; // latency weight (ms → normalized)
    if cost <= 0.0 {
        return estimated_quality_gain;
    }
    estimated_quality_gain / cost
}

/// Three-layer token reserve system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenReserve {
    /// Total tokens available this period.
    pub total_balance: u64,
    /// Operating budget (85% by default).
    pub operating_ratio: f64,
    /// Emergency reserve (10%).
    pub emergency_ratio: f64,
    /// Burst quota (5%).
    pub burst_ratio: f64,
}

impl TokenReserve {
    pub fn new(total_balance: u64) -> Self {
        Self {
            total_balance,
            operating_ratio: 0.85,
            emergency_ratio: 0.10,
            burst_ratio: 0.05,
        }
    }

    pub fn operating_budget(&self) -> u64 {
        (self.total_balance as f64 * self.operating_ratio) as u64
    }

    pub fn emergency_reserve(&self) -> u64 {
        (self.total_balance as f64 * self.emergency_ratio) as u64
    }

    pub fn burst_quota(&self) -> u64 {
        (self.total_balance as f64 * self.burst_ratio) as u64
    }

    /// Allocate tokens from operating budget.
    /// Returns the granted amount (may be less than requested).
    pub fn allocate(&mut self, requested: u64, priority: f64) -> u64 {
        let available = self.operating_budget();
        let granted = (requested as f64 * priority.min(1.0)) as u64;
        let actual = granted.min(available);
        if actual > 0 {
            self.total_balance = self.total_balance.saturating_sub(actual);
        }
        actual
    }

    /// Tap emergency reserve (during API outage).
    pub fn emergency_withdraw(&mut self, requested: u64) -> u64 {
        let available = self.emergency_reserve();
        let actual = requested.min(available);
        self.total_balance = self.total_balance.saturating_sub(actual);
        actual
    }

    /// Use burst quota for high-priority surge.
    pub fn burst_withdraw(&mut self, requested: u64) -> u64 {
        let available = self.burst_quota();
        let actual = requested.min(available);
        self.total_balance = self.total_balance.saturating_sub(actual);
        actual
    }
}

/// Tracks R&D investments and their future payoff estimates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchInvestment {
    pub category: ResearchCategory,
    pub tokens_invested: u64,
    pub estimated_future_savings: u64,
    pub roi: f64, // savings / investment
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ResearchCategory {
    SkillCrystallization,
    DreamMode,
    CodexIndexing,
    PatternMining,
    Benchmarking,
}

impl std::fmt::Display for ResearchCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResearchCategory::SkillCrystallization => write!(f, "SkillCrystallization"),
            ResearchCategory::DreamMode => write!(f, "DreamMode"),
            ResearchCategory::CodexIndexing => write!(f, "CodexIndexing"),
            ResearchCategory::PatternMining => write!(f, "PatternMining"),
            ResearchCategory::Benchmarking => write!(f, "Benchmarking"),
        }
    }
}

/// Bank of Pharmakon — the central economic controller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankOfPharmakon {
    pub reserve: TokenReserve,
    pub investments: Vec<ResearchInvestment>,
    pub roi_history: Vec<(chrono::DateTime<chrono::Utc>, f64)>, // timestamp → ROI
    pub total_tokens_spent: u64,
    pub total_tokens_saved: u64, // cumulative savings from investments
}

impl BankOfPharmakon {
    pub fn new(total_balance: u64) -> Self {
        Self {
            reserve: TokenReserve::new(total_balance),
            investments: Vec::new(),
            roi_history: Vec::new(),
            total_tokens_spent: 0,
            total_tokens_saved: 0,
        }
    }

    /// Record a task's token consumption and update ROI tracking.
    pub fn record_task(&mut self, tokens: u64, quality_gain: f64, latency_ms: u64) {
        self.total_tokens_spent += tokens;
        let roi = cognitive_roi(quality_gain, tokens, latency_ms);
        self.roi_history.push((chrono::Utc::now(), roi));
        // Keep last 100 entries
        if self.roi_history.len() > 100 {
            self.roi_history.remove(0);
        }
    }

    /// Record an R&D investment.
    pub fn invest(&mut self, category: ResearchCategory, tokens: u64, estimated_savings: u64) {
        let roi = if tokens > 0 {
            estimated_savings as f64 / tokens as f64
        } else {
            0.0
        };
        self.investments.push(ResearchInvestment {
            category,
            tokens_invested: tokens,
            estimated_future_savings: estimated_savings,
            roi,
            completed_at: None,
        });
        self.total_tokens_saved += estimated_savings;
    }

    /// Mark an investment as completed (e.g., crystallization finished).
    pub fn complete_investment(&mut self, category: ResearchCategory) {
        for inv in &mut self.investments {
            if inv.category == category && inv.completed_at.is_none() {
                inv.completed_at = Some(chrono::Utc::now());
            }
        }
    }

    /// Average Cognitive ROI over recent history.
    pub fn average_roi(&self) -> f64 {
        if self.roi_history.is_empty() {
            return 0.0;
        }
        let sum: f64 = self.roi_history.iter().map(|(_, r)| r).sum();
        sum / self.roi_history.len() as f64
    }

    /// Net token savings (saved − invested).
    pub fn net_savings(&self) -> i64 {
        let invested: u64 = self
            .investments
            .iter()
            .filter(|i| i.completed_at.is_some())
            .map(|i| i.tokens_invested)
            .sum();
        self.total_tokens_saved as i64 - invested as i64
    }

    /// Check if a task is worth executing based on ROI threshold.
    pub fn should_execute(
        &self,
        estimated_tokens: u64,
        estimated_quality: f64,
        estimated_latency: u64,
    ) -> bool {
        let roi = cognitive_roi(estimated_quality, estimated_tokens, estimated_latency);
        let threshold = self.average_roi() * 0.5; // half of average ROI
        roi > threshold || estimated_quality > 0.8 // always run high-quality tasks
    }

    /// Status dashboard string.
    pub fn status(&self) -> String {
        format!(
            "💊 Bank of Pharmakon\n\
             ├─ Balance: {} tokens\n\
             │  ├─ Operating: {} ({}%)\n\
             │  ├─ Emergency: {} ({}%)\n\
             │  └─ Burst: {} ({}%)\n\
             ├─ Total Spent: {} tokens\n\
             ├─ Total Saved: {} tokens (net: {})\n\
             ├─ Avg ROI: {:.2}\n\
             ├─ Investments: {}\n\
             └─ R&D Active: {}",
            self.reserve.total_balance,
            self.reserve.operating_budget(),
            (self.reserve.operating_ratio * 100.0) as u32,
            self.reserve.emergency_reserve(),
            (self.reserve.emergency_ratio * 100.0) as u32,
            self.reserve.burst_quota(),
            (self.reserve.burst_ratio * 100.0) as u32,
            self.total_tokens_spent,
            self.total_tokens_saved,
            self.net_savings(),
            self.average_roi(),
            self.investments.len(),
            self.investments
                .iter()
                .filter(|i| i.completed_at.is_none())
                .count(),
        )
    }
}

impl Default for BankOfPharmakon {
    fn default() -> Self {
        Self::new(100_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cognitive_roi() {
        let high_roi = cognitive_roi(0.9, 500, 200);
        let low_roi = cognitive_roi(0.2, 5000, 5000);
        assert!(
            high_roi > low_roi,
            "High quality/low cost should have higher ROI"
        );
    }

    #[test]
    fn test_token_reserve_allocation() {
        let mut reserve = TokenReserve::new(10000);
        let granted = reserve.allocate(1000, 1.0);
        assert_eq!(granted, 1000);
        assert_eq!(reserve.total_balance, 9000);
    }

    #[test]
    fn test_emergency_withdraw() {
        let mut reserve = TokenReserve::new(10000);
        let granted = reserve.emergency_withdraw(500);
        assert_eq!(granted, 500);
        assert!(reserve.total_balance < 10000);
    }

    #[test]
    fn test_bank_investment_tracking() {
        let mut bank = BankOfPharmakon::new(100000);
        bank.invest(ResearchCategory::SkillCrystallization, 5000, 20000);
        assert_eq!(bank.investments.len(), 1);
        assert_eq!(bank.total_tokens_saved, 20000);
    }

    #[test]
    fn test_bank_roi_history() {
        let mut bank = BankOfPharmakon::new(100000);
        bank.record_task(500, 0.8, 200);
        bank.record_task(1000, 0.5, 1000);
        assert_eq!(bank.roi_history.len(), 2);
        assert!(bank.average_roi() > 0.0);
    }
}
