use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde_json::{json, Value};
use std::process::Command;
use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};



pub struct TemporalAwarenessTool;

impl Default for TemporalAwarenessTool {
    fn default() -> Self { Self::new() }
}

impl TemporalAwarenessTool {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl Tool for TemporalAwarenessTool {
    fn name(&self) -> &str {
        "temporal_awareness"
    }
    fn description(&self) -> &str {
        "Analyze a file's git history to understand its change frequency, recent authors, and temporal risk (churn)."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file to analyze" },
                "lookback_days": { "type": "integer", "description": "Number of days to look back for churn analysis (default 30)", "default": 30 }
            },
            "required": ["path"]
        })
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let path = args["path"].as_str().ok_or_else(|| AgentError("Missing path".into()))?;
        let days = args["lookback_days"].as_u64().unwrap_or(30);

        let since_arg = format!("--since={} days ago", days);
        let log_cmd = Command::new("git")
            .args(["log", &since_arg, "--oneline", "--", path])
            .output()
            .map_err(|e| AgentError(format!("Git log failed: {}", e)))?;

        let log_output = String::from_utf8_lossy(&log_cmd.stdout).to_string();
        let commit_count = if log_output.trim().is_empty() {
            0
        } else {
            log_output.lines().count()
        };

        let author_cmd = Command::new("git")
            .args(["log", &since_arg, "--format=%an", "--", path])
            .output()
            .map_err(|e| AgentError(format!("Git log (authors) failed: {}", e)))?;
            
        let author_output = String::from_utf8_lossy(&author_cmd.stdout).to_string();
        let mut author_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for author in author_output.lines() {
            let author = author.trim().to_string();
            if !author.is_empty() {
                *author_counts.entry(author).or_insert(0) += 1;
            }
        }
        let mut sorted_authors: Vec<_> = author_counts.into_iter().collect();
        sorted_authors.sort_by(|a, b| b.1.cmp(&a.1));

        let last_mod_cmd = Command::new("git")
            .args(["log", "-1", "--format=%cd", "--date=relative", "--", path])
            .output()
            .map_err(|e| AgentError(format!("Git last mod failed: {}", e)))?;
            
        let last_modified = String::from_utf8_lossy(&last_mod_cmd.stdout).trim().to_string();

        let risk_level = if commit_count > 10 {
            "HIGH (Hotspot - frequently changed recently)"
        } else if commit_count > 3 {
            "MEDIUM (Active development)"
        } else {
            "LOW (Stable)"
        };

        let mut report = format!("### Temporal Analysis for `{}`\n", path);
        if last_modified.is_empty() {
            report.push_str("File is either untracked or has no git history.\n");
            return Ok(report);
        }

        report.push_str(&format!("- **Last Modified**: {}\n", last_modified));
        report.push_str(&format!("- **Changes in last {} days**: {}\n", days, commit_count));
        report.push_str(&format!("- **Volatility Risk**: {}\n", risk_level));
        
        if !sorted_authors.is_empty() {
            report.push_str("\n**Recent Authors:**\n");
            for (author, count) in sorted_authors {
                report.push_str(&format!("- {} ({} commits)\n", author, count));
            }
        }

        Ok(report)
    }
}


pub struct FailurePredictionTool;

impl Default for FailurePredictionTool {
    fn default() -> Self { Self::new() }
}

impl FailurePredictionTool {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl Tool for FailurePredictionTool {
    fn name(&self) -> &str {
        "failure_prediction"
    }
    fn description(&self) -> &str {
        "Analyze a file to predict the risk of introducing a regression or bug based on complexity and historical fix frequency."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file to analyze" }
            },
            "required": ["path"]
        })
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let path = args["path"].as_str().ok_or_else(|| AgentError("Missing path".into()))?;

        // 1. Static Analysis: Complexity Proxy & Safety Hotspots
        let content = fs::read_to_string(path).map_err(|e| AgentError(format!("Failed to read file: {}", e)))?;
        let lines_of_code = content.lines().filter(|l| !l.trim().is_empty()).count();
        
        let mut max_indent = 0;
        let mut avg_indent_acc = 0;
        let mut unwrap_count = 0;
        let mut unsafe_count = 0;
        let mut clone_count = 0;
        let mut panic_count = 0;
        let mut deep_nest_loops = 0;

        for line in content.lines() {
            let trimmed = line.trim();
            let indent = line.chars().take_while(|c| *c == ' ' || *c == '\t').count();
            if indent > max_indent { max_indent = indent; }
            avg_indent_acc += indent;

            // Simple robust regex/string safety checks
            if trimmed.contains(".unwrap()") || trimmed.contains(".expect(") {
                unwrap_count += 1;
            }
            if trimmed.contains("unsafe {") || trimmed.starts_with("unsafe ") {
                unsafe_count += 1;
            }
            if trimmed.contains(".clone()") {
                clone_count += 1;
            }
            if trimmed.contains("panic!(") || trimmed.contains("todo!(") || trimmed.contains("unimplemented!(") {
                panic_count += 1;
            }
            // Loop complexity proxy
            if (trimmed.starts_with("for ") || trimmed.starts_with("while ") || trimmed.starts_with("loop ")) && indent >= 12 {
                deep_nest_loops += 1;
            }
        }
        let avg_indent = if lines_of_code > 0 { avg_indent_acc as f64 / lines_of_code as f64 } else { 0.0 };

        // 2. Historical Analysis: Bug Fix Frequency
        let bug_cmd = Command::new("git")
            .args(["log", "--oneline", "-i", "-E", "--grep=(fix|bug|issue|resolve)", "--", path])
            .output()
            .map_err(|e| AgentError(format!("Git log failed: {}", e)))?;
        
        let bug_output = String::from_utf8_lossy(&bug_cmd.stdout).to_string();
        let bug_fix_count = if bug_output.trim().is_empty() { 0 } else { bug_output.lines().count() };

        let total_cmd = Command::new("git")
            .args(["log", "--oneline", "--", path])
            .output()
            .unwrap_or_else(|_| std::process::Output {
                status: std::os::unix::process::ExitStatusExt::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            });
        let total_output = String::from_utf8_lossy(&total_cmd.stdout).to_string();
        let total_commits = if total_output.trim().is_empty() { 0 } else { total_output.lines().count() };

        let defect_density = if total_commits > 0 {
            (bug_fix_count as f64 / total_commits as f64) * 100.0
        } else {
            0.0
        };

        // 3. Risk Scoring (0 to 100)
        let mut risk_score = 0.0;
        
        // Size contribution (max 20)
        if lines_of_code > 500 { risk_score += 20.0; }
        else if lines_of_code > 200 { risk_score += 10.0; }
        
        // Layout nesting contribution (max 20)
        if max_indent > 24 { risk_score += 20.0; }
        else if max_indent > 16 { risk_score += 10.0; }

        // History contribution (max 40)
        if defect_density > 50.0 { risk_score += 40.0; }
        else if defect_density > 30.0 { risk_score += 25.0; }
        else if defect_density > 10.0 { risk_score += 10.0; }

        // Safety Hotspot penalty contribution (max 20)
        let hotspot_penalty = (unwrap_count * 2) + (unsafe_count * 5) + (panic_count * 3) + (deep_nest_loops * 4) + (clone_count / 3);
        risk_score += (hotspot_penalty as f64).min(20.0);

        let risk_level = if risk_score >= 75.0 {
            "CRITICAL (High probability of introducing regressions)"
        } else if risk_score >= 45.0 {
            "MODERATE (Tread carefully, enforce rigorous testing)"
        } else {
            "LOW (Generally stable/safe)"
        };

        let mut report = format!("### Failure Prediction Analysis for `{}`\n\n", path);
        report.push_str("#### 1. Code Complexity & Nesting Metrics\n");
        report.push_str(&format!("- **Lines of Code**: {}\n", lines_of_code));
        report.push_str(&format!("- **Max Indentation Depth**: {} spaces\n", max_indent));
        report.push_str(&format!("- **Avg Indentation Depth**: {:.1} spaces\n", avg_indent));
        report.push_str(&format!("- **Deeply Nested Loops (>=3 levels)**: {}\n\n", deep_nest_loops));

        report.push_str("#### 2. Safety & Performance Hotspots (Static Analysis)\n");
        report.push_str(&format!("- **Unchecked Panickers (`.unwrap()`/`.expect()`))**: {}\n", unwrap_count));
        report.push_str(&format!("- **Unsafe Code Blocks (`unsafe`)**: {}\n", unsafe_count));
        report.push_str(&format!("- **Explicit Panics (`panic!()`/`todo!()`)**: {}\n", panic_count));
        report.push_str(&format!("- **Heap Allocating Clones (`.clone()`)**: {}\n\n", clone_count));
        
        report.push_str("#### 3. Historical Defect Metrics (Git Log)\n");
        report.push_str(&format!("- **Total Commits**: {}\n", total_commits));
        report.push_str(&format!("- **Bug Fix Commits**: {}\n", bug_fix_count));
        report.push_str(&format!("- **Historical Defect Ratio**: {:.1}%\n\n", defect_density));
        
        report.push_str("#### 4. Overall Risk Assessment\n");
        report.push_str(&format!("- Computed Risk Score: **{:.0} / 100**\n", risk_score));
        report.push_str(&format!("- Predicted Risk Level: **{}**\n", risk_level));
        
        if risk_score >= 45.0 {
            report.push_str("\n> [!WARNING]\n");
            report.push_str("> **RECOMMENDATION**: This file has substantial complexity, history of defects, or safety hazards. Please write unit tests or perform a focused review of any modified unsafe/unwrap paths.");
        } else {
            report.push_str("\n> [!NOTE]\n");
            report.push_str("> This file appears relatively safe and maintains high code hygiene standards.");
        }

        Ok(report)
    }
}


pub struct ProactiveSelfOptimizationTool;

impl Default for ProactiveSelfOptimizationTool {
    fn default() -> Self { Self::new() }
}

impl ProactiveSelfOptimizationTool {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl Tool for ProactiveSelfOptimizationTool {
    fn name(&self) -> &str {
        "proactive_self_optimization"
    }
    fn description(&self) -> &str {
        "Proactively scan the workspace for performance bottlenecks, unoptimized clones, and code smells, returning actionable refactoring suggestions."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "scan_type": {
                    "type": "string",
                    "enum": ["clippy_perf", "hotspot_analysis"],
                    "description": "Type of optimization scan to run. 'clippy_perf' checks for Rust performance lints. 'hotspot_analysis' searches for cloning in loops."
                }
            },
            "required": ["scan_type"]
        })
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let scan_type = args["scan_type"].as_str().unwrap_or("clippy_perf");

        match scan_type {
            "clippy_perf" => {
                let cmd_output = Command::new("cargo")
                    .args(["clippy", "--message-format=short", "--", "-W", "clippy::perf"])
                    .output()
                    .map_err(|e| AgentError(format!("Cargo clippy failed: {}", e)))?;

                let stdout = String::from_utf8_lossy(&cmd_output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&cmd_output.stderr).to_string();

                let mut issues = Vec::new();
                for line in stdout.lines().chain(stderr.lines()) {
                    if line.contains("warning: ") || line.contains("error: ") {
                        issues.push(line.to_string());
                    }
                }

                if issues.is_empty() {
                    Ok("No critical performance issues found by Clippy. The codebase is well optimized.".into())
                } else {
                    let mut report = String::from("### Performance Optimization Opportunities (Clippy)\n\n");
                    for issue in issues.iter().take(20) {
                        report.push_str(&format!("- `{}`\n", issue));
                    }
                    if issues.len() > 20 {
                        report.push_str(&format!("\n...and {} more issues. Please fix these first.", issues.len() - 20));
                    }
                    Ok(report)
                }
            },
            "hotspot_analysis" => {
                let cmd = Command::new("git")
                    .args(["grep", "-n", "clone()", "--", "*.rs"])
                    .output()
                    .map_err(|e| AgentError(format!("Grep failed: {}", e)))?;

                let stdout = String::from_utf8_lossy(&cmd.stdout).to_string();
                let lines: Vec<&str> = stdout.lines().collect();

                let mut report = String::from("### Hotspot Analysis (Potential Unnecessary Clones)\n");
                report.push_str("Consider using references (`&`) or `Arc` instead of `.clone()` for the following:\n\n");

                for line in lines.iter().take(30) {
                    report.push_str(&format!("- {}\n", line));
                }

                if lines.len() > 30 {
                    report.push_str(&format!("\n...and {} more clone sites.", lines.len() - 30));
                }

                Ok(report)
            },
            _ => Err(AgentError("Unknown scan_type".into()))
        }
    }
}


pub struct RegretMinimizationTool;

impl Default for RegretMinimizationTool {
    fn default() -> Self { Self::new() }
}

impl RegretMinimizationTool {
    pub fn new() -> Self { Self }

    fn get_db_path() -> PathBuf {
        let dir = PathBuf::from(".pharmakon");
        if !dir.exists() {
            let _ = fs::create_dir_all(&dir);
        }
        dir.join("regrets.json")
    }

    fn load_regrets() -> Vec<RegretEntry> {
        let path = Self::get_db_path();
        if !path.exists() {
            return Vec::new();
        }
        let data = fs::read_to_string(&path).unwrap_or_default();
        serde_json::from_str(&data).unwrap_or_else(|_| Vec::new())
    }

    fn save_regrets(regrets: &Vec<RegretEntry>) -> Result<(), std::io::Error> {
        let data = serde_json::to_string_pretty(regrets)?;
        fs::write(Self::get_db_path(), data)
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct RegretEntry {
    context: String,
    approach: String,
    reason: String,
}

#[async_trait]
impl Tool for RegretMinimizationTool {
    fn name(&self) -> &str {
        "regret_minimization"
    }
    fn description(&self) -> &str {
        "Store and retrieve 'regrets' (failed approaches or mistakes) to avoid repeating them in the future. Useful for pruning search spaces during planning."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["record", "query"],
                    "description": "Whether to record a new regret, or query past regrets."
                },
                "context": {
                    "type": "string",
                    "description": "The problem context or file/module being worked on. Required for 'record', optional for 'query'."
                },
                "approach": {
                    "type": "string",
                    "description": "The approach that failed. Required for 'record'."
                },
                "reason": {
                    "type": "string",
                    "description": "Why it failed (error message, logical flaw). Required for 'record'."
                }
            },
            "required": ["action"]
        })
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let action = args["action"].as_str().unwrap_or("query");

        match action {
            "record" => {
                let context = args["context"].as_str().ok_or_else(|| AgentError("Missing 'context'".into()))?;
                let approach = args["approach"].as_str().ok_or_else(|| AgentError("Missing 'approach'".into()))?;
                let reason = args["reason"].as_str().ok_or_else(|| AgentError("Missing 'reason'".into()))?;

                let mut regrets = Self::load_regrets();
                regrets.push(RegretEntry {
                    context: context.to_string(),
                    approach: approach.to_string(),
                    reason: reason.to_string(),
                });

                Self::save_regrets(&regrets).map_err(|e| AgentError(format!("Failed to save: {}", e)))?;
                Ok("Regret recorded successfully. This approach will be avoided in the future.".into())
            }
            "query" => {
                let regrets = Self::load_regrets();
                if regrets.is_empty() {
                    return Ok("No regrets recorded yet.".into());
                }

                let filter_ctx = args.get("context").and_then(|v| v.as_str()).unwrap_or("");
                
                let mut scored_regrets: Vec<(f64, &RegretEntry)> = Vec::new();
                
                if !filter_ctx.is_empty() {
                    let query_terms: Vec<String> = filter_ctx
                        .to_lowercase()
                        .split_whitespace()
                        .map(|s| s.chars().filter(|c| c.is_alphanumeric()).collect())
                        .filter(|s: &String| !s.is_empty())
                        .collect();
                        
                    for r in &regrets {
                        let mut score = 0.0;
                        let context_lower = r.context.to_lowercase();
                        let approach_lower = r.approach.to_lowercase();
                        let reason_lower = r.reason.to_lowercase();
                        
                        for term in &query_terms {
                            if context_lower.contains(term) { score += 10.0; }
                            if approach_lower.contains(term) { score += 3.0; }
                            if reason_lower.contains(term) { score += 2.0; }
                        }
                        
                        // Large bonus for exact phrase match
                        if context_lower.contains(&filter_ctx.to_lowercase()) { score += 20.0; }
                        
                        if score > 0.0 {
                            scored_regrets.push((score, r));
                        }
                    }
                    scored_regrets.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
                } else {
                    for r in &regrets {
                        scored_regrets.push((0.0, r));
                    }
                }

                if scored_regrets.is_empty() {
                    return Ok(format!("No regrets matched the query context '{}'.", filter_ctx));
                }

                let mut output = String::new();
                output.push_str(&format!("### Known Regrets & Avoided Approaches (Semantic Search Results for '{}')\n\n", filter_ctx));
                
                for (i, (score, r)) in scored_regrets.iter().enumerate() {
                    output.push_str(&format!("#### Regret #{} (Relevance Score: {:.1})\n", i + 1, score));
                    output.push_str(&format!("- **Context**: {}\n", r.context));
                    output.push_str(&format!("- **Failed Approach**: {}\n", r.approach));
                    output.push_str(&format!("- **Reason / Outcome**: {}\n\n", r.reason));
                }

                Ok(output)
            }
            _ => Err(AgentError("Invalid action. Must be 'record' or 'query'.".into()))
        }
    }
}

