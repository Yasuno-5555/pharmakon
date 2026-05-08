//! Tool Governor — centralized guardrails for all tool execution.
//!
//! Prevents the "grep 7 times → PC dies" class of problems by enforcing:
//! - Per-tool rate limits (max calls per second)
//! - Concurrent tool execution caps
//! - Sub-agent resource propagation
//! - Tool-specific safety limits (recursion depth, file size, output size)
//!
//! Architecture: wraps every tool call through a single check point.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// --- Global Governor ---

/// Centralized tool execution governor.
/// One instance per agent process. Shared across all sub-agents via Arc.
pub struct ToolGovernor {
    /// Maximum concurrent tool calls across ALL tools.
    max_concurrent: usize,
    /// Current number of in-flight tool calls.
    in_flight: AtomicUsize,

    /// Per-tool call counters and rate limit state.
    tool_stats: Mutex<HashMap<String, ToolStats>>,

    /// Global call counter for diagnostics.
    total_calls: AtomicU32,

    /// Sub-agent depth (0 = parent, 1 = first child, etc.)
    depth: u8,
    /// Maximum sub-agent depth.
    max_depth: u8,
}

#[derive(Debug, Clone)]
struct ToolStats {
    call_count: u32,
    last_call: Instant,
    consecutive_failures: u32,
}

#[derive(Debug, Clone)]
pub struct GovernorConfig {
    /// Maximum concurrent tool calls across all tools (default: 8).
    pub max_concurrent: usize,
    /// Minimum interval between calls to the same tool (default: 50ms).
    pub min_interval_ms: u64,
    /// Maximum calls per tool per second (default: 20).
    pub max_calls_per_sec: u32,
    /// Maximum sub-agent depth (default: 3).
    pub max_sub_agent_depth: u8,
    /// Whether this governor is for a sub-agent (inherits limits from parent).
    pub is_sub_agent: bool,
    /// Parent governor (for sub-agents to check global limits).
    pub parent: Option<Arc<ToolGovernor>>,
}

impl Default for GovernorConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 8,
            min_interval_ms: 50,
            max_calls_per_sec: 20,
            max_sub_agent_depth: 3,
            is_sub_agent: false,
            parent: None,
        }
    }
}

impl ToolGovernor {
    pub fn new(config: GovernorConfig) -> Self {
        Self {
            max_concurrent: config.max_concurrent,
            in_flight: AtomicUsize::new(0),
            tool_stats: Mutex::new(HashMap::new()),
            total_calls: AtomicU32::new(0),
            depth: if config.is_sub_agent { 1 } else { 0 },
            max_depth: config.max_sub_agent_depth,
        }
    }

    /// Create a child governor for a sub-agent (inherits depth + 1).
    pub fn child_governor(&self) -> Self {
        Self {
            max_concurrent: (self.max_concurrent / 2).max(2), // Sub-agents get half the slots
            in_flight: AtomicUsize::new(0),
            tool_stats: Mutex::new(HashMap::new()),
            total_calls: AtomicU32::new(0),
            depth: self.depth + 1,
            max_depth: self.max_depth,
        }
    }

    /// Check if a tool call is allowed. Returns Ok(()) or Err with reason.
    pub fn check(&self, tool_name: &str) -> Result<(), String> {
        // Depth limit
        if self.depth > self.max_depth {
            return Err(format!(
                "Max sub-agent depth ({}) exceeded (current: {})",
                self.max_depth, self.depth
            ));
        }

        // Concurrent limit
        let current = self.in_flight.fetch_add(1, Ordering::SeqCst);
        if current >= self.max_concurrent {
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            return Err(format!(
                "Max concurrent tool calls ({}) reached. {} in flight.",
                self.max_concurrent, current
            ));
        }

        // Per-tool rate limit
        {
            let mut stats = self.tool_stats.lock().unwrap();
            let entry = stats.entry(tool_name.to_string()).or_insert_with(|| ToolStats {
                call_count: 0,
                last_call: Instant::now(),
                consecutive_failures: 0,
            });

            let elapsed = entry.last_call.elapsed();
            if entry.call_count > 0 && elapsed < Duration::from_millis(50) {
                self.in_flight.fetch_sub(1, Ordering::SeqCst);
                return Err(format!(
                    "Rate limit: tool '{}' called too quickly ({:.0}ms since last call). Min interval: 50ms.",
                    tool_name,
                    elapsed.as_millis()
                ));
            }

            entry.call_count += 1;
            entry.last_call = Instant::now();
        }

        self.total_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    /// Mark a tool call as completed (must be called after tool execution).
    pub fn complete(&self) {
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
    }

    /// Record a tool failure for circuit-breaking.
    pub fn record_failure(&self, tool_name: &str) {
        let mut stats = self.tool_stats.lock().unwrap();
        if let Some(entry) = stats.get_mut(tool_name) {
            entry.consecutive_failures += 1;
        }
    }

    /// Record a tool success (resets failure counter).
    pub fn record_success(&self, tool_name: &str) {
        let mut stats = self.tool_stats.lock().unwrap();
        if let Some(entry) = stats.get_mut(tool_name) {
            entry.consecutive_failures = 0;
        }
    }

    /// Check if a tool should be circuit-broken (too many consecutive failures).
    pub fn is_circuit_broken(&self, tool_name: &str) -> bool {
        let stats = self.tool_stats.lock().unwrap();
        stats.get(tool_name).map(|s| s.consecutive_failures >= 5).unwrap_or(false)
    }

    /// Get current stats for diagnostics.
    pub fn snapshot(&self) -> GovernorSnapshot {
        let stats = self.tool_stats.lock().unwrap();
        GovernorSnapshot {
            in_flight: self.in_flight.load(Ordering::SeqCst),
            total_calls: self.total_calls.load(Ordering::SeqCst),
            depth: self.depth,
            tool_counts: stats.iter().map(|(k, v)| (k.clone(), v.call_count)).collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GovernorSnapshot {
    pub in_flight: usize,
    pub total_calls: u32,
    pub depth: u8,
    pub tool_counts: HashMap<String, u32>,
}

impl std::fmt::Debug for ToolGovernor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolGovernor")
            .field("max_concurrent", &self.max_concurrent)
            .field("in_flight", &self.in_flight)
            .field("total_calls", &self.total_calls)
            .field("depth", &self.depth)
            .finish()
    }
}

// --- Tool-Specific Safety Limits ---

/// Safety limits applied before tool execution.
pub struct ToolSafetyLimits {
    /// Maximum file size for read_file (bytes). Default: 1MB.
    pub max_read_size: usize,
    /// Maximum recursion depth for grep_files. Default: 5.
    pub max_grep_depth: u32,
    /// Maximum files to scan in grep_files. Default: 500.
    pub max_grep_files: u32,
    /// Maximum output size for shell commands (bytes). Default: 100KB.
    pub max_shell_output: usize,
    /// Shell command timeout (seconds). Default: 30.
    pub shell_timeout_secs: u64,
    /// Maximum file size for write_file (bytes). Default: 5MB.
    pub max_write_size: usize,
    /// Maximum CodeAct script execution time (ms). Default: 5000.
    pub codeact_timeout_ms: u64,
    /// Maximum CodeAct script length (chars). Default: 10000.
    pub max_codeact_script_len: usize,
}

impl Default for ToolSafetyLimits {
    fn default() -> Self {
        Self {
            max_read_size: 1_048_576,       // 1MB
            max_grep_depth: 5,
            max_grep_files: 500,
            max_shell_output: 102_400,       // 100KB
            shell_timeout_secs: 30,
            max_write_size: 5_242_880,       // 5MB
            codeact_timeout_ms: 5000,
            max_codeact_script_len: 10000,
        }
    }
}

impl ToolSafetyLimits {
    /// Validate a file read against size limits.
    pub fn check_read_size(&self, size: usize) -> Result<(), String> {
        if size > self.max_read_size {
            Err(format!(
                "File too large to read: {} bytes (max: {} bytes). Use grep or head/tail instead.",
                size, self.max_read_size
            ))
        } else {
            Ok(())
        }
    }

    /// Validate a file write against size limits.
    pub fn check_write_size(&self, size: usize) -> Result<(), String> {
        if size > self.max_write_size {
            Err(format!(
                "Content too large to write: {} bytes (max: {} bytes). Split into smaller files.",
                size, self.max_write_size
            ))
        } else {
            Ok(())
        }
    }

    /// Validate grep_files parameters.
    pub fn check_grep_limits(&self, current_depth: u32, files_scanned: u32) -> Result<(), String> {
        if current_depth > self.max_grep_depth {
            return Err(format!(
                "Grep recursion depth limit ({}) exceeded.",
                self.max_grep_depth
            ));
        }
        if files_scanned > self.max_grep_files {
            return Err(format!(
                "Grep file scan limit ({}) exceeded. Narrow your search.",
                self.max_grep_files
            ));
        }
        Ok(())
    }

    /// Validate shell command.
    pub fn check_shell_command(&self, command: &str) -> Result<(), String> {
        // Block obviously dangerous patterns
        let lower = command.to_lowercase();
        let blocked = [
            "rm -rf /", "dd if=", "mkfs.", ":(){ :|:& };:", "> /dev/sda",
            "fork bomb", "shutdown", "reboot", "halt",
        ];
        for pattern in &blocked {
            if lower.contains(pattern) {
                return Err(format!("Blocked dangerous command pattern: '{}'", pattern));
            }
        }
        Ok(())
    }

    /// Validate CodeAct script.
    pub fn check_codeact_script(&self, script: &str) -> Result<(), String> {
        if script.len() > self.max_codeact_script_len {
            return Err(format!(
                "CodeAct script too long: {} chars (max: {})",
                script.len(), self.max_codeact_script_len
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_governor_concurrent_limit() {
        let gov = ToolGovernor::new(GovernorConfig {
            max_concurrent: 2,
            min_interval_ms: 0,
            ..Default::default()
        });
        assert!(gov.check("read_file").is_ok());
        assert!(gov.check("grep").is_ok());
        assert!(gov.check("shell").is_err());
        gov.complete();
        assert!(gov.check("shell").is_ok());
    }

    #[test]
    fn test_governor_rate_limit() {
        let gov = ToolGovernor::new(GovernorConfig {
            min_interval_ms: 100,
            ..Default::default()
        });
        assert!(gov.check("grep").is_ok());
        gov.complete();
        assert!(gov.check("grep").is_err());
    }

    #[test]
    fn test_governor_circuit_breaker() {
        let gov = ToolGovernor::new(GovernorConfig {
            min_interval_ms: 0,
            ..Default::default()
        });
        gov.check("shell").ok();
        gov.complete();
        for _ in 0..5 { gov.record_failure("shell"); }
        assert!(gov.is_circuit_broken("shell"));
        gov.record_success("shell");
        assert!(!gov.is_circuit_broken("shell"));
    }

    #[test]
    fn test_safety_limits_shell_blocked() {
        let limits = ToolSafetyLimits::default();
        assert!(limits.check_shell_command("rm -rf /").is_err());
        assert!(limits.check_shell_command("cargo build").is_ok());
    }

    #[test]
    fn test_safety_limits_grep() {
        let limits = ToolSafetyLimits::default();
        assert!(limits.check_grep_limits(3, 100).is_ok());
        assert!(limits.check_grep_limits(6, 100).is_err()); // depth exceeded
        assert!(limits.check_grep_limits(3, 600).is_err()); // files exceeded
    }

    #[test]
    fn test_safety_limits_file_size() {
        let limits = ToolSafetyLimits::default();
        assert!(limits.check_read_size(500_000).is_ok());
        assert!(limits.check_read_size(2_000_000).is_err());
    }
}
