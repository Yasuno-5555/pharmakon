//! Cognitive Scheduler — complexity-aware task budgeting with LLM classification.
//!
//! Replaces the simple keyword-match approach in `budget.rs` with:
//! 1. Primary: LLM-based classification (when a model is available)
//! 2. Fallback: enriched heuristic matching (extended keyword set)
//!
//! Also introduces the `ManagedTask` struct with cognitive economics fields
//! (`priority_score`, `expected_information_gain`, `retry_cost`) to support
//! future multi-task prioritization and cost-benefit scheduling.

use crate::orchestration::budget::{self, ExecutionBudget, TaskComplexity};
use crate::model::{AgentModel, CompletionRequest, Message, MessageContent};
use std::sync::Arc;

// --- Managed Task ---

/// A task enriched with cognitive economics metadata for intelligent scheduling.
#[derive(Debug, Clone)]
pub struct ManagedTask {
    /// The task description / user message.
    pub description: String,

    /// Inferred task complexity from classification.
    pub complexity: TaskComplexity,

    /// Execution budget derived from complexity.
    pub budget: ExecutionBudget,

    /// Cognitive economics fields (future: multi-task prioritization).
    /// How urgent/important this task is (0.0–1.0).
    pub priority_score: f32,

    /// Expected information gain from executing this task (0.0–1.0).
    /// High: debugging an unknown bug. Low: routine formatting.
    pub expected_information_gain: f32,

    /// Estimated cost of retrying this task if it fails.
    /// High: expensive API calls, irreversible side effects.
    /// Low: quick read-only operations.
    pub retry_cost: f32,
}

impl ManagedTask {
    /// Create a new managed task with default cognitive economics.
    pub fn new(description: &str, complexity: TaskComplexity, budget: ExecutionBudget) -> Self {
        let (priority, gain, cost) = match complexity {
            TaskComplexity::Simple => (0.3, 0.2, 0.1),
            TaskComplexity::Standard => (0.6, 0.5, 0.4),
            TaskComplexity::Deep => (0.9, 0.8, 0.7),
        };

        Self {
            description: description.to_string(),
            complexity,
            budget,
            priority_score: priority,
            expected_information_gain: gain,
            retry_cost: cost,
        }
    }

    /// Suspend this task: save state for later resumption.
    /// Records a snapshot of the task description and complexity to the event log.
    pub async fn suspend(
        &self,
        event_log: &crate::event_log::EventLog,
        session_id: &str,
    ) -> anyhow::Result<u64> {
        let event_id = event_log
            .append(
                session_id,
                crate::event_log::EventKind::SessionEvent {
                    action: "suspended".to_string(),
                    detail: format!(
                        "Task suspended: {} (complexity: {:?}, priority: {:.2})",
                        self.description, self.complexity, self.priority_score
                    ),
                },
            )
            .await;

        log::info!(
            "ManagedTask suspended: '{}' (event_id={}, priority={:.2})",
            self.description,
            event_id,
            self.priority_score
        );

        Ok(event_id)
    }

    /// Resume this task from a suspended state.
    /// Adjusts the retry cost upward since resumption implies prior failure/abandonment.
    pub fn resume(&mut self) {
        self.retry_cost = (self.retry_cost * 1.3).min(1.0);
        log::info!(
            "ManagedTask resumed: '{}' (retry_cost adjusted to {:.2})",
            self.description,
            self.retry_cost
        );
    }

    /// Check if this task is worth retrying based on cognitive economics.
    /// Returns true if the expected information gain outweighs the retry cost.
    pub fn is_worth_retrying(&self) -> bool {
        let net_benefit = self.expected_information_gain - self.retry_cost;
        net_benefit > 0.0
    }
}

/// Serializable snapshot of a ManagedTask for disk persistence.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TaskSnapshot {
    pub description: String,
    pub complexity: String, // serialized as string
    pub priority_score: f32,
    pub expected_information_gain: f32,
    pub retry_cost: f32,
    pub suspended_at: String,
    pub budget_wall_time_secs: u64,
    pub budget_stall_threshold: usize,
}

impl From<&ManagedTask> for TaskSnapshot {
    fn from(task: &ManagedTask) -> Self {
        Self {
            description: task.description.clone(),
            complexity: format!("{:?}", task.complexity),
            priority_score: task.priority_score,
            expected_information_gain: task.expected_information_gain,
            retry_cost: task.retry_cost,
            suspended_at: chrono::Utc::now().to_rfc3339(),
            budget_wall_time_secs: task.budget.hard_max_wall_time.as_secs(),
            budget_stall_threshold: match &task.budget.policy {
                crate::orchestration::budget::TerminationPolicy::ProgressBased { stall_threshold } => *stall_threshold,
                _ => 3,
            },
        }
    }
}

impl TaskSnapshot {
    /// Reconstruct a ManagedTask from a snapshot.
    pub fn to_managed_task(&self) -> ManagedTask {
        let complexity = match self.complexity.as_str() {
            "Deep" => TaskComplexity::Deep,
            "Standard" => TaskComplexity::Standard,
            _ => TaskComplexity::Simple,
        };
        let budget = budget::ExecutionBudget {
            hard_max_wall_time: std::time::Duration::from_secs(self.budget_wall_time_secs),
            policy: budget::TerminationPolicy::ProgressBased {
                stall_threshold: self.budget_stall_threshold,
            },
        };
        let mut task = ManagedTask::new(&self.description, complexity, budget);
        task.priority_score = self.priority_score;
        task.expected_information_gain = self.expected_information_gain;
        task.retry_cost = self.retry_cost;
        task
    }
}

// --- LLM-based Classification ---

/// Classify a task description into a complexity tier.
///
/// Uses heuristic first to save API cost. LLM classification only for ambiguous
/// short tasks where the heuristic can't distinguish Simple from Standard.
pub async fn classify_task_complexity(
    description: &str,
    model: Option<&Arc<dyn AgentModel>>,
) -> TaskComplexity {
    // Heuristic first — free, catches most cases
    let heuristic = heuristic_classify(description);

    // Only consult LLM for ambiguous short tasks (word_count < 3 or len < 12)
    // where the heuristic defaults to Simple but the task might be Standard.
    // This eliminates ~90% of LLM classification calls.
    let trimmed = description.trim().to_lowercase();
    let word_count = trimmed.split_whitespace().count();
    let is_ambiguous = word_count < 3 || trimmed.len() < 12;

    if is_ambiguous && heuristic == TaskComplexity::Simple {
        if let Some(model) = model {
            if let Some(llm_result) = llm_classify(description, model).await {
                return llm_result;
            }
        }
    }

    heuristic
}

/// LLM-based classification prompt.
async fn llm_classify(
    description: &str,
    model: &Arc<dyn AgentModel>,
) -> Option<TaskComplexity> {
    let prompt = format!(
        "Classify the following task into exactly one category: Simple, Standard, or Deep.\n\
         - Simple: one-shot commands, quick lookups, trivial questions.\n\
         - Standard: file modifications, code generation, testing, bug fixes.\n\
         - Deep: complex refactoring, architecture redesign, multi-step debugging, \
           system migration, swarm operations, security audits.\n\
         \n\
         Task: \"{}\"\n\
         \n\
         Reply with exactly one word: Simple, Standard, or Deep.",
        description
    );

    let request = CompletionRequest {
        messages: vec![Message {
            role: "user".to_string(),
            content: Some(MessageContent::Text(prompt)),
            ..Default::default()
        }],
        temperature: Some(0.0),
        max_tokens: Some(64), // bumped for thinking models (Gemini Flash internally consumes some tokens for thought)
        tools: None,
    };

    let response = model.complete(request).await.ok()?;
    let text = response.content.as_ref()?.as_text()?.trim().to_lowercase();

    if text.contains("deep") {
        Some(TaskComplexity::Deep)
    } else if text.contains("standard") {
        Some(TaskComplexity::Standard)
    } else if text.contains("simple") {
        Some(TaskComplexity::Simple)
    } else {
        None // Unparseable — fall through to heuristic
    }
}

/// Enriched heuristic classification with extended keyword sets.
fn heuristic_classify(description: &str) -> TaskComplexity {
    let lower = description.to_lowercase();
    let trimmed = lower.trim();

    // If message is very short (e.g., < 3 words or < 12 chars), default to Simple
    // unless it explicitly contains strong architectural keywords like "rewrite" or "refactor"
    let word_count = trimmed.split_whitespace().count();
    let is_short = word_count < 3 || trimmed.len() < 12;

    // Deep: complex, multi-step, or architectural tasks
    const DEEP_KEYWORDS: &[&str] = &[
        "rewrite", "redesign", "migrate", "architecture", "refactor",
        "security audit", "multi-step", "complex", "swarm",
        "decompose", "rearchitect", "overhaul", "restructure",
        "concurrency", "async migration", "database migration",
    ];

    if DEEP_KEYWORDS.iter().any(|k| trimmed.contains(k)) {
        return TaskComplexity::Deep;
    }

    if is_short {
        return TaskComplexity::Simple;
    }

    // Standard: file/code modifications
    const STANDARD_KEYWORDS: &[&str] = &[
        "implement", "debug", "test", "fix", "add", "create",
        "modify", "update", "change", "build", "compile",
        "optimize", "improve", "review", "analyze",
    ];

    if STANDARD_KEYWORDS.iter().any(|k| trimmed.contains(k)) {
        return TaskComplexity::Standard;
    }

    // Default: Simple
    TaskComplexity::Simple
}

/// Convenience: classify and create a fully managed task in one call.
pub async fn manage_task(
    description: &str,
    model: Option<&Arc<dyn AgentModel>>,
) -> ManagedTask {
    let complexity = classify_task_complexity(description, model).await;
    let budget = budget::estimate_budget(complexity.clone());
    ManagedTask::new(description, complexity, budget)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heuristic_deep() {
        assert_eq!(
            heuristic_classify("rewrite the auth system"),
            TaskComplexity::Deep
        );
        assert_eq!(
            heuristic_classify("redesign the database schema"),
            TaskComplexity::Deep
        );
        assert_eq!(
            heuristic_classify("migrate from REST to GraphQL"),
            TaskComplexity::Deep
        );
    }

    #[test]
    fn test_heuristic_standard() {
        assert_eq!(
            heuristic_classify("implement a login endpoint"),
            TaskComplexity::Standard
        );
        assert_eq!(
            heuristic_classify("fix the null pointer bug"),
            TaskComplexity::Standard
        );
        assert_eq!(
            heuristic_classify("add unit tests for the parser"),
            TaskComplexity::Standard
        );
    }

    #[test]
    fn test_heuristic_simple() {
        assert_eq!(
            heuristic_classify("what time is it"),
            TaskComplexity::Simple
        );
        assert_eq!(
            heuristic_classify("show me the weather"),
            TaskComplexity::Simple
        );
        assert_eq!(
            heuristic_classify("read the config file"),
            TaskComplexity::Simple
        );
    }

    #[test]
    fn test_managed_task_cognitive_economics() {
        let task = ManagedTask::new(
            "rewrite the auth system",
            TaskComplexity::Deep,
            budget::estimate_budget(TaskComplexity::Deep),
        );

        assert!(task.priority_score > 0.8);
        assert!(task.expected_information_gain > 0.7);
        assert!(task.retry_cost > 0.5);
    }
}