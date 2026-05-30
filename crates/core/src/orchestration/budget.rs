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
    /// Success rate this iteration (0.0 to 1.0).
    pub tool_success_rate: f32,
    /// Average tool latency in ms for this iteration.
    pub avg_latency_ms: u64,
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
            tool_success_rate: 1.0,
            avg_latency_ms: 0,
        }
    }

    /// Convert to a low-dimensional feature vector for cosine similarity comparison.
    pub fn to_embedding(&self) -> IterationEmbedding {
        let success_rate = if self.tool_calls > 0 {
            self.successful_tool_calls as f32 / self.tool_calls as f32
        } else {
            1.0
        };
        let latency_norm = (self.avg_latency_ms as f32 / 1000.0).min(10.0);
        let count_norm = (self.tool_calls as f32 / 10.0).min(1.0);
        // Repetition indicator: 1.0 if args are identical to previous (set externally)
        let repetition = 0.0_f32;

        IterationEmbedding {
            features: [success_rate, latency_norm, count_norm, repetition],
        }
    }
}

/// Low-dimensional feature vector for comparing iteration patterns.
#[derive(Clone, Debug)]
pub struct IterationEmbedding {
    pub features: [f32; 4],
}

/// Compute cosine similarity between two feature vectors.
pub fn cosine_similarity(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a < 1e-6 || norm_b < 1e-6 {
        return 0.0;
    }
    (dot / (norm_a * norm_b)).clamp(-1.0, 1.0)
}

/// Monitors the agent's progress over a series of iterations to detect stalls.
pub struct ProgressTracker {
    history: VecDeque<IterationSnapshot>,
    /// Embedding history for cosine stagnation detection.
    embeddings: VecDeque<IterationEmbedding>,
    stall_count: usize,
    stall_threshold: usize,
    max_history: usize,
    /// Current entropy tier with hysteresis.
    current_tier: EntropyTier,
    /// Whether we have entered a higher tier this iteration (for edge-triggered responses).
    tier_escalated: bool,
    /// Consecutive iterations with high cosine similarity.
    cosine_stagnation_count: usize,
}

#[derive(Debug, PartialEq)]
pub enum TerminationSignal {
    Continue,
    EntropyOverflow { score: f32, tier: u8 },
    CosineStagnation { cos: f32 },
    Stalled,
    LoopDetected,
    PolicyFinished,
}

// --- Entropy Tier System (from LKO/objeta findings) ---

/// Entropy tier for multi-tier agent loop response.
/// Hysteresis prevents oscillation at tier boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EntropyTier {
    Normal,   // entropy ≤ T1
    Elevated, // T1 < entropy ≤ T2
    High,     // T2 < entropy ≤ T3
    Critical, // T3 < entropy ≤ T4
    Overflow, // entropy > T4 (hard terminate)
}

impl EntropyTier {
    /// Classify entropy into a tier (no hysteresis).
    pub fn classify(entropy: f32) -> Self {
        let t1 = read_entropy_tier_env("PHARMAKON_ENTROPY_TIER1", 0.50);
        let t2 = read_entropy_tier_env("PHARMAKON_ENTROPY_TIER2", 0.70);
        let t3 = read_entropy_tier_env("PHARMAKON_ENTROPY_TIER3", 0.85);
        let t4 = read_entropy_tier_env("PHARMAKON_MAX_ENTROPY", 0.95);

        if entropy > t4 {
            EntropyTier::Overflow
        } else if entropy > t3 {
            EntropyTier::Critical
        } else if entropy > t2 {
            EntropyTier::High
        } else if entropy > t1 {
            EntropyTier::Elevated
        } else {
            EntropyTier::Normal
        }
    }

    /// Entry threshold for this tier (used by hysteresis).
    pub fn entry_threshold(&self) -> f32 {
        match self {
            EntropyTier::Normal => 0.0,
            EntropyTier::Elevated => read_entropy_tier_env("PHARMAKON_ENTROPY_TIER1", 0.50),
            EntropyTier::High => read_entropy_tier_env("PHARMAKON_ENTROPY_TIER2", 0.70),
            EntropyTier::Critical => read_entropy_tier_env("PHARMAKON_ENTROPY_TIER3", 0.85),
            EntropyTier::Overflow => read_entropy_tier_env("PHARMAKON_MAX_ENTROPY", 0.95),
        }
    }

    /// Numeric tier for event logging.
    pub fn as_u8(&self) -> u8 {
        match self {
            EntropyTier::Normal => 0,
            EntropyTier::Elevated => 1,
            EntropyTier::High => 2,
            EntropyTier::Critical => 3,
            EntropyTier::Overflow => 4,
        }
    }
}

fn read_entropy_tier_env(var: &str, default: f32) -> f32 {
    std::env::var(var)
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(default)
}

impl ProgressTracker {
    pub fn new(policy: &TerminationPolicy) -> Self {
        let stall_threshold = match policy {
            TerminationPolicy::ProgressBased { stall_threshold } => *stall_threshold,
            _ => 3, // Default for other policies
        };
        Self {
            history: VecDeque::with_capacity(10),
            embeddings: VecDeque::with_capacity(10),
            stall_count: 0,
            stall_threshold,
            max_history: 10,
            current_tier: EntropyTier::Normal,
            tier_escalated: false,
            cosine_stagnation_count: 0,
        }
    }

    /// Update the entropy tier with hysteresis and return whether it escalated.
    ///
    /// Hysteresis prevents rapid oscillation at tier boundaries:
    /// - Entering a tier: entropy must exceed the tier's entry threshold.
    /// - Leaving a tier: entropy must drop below (entry_threshold - 0.05).
    pub fn update_tier(&mut self, entropy: f32) -> (EntropyTier, bool) {
        let raw_tier = EntropyTier::classify(entropy);
        const HYSTERESIS: f32 = 0.05;

        let effective_tier =
            if raw_tier < self.current_tier && self.current_tier != EntropyTier::Normal {
                let entry = self.current_tier.entry_threshold();
                if entropy > entry - HYSTERESIS {
                    self.current_tier // hold current tier
                } else {
                    raw_tier
                }
            } else {
                raw_tier
            };

        let escalated = effective_tier > self.current_tier;
        self.current_tier = effective_tier;
        self.tier_escalated = escalated;
        (effective_tier, escalated)
    }

    /// Returns the current entropy tier (after hysteresis).
    pub fn current_tier(&self) -> EntropyTier {
        self.current_tier
    }

    /// Check entropy and return a termination signal if the score exceeds the threshold.
    /// This integrates the rich entropy from EventLog into the progress tracking pipeline.
    pub fn check_entropy(&self, entropy: f32, threshold: f32) -> TerminationSignal {
        if entropy > threshold {
            let tier = self.current_tier.as_u8();
            TerminationSignal::EntropyOverflow {
                score: entropy,
                tier,
            }
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
            log::warn!(
                "Loop detected: Same tool call args repeated {} times.",
                loop_count
            );
            return TerminationSignal::LoopDetected;
        }

        // Cosine stagnation: detect subtly identical iteration patterns
        // even when tool args differ. (from LKO Adaptive Runtime pattern)
        let embedding = snapshot.to_embedding();
        let mut cosine_stagnation_signal: Option<TerminationSignal> = None;
        if let Some(prev_emb) = self.embeddings.back() {
            let cos = cosine_similarity(&prev_emb.features, &embedding.features);
            if cos > 0.98 {
                self.cosine_stagnation_count += 1;
                if self.cosine_stagnation_count >= 2 {
                    log::warn!(
                        "Cosine stagnation detected: cos={:.3} for {} consecutive iterations.",
                        cos,
                        self.cosine_stagnation_count
                    );
                    cosine_stagnation_signal = Some(TerminationSignal::CosineStagnation { cos });
                }
            } else if cos > 0.95 {
                // Micro-stagnation: don't terminate but note it
                log::debug!("Micro-stagnation: cos={:.3}", cos);
                self.cosine_stagnation_count = 0;
            } else {
                self.cosine_stagnation_count = 0;
            }
        }
        self.embeddings.push_back(embedding);
        if self.embeddings.len() > self.max_history {
            self.embeddings.pop_front();
        }

        let progress = self.measure_delta(&snapshot);

        if progress < 0.01 {
            // Using a small epsilon for progress
            self.stall_count += 1;
            log::warn!(
                "Stall count increased to {}/{}",
                self.stall_count,
                self.stall_threshold
            );
        } else {
            self.stall_count = 0; // Progress was made, reset the counter
            log::info!(
                "Progress detected (score: {:.2}), stall count reset.",
                progress
            );
        }

        self.history.push_back(snapshot);
        if self.history.len() > self.max_history {
            self.history.pop_front();
        }

        // Return cosine stagnation before regular stall check
        if let Some(signal) = cosine_stagnation_signal {
            return signal;
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
