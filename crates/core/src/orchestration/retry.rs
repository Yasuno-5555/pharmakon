//! Intelligent Retry Policy — failure classification and differentiated responses.
//!
//! Problem: naive retry loops ("failure → retry → retry → retry") burn tokens,
//! pollute context, and never resolve the root cause.
//!
//! Solution: classify every failure into one of four categories
//! and apply the appropriate strategy per category.
//!
//! Categories:
//!   Transient  → exponential backoff, auto-retry
//!   Strategic  → switch approach (MCTS suggestion, RLFC), then retry
//!   Escalation → halt and ask human
//!   Terminal   → abort immediately, don't retry

use std::time::Duration;

// --- Failure Classification ---

/// The root cause category of a tool execution failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureClass {
    /// Temporary issue that resolves on retry.
    /// Examples: rate limits (429), network timeout, transient I/O error.
    Transient,

    /// Wrong approach — the tool executed successfully but the result
    /// doesn't advance the task. Needs strategy change before retry.
    /// Examples: wrong tool chosen, incorrect parameters, logic error.
    Strategic,

    /// Cannot proceed without human input.
    /// Examples: insufficient permissions, ambiguous requirements, approval denied.
    Escalation,

    /// Fundamentally impossible — retrying will never work.
    /// Examples: file doesn't exist, syntax error in generated code, dead URL.
    Terminal,
}

/// Classify a tool failure based on the error message and context.
pub fn classify_failure(error: &str, tool_name: &str, is_consecutive: bool) -> FailureClass {
    let lower = error.to_lowercase();

    // --- Transient signals ---
    if lower.contains("429")
        || lower.contains("rate limit")
        || lower.contains("too many requests")
        || lower.contains("quota")
        || lower.contains("timeout")
        || lower.contains("connection reset")
        || lower.contains("temporarily unavailable")
        || lower.contains("try again")
        || lower.contains("503")
        || lower.contains("502")
    {
        return FailureClass::Transient;
    }

    // --- Terminal signals ---
    if lower.contains("not found")
        || lower.contains("no such file")
        || lower.contains("does not exist")
        || lower.contains("cannot find")
        || lower.contains("404")
        || lower.contains("invalid syntax")
        || lower.contains("parse error")
        || lower.contains("permission denied")
        || lower.contains("access denied")
        || lower.contains("403")
    {
        return FailureClass::Terminal;
    }

    // --- Escalation signals ---
    if lower.contains("authentication")
        || lower.contains("unauthorized")
        || lower.contains("login required")
        || lower.contains("api key")
        || lower.contains("denied by policy")
        || lower.contains("denied by user")
        || lower.contains("requires approval")
        || lower.contains("ambiguous")
        || lower.contains("unclear")
        || (lower.contains("permission") && !lower.contains("not found"))
        || (lower.contains("access denied") && !lower.contains("file"))
    {
        return FailureClass::Escalation;
    }

    // --- Strategic signals (consecutive failures on the same tool) ---
    if is_consecutive && lower.contains("error") {
        return FailureClass::Strategic;
    }

    // The tool failed but we can't confidently classify → be conservative
    if is_consecutive {
        FailureClass::Strategic
    } else {
        FailureClass::Transient // Assume transient on first failure
    }
}

// --- Retry Policy ---

/// Action to take after a failure is classified.
#[derive(Debug, Clone)]
pub enum RetryAction {
    /// Retry after waiting (exponential backoff).
    Backoff { delay: Duration, max_retries: u32 },

    /// Change the strategy before retrying.
    /// The agent should: 1) reflect, 2) try a different tool/approach, 3) retry.
    SwitchStrategy { reason: String },

    /// Escalate to human — stop the agent loop.
    AskHuman { reason: String },

    /// Abort immediately. Do not retry.
    Abort { reason: String },
}

/// Stateful retry tracker for a single tool or operation.
#[derive(Debug, Clone)]
pub struct RetryState {
    /// How many times we've retried this specific operation.
    pub attempt: u32,
    /// Maximum retries before escalation.
    pub max_retries: u32,
    /// Base backoff duration.
    pub base_delay: Duration,
    /// The tool being retried.
    pub tool_name: String,
    /// Last error message.
    pub last_error: Option<String>,
}

impl RetryState {
    pub fn new(tool_name: &str) -> Self {
        Self {
            attempt: 0,
            max_retries: 3,
            base_delay: Duration::from_secs(1),
            tool_name: tool_name.to_string(),
            last_error: None,
        }
    }

    /// Evaluate the current failure and return the appropriate action.
    pub fn evaluate(&mut self, error: &str) -> RetryAction {
        self.attempt += 1;
        self.last_error = Some(error.to_string());

        let consecutive = self.attempt > 1;
        let class = classify_failure(error, &self.tool_name, consecutive);

        match class {
            FailureClass::Transient => {
                if self.attempt >= self.max_retries {
                    RetryAction::AskHuman {
                        reason: format!(
                            "Persistent transient errors ({} attempts): {}",
                            self.attempt, error
                        ),
                    }
                } else {
                    // Exponential backoff: 1s, 2s, 4s, 8s...
                    let delay = self.base_delay * 2u32.pow(self.attempt.saturating_sub(1));
                    RetryAction::Backoff {
                        delay,
                        max_retries: self.max_retries,
                    }
                }
            }

            FailureClass::Strategic => {
                if self.attempt >= 2 {
                    // Two strategic failures → escalate
                    RetryAction::AskHuman {
                        reason: format!(
                            "Repeated strategic failure ({} attempts). Current approach is not working. Error: {}",
                            self.attempt, error
                        ),
                    }
                } else {
                    RetryAction::SwitchStrategy {
                        reason: format!(
                            "Strategic failure detected: {}. Consider using a different tool or approach.",
                            error
                        ),
                    }
                }
            }

            FailureClass::Escalation => {
                RetryAction::AskHuman {
                    reason: format!("Escalation required: {}", error),
                }
            }

            FailureClass::Terminal => {
                RetryAction::Abort {
                    reason: format!("Terminal failure — retrying will not help: {}", error),
                }
            }
        }
    }

    /// Reset the retry state for a new operation.
    pub fn reset(&mut self) {
        self.attempt = 0;
        self.last_error = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_rate_limit_as_transient() {
        assert_eq!(
            classify_failure("HTTP 429: Too Many Requests", "shell", false),
            FailureClass::Transient
        );
    }

    #[test]
    fn test_classify_not_found_as_terminal() {
        assert_eq!(
            classify_failure("File not found: Cargo.toml", "read_file", false),
            FailureClass::Terminal
        );
    }

    #[test]
    fn test_classify_permission_as_terminal() {
        assert_eq!(
            classify_failure("Permission denied", "shell", false),
            FailureClass::Terminal
        );
    }

    #[test]
    fn test_consecutive_error_is_strategic() {
        assert_eq!(
            classify_failure("Compilation error: type mismatch", "cargo_check", true),
            FailureClass::Strategic
        );
    }

    #[test]
    fn test_retry_state_transient_backoff() {
        let mut state = RetryState::new("web_fetch");
        let action = state.evaluate("HTTP 429: Too Many Requests");
        match action {
            RetryAction::Backoff { delay, .. } => assert!(delay >= Duration::from_secs(1)),
            other => panic!("Expected Backoff, got {:?}", other),
        }
    }

    #[test]
    fn test_retry_state_terminal_abort() {
        let mut state = RetryState::new("read_file");
        let action = state.evaluate("No such file: does_not_exist.rs");
        match action {
            RetryAction::Abort { .. } => (),
            other => panic!("Expected Abort, got {:?}", other),
        }
    }

    #[test]
    fn test_retry_state_exhausted_transient() {
        let mut state = RetryState::new("web_fetch");
        state.attempt = 3; // On 4th attempt → exceeds max_retries
        let action = state.evaluate("Rate limit exceeded");
        match action {
            RetryAction::AskHuman { .. } => (),
            other => panic!("Expected Escalation, got {:?}", other),
        }
    }
}
