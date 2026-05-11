
use std::collections::VecDeque;
use std::time::Duration;

// --- 1. Core Budget and Policy Definitions ---

/// Defines the execution resources allocated to a task.
#[derive(Debug, Clone)]
pub struct ExecutionBudget {
    /// A hard safety net to prevent absolute runaways.
    pub hard_max_wall_time: Duration,
    /// The specific policy that determines when a task should terminate.
    pub policy: TerminationPolicy,
}

/// Defines the rules for when an agent's execution loop should stop.
#[derive(Debug, Clone)]
pub enum TerminationPolicy {
    /// For simple, predictable tasks.
    FixedIterations(usize),
    /// For complex tasks where progress must be monitored.
    ProgressBased {
        /// How many consecutive non-progressing iterations are allowed.
        stall_threshold: usize,
    },
}

// --- 2. Progress Tracking ---

/// A snapshot of the agent's state at a single point in time to measure progress.
#[derive(Clone, Debug, PartialEq)]
pub struct IterationSnapshot {
    pub tool_calls: usize,
    pub successful_tool_calls: usize,
    pub last_tool_call_args: Option<String>,
    // Future metrics
    // pub compiler_errors: usize,
    // pub new_facts_learned: usize,
}

impl Default for IterationSnapshot {
    fn default() -> Self {
        Self::new()
    }
}

impl IterationSnapshot {
    pub fn new() -> Self {
        Self {
            tool_calls: 0,
            successful_tool_calls: 0,
            last_tool_call_args: None,
        }
    }
}

/// Monitors the agent's progress over a series of iterations to detect stalls.
pub struct ProgressTracker {
    history: VecDeque<IterationSnapshot>,
    stall_count: usize,
    stall_threshold: usize,
    max_history: usize,
}

#[derive(Debug, PartialEq)]
pub enum TerminationSignal {
    Continue,
    EntropyOverflow { score: f32 },
    Stalled,
    LoopDetected,
    PolicyFinished,
}

impl ProgressTracker {
    pub fn new(policy: &TerminationPolicy) -> Self {
        let stall_threshold = match policy {
            TerminationPolicy::ProgressBased { stall_threshold } => *stall_threshold,
            _ => 3, // Default for other policies
        };
        Self {
            history: VecDeque::with_capacity(10),
            stall_count: 0,
            stall_threshold,
            max_history: 10,
        }
    }

    /// Check entropy and return a termination signal if the score exceeds the threshold.
    /// This integrates the rich entropy from EventLog into the progress tracking pipeline.
    pub fn check_entropy(&self, entropy: f32, threshold: f32) -> TerminationSignal {
        if entropy > threshold {
            TerminationSignal::EntropyOverflow { score: entropy }
        } else {
            TerminationSignal::Continue
        }
    }

    /// Records the latest state snapshot and returns a signal if termination is needed.
    pub fn record(&mut self, snapshot: IterationSnapshot) -> TerminationSignal {
        // Simple loop detection: check for identical consecutive tool calls
        let mut loop_count: usize = 1;
        if let Some(args) = &snapshot.last_tool_call_args {
            for prev in self.history.iter().rev() {
                if prev.last_tool_call_args.as_ref() == Some(args) {
                    loop_count += 1;
                } else {
                    break;
                }
            }
        }
        if loop_count >= 3 {
            log::warn!("Loop detected: Same tool call args repeated {} times.", loop_count);
            return TerminationSignal::LoopDetected;
        }
        
        let progress = self.measure_delta(&snapshot);

        if progress < 0.01 { // Using a small epsilon for progress
            self.stall_count += 1;
            log::warn!("Stall count increased to {}/{}", self.stall_count, self.stall_threshold);

        } else {
            self.stall_count = 0; // Progress was made, reset the counter
            log::info!("Progress detected (score: {:.2}), stall count reset.", progress);
        }

        self.history.push_back(snapshot);
        if self.history.len() > self.max_history {
            self.history.pop_front();
        }

        if self.stall_count >= self.stall_threshold {
            TerminationSignal::Stalled
        } else {
            TerminationSignal::Continue
        }
    }

    /// Measures the progress between the last snapshot and the new one.
    /// Returns a score, where > 0 means progress.
    fn measure_delta(&self, current: &IterationSnapshot) -> f32 {
        let Some(prev) = self.history.back() else {
            // First iteration, so any action is progress.
            return 1.0;
        };

        

        // More metrics can be added here and weighted.
        // For now, any successful tool call is progress.
        (current.successful_tool_calls > prev.successful_tool_calls) as i32 as f32
    }
}

// --- 3. Task Classification (Placeholder) ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskComplexity {
    Simple,   // e.g., one-shot command
    Standard, // e.g., file modification and check
    Deep,     // e.g., complex debugging, swarm operations
}

/// Estimates an appropriate budget for a given task complexity.
pub fn estimate_budget(complexity: TaskComplexity) -> ExecutionBudget {
    match complexity {
        TaskComplexity::Simple => ExecutionBudget {
            hard_max_wall_time: Duration::from_secs(120),
            policy: TerminationPolicy::FixedIterations(8),
        },
        TaskComplexity::Standard => ExecutionBudget {
            hard_max_wall_time: Duration::from_secs(600),
            policy: TerminationPolicy::ProgressBased {
                stall_threshold: 4, // A bit more lenient
            },
        },
        TaskComplexity::Deep => ExecutionBudget {
            hard_max_wall_time: Duration::from_secs(1800), // 30 minutes
            policy: TerminationPolicy::ProgressBased {
                stall_threshold: 10, // Very lenient for research tasks
            },
        },
    }
}