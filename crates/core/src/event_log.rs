//! Append-only event log for agent state reconstruction and rollback.
//!
//! This module provides the foundation for:
//! - Deterministic replay of agent sessions
//! - Atomic rollback to any prior event ID
//! - Entropy monitoring via event stream analysis
//! - Forensic debugging of multi-step tool chains

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::Mutex;

/// A single immutable event in the agent's execution history.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentEvent {
    pub id: u64,
    pub timestamp: DateTime<Utc>,
    pub session_id: String,
    pub kind: EventKind,
}

/// Typed event kinds for structured reconstruction.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    /// A tool was invoked by the agent.
    ToolCalled {
        tool: String,
        args_hash: String,
    },
    /// A tool returned a result.
    ToolResult {
        tool: String,
        success: bool,
        latency_ms: u64,
        output_hash: String,
    },
    /// A file was mutated on disk.
    /// References SnapshotStore IDs — never inline content.
    /// EventLog = causal history, SnapshotStore = state materialization.
    FileMutated {
        path: String,
        /// SnapshotStore ID for the file state before mutation
        snapshot_before_id: String,
        /// SnapshotStore ID for the file state after mutation
        snapshot_after_id: String,
    },
    /// Token/cost budget was consumed.
    BudgetConsumed {
        tokens: u64,
        cost_usd: f64,
    },
    /// A sub-agent was spawned.
    SubAgentSpawned {
        child_session: String,
        task: String,
        role: String,
    },
    /// Entropy exceeded the alert threshold.
    EntropyAlert {
        score: f32,
        pattern: String,
    },
    /// An iteration of the decision loop completed.
    IterationCompleted {
        iteration: usize,
        progress_delta: f32,
        entropy: f32,
    },
    /// The agent emitted a thought.
    ThoughtEmitted {
        content_hash: String,
    },
    /// Category activation changed.
    CategoryActivated {
        category: String,
    },
    /// Session lifecycle event.
    SessionEvent {
        action: String, // "started", "completed", "suspended", "failed"
        detail: String,
    },
}

/// Append-only event log with optional disk persistence.
pub struct EventLog {
    events: Mutex<Vec<AgentEvent>>,
    next_id: AtomicU64,
    persist_path: Option<PathBuf>,
}

impl EventLog {
    /// Create a new event log with optional JSONL persistence path.
    pub fn new(persist_path: Option<PathBuf>) -> Self {
        // Ensure persistence directory exists
        if let Some(ref path) = persist_path {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
        }

        Self {
            events: Mutex::new(Vec::new()),
            next_id: AtomicU64::new(1),
            persist_path,
        }
    }

    /// Append an event and return its monotonic ID.
    pub async fn append(&self, session_id: &str, kind: EventKind) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let event = AgentEvent {
            id,
            timestamp: Utc::now(),
            session_id: session_id.to_string(),
            kind,
        };

        // Persist to JSONL (best-effort, non-blocking)
        if let Some(ref path) = self.persist_path {
            if let Ok(line) = serde_json::to_string(&event) {
                use std::io::Write;
                if let Ok(mut file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                {
                    let _ = writeln!(file, "{}", line);
                }
            }
        }

        self.events.lock().await.push(event);
        id
    }

    /// Get all events after a given ID (for rollback computation).
    pub async fn events_since(&self, after_id: u64) -> Vec<AgentEvent> {
        self.events
            .lock()
            .await
            .iter()
            .filter(|e| e.id > after_id)
            .cloned()
            .collect()
    }

    /// Get the last N events (for entropy monitoring).
    pub async fn last_n(&self, n: usize) -> Vec<AgentEvent> {
        let events = self.events.lock().await;
        events.iter().rev().take(n).cloned().collect()
    }

    /// Get the current event count.
    pub async fn len(&self) -> usize {
        self.events.lock().await.len()
    }

    /// Get the last event ID (for checkpoint/rollback reference).
    pub fn current_id(&self) -> u64 {
        self.next_id.load(Ordering::SeqCst).saturating_sub(1)
    }

    /// Compute tool call entropy from recent events (for inline monitoring).
    /// Returns a score between 0.0 (no repetition) and 1.0 (pure loop).
    pub async fn recent_tool_entropy(&self, window: usize) -> f32 {
        let events = self.events.lock().await;
        let tool_calls: Vec<&str> = events
            .iter()
            .rev()
            .filter_map(|e| match &e.kind {
                EventKind::ToolCalled { tool, .. } => Some(tool.as_str()),
                _ => None,
            })
            .take(window)
            .collect();

        if tool_calls.len() < 3 {
            return 0.0;
        }

        // Bigram repetition analysis
        let bigrams: Vec<String> = tool_calls
            .windows(2)
            .map(|w| format!("{}→{}", w[0], w[1]))
            .collect();

        let unique: std::collections::HashSet<&String> = bigrams.iter().collect();
        let repetition = if bigrams.is_empty() {
            0.0
        } else {
            1.0 - (unique.len() as f32 / bigrams.len() as f32)
        };

        // Failure ratio from recent tool results
        let results: Vec<bool> = events
            .iter()
            .rev()
            .filter_map(|e| match &e.kind {
                EventKind::ToolResult { success, .. } => Some(*success),
                _ => None,
            })
            .take(window)
            .collect();

        let failure_ratio = if results.is_empty() {
            0.0
        } else {
            results.iter().filter(|&&s| !s).count() as f32 / results.len() as f32
        };

        // Stagnation detection: same output_hash appearing repeatedly
        // means tools succeed but produce identical results → no progress.
        // This catches the "grep → read → grep → read" loop where everything
        // technically succeeds but nothing advances.
        let output_hashes: Vec<&str> = events
            .iter()
            .rev()
            .filter_map(|e| match &e.kind {
                EventKind::ToolResult { output_hash, .. } => Some(output_hash.as_str()),
                _ => None,
            })
            .take(window)
            .collect();

        let stagnation = if output_hashes.len() < 2 {
            0.0
        } else {
            let unique_outputs: std::collections::HashSet<&&str> = output_hashes.iter().collect();
            1.0 - (unique_outputs.len() as f32 / output_hashes.len() as f32)
        };

        // Weights tuned for real-world agent behavior:
        // - Stagnation (0.4): most dangerous — agent looks productive but isn't
        // - Repetition (0.25): tool call pattern loops
        // - Failure (0.2): explicit errors
        // - Token drift (0.15): budget consumption without progress
        let token_drift = 0.0_f32; // TODO: integrate with BudgetConsumed events

        (stagnation * 0.4 + repetition * 0.25 + failure_ratio * 0.2 + token_drift * 0.15).min(1.0)
    }

    /// Get events for a specific session.
    pub async fn session_events(&self, session_id: &str) -> Vec<AgentEvent> {
        self.events
            .lock()
            .await
            .iter()
            .filter(|e| e.session_id == session_id)
            .cloned()
            .collect()
    }
}

/// Utility: compute a short hash for deduplication/comparison.
pub fn short_hash(input: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    input.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_log_append_and_query() {
        let log = EventLog::new(None);
        let id1 = log
            .append(
                "test-session",
                EventKind::ToolCalled {
                    tool: "grep".to_string(),
                    args_hash: "abc".to_string(),
                },
            )
            .await;
        let id2 = log
            .append(
                "test-session",
                EventKind::ToolResult {
                    tool: "grep".to_string(),
                    success: true,
                    latency_ms: 50,
                    output_hash: "def".to_string(),
                },
            )
            .await;

        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(log.len().await, 2);

        let since = log.events_since(1).await;
        assert_eq!(since.len(), 1);
        assert_eq!(since[0].id, 2);
    }

    #[tokio::test]
    async fn test_entropy_zero_for_diverse_tools() {
        let log = EventLog::new(None);
        for tool in &["grep", "read_file", "shell", "write_file", "lsp"] {
            log.append(
                "s",
                EventKind::ToolCalled {
                    tool: tool.to_string(),
                    args_hash: "x".to_string(),
                },
            )
            .await;
        }
        let entropy = log.recent_tool_entropy(10).await;
        assert!(entropy < 0.3, "Diverse tools should have low entropy: {}", entropy);
    }

    #[tokio::test]
    async fn test_entropy_high_for_loop() {
        let log = EventLog::new(None);
        for _ in 0..10 {
            log.append(
                "s",
                EventKind::ToolCalled {
                    tool: "apply_patch".to_string(),
                    args_hash: "x".to_string(),
                },
            )
            .await;
            log.append(
                "s",
                EventKind::ToolCalled {
                    tool: "cargo_check".to_string(),
                    args_hash: "y".to_string(),
                },
            )
            .await;
        }
        let entropy = log.recent_tool_entropy(20).await;
        assert!(entropy > 0.4, "Looping tools should have high entropy: {}", entropy);
    }
}
