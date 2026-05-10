//! World Model Agent V2 — Simulate-before-acting execution loop with Plan AST, Static Verification, and Failure Taxonomy.
//!
//! Activated for Deep tasks; Simple/Standard tasks skip to standard CodeAct.

use crate::agent::Agent;
use crate::model::{CompletionRequest, Message, MessageContent};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use pharmakon_common::Tool;
use futures::future::{BoxFuture, FutureExt};

// ═══════════════════════════════════════════════════════
// Plan AST Types
// ═══════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PlanNode {
    Step {
        tool: String,
        args: serde_json::Value,
        #[serde(default)]
        dry_run_first: bool,
    },
    Sequence {
        nodes: Vec<PlanNode>,
    },
    Parallel {
        nodes: Vec<PlanNode>,
    },
    Conditional {
        condition_script: String,
        then_branch: Box<PlanNode>,
        else_branch: Option<Box<PlanNode>>,
    },
    Retry {
        node: Box<PlanNode>,
        max_attempts: usize,
    },
    Verify {
        node: Box<PlanNode>,
        assertion_script: String,
    },
    Gate {
        gate_name: String,
        node: Box<PlanNode>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidatePlan {
    pub id: String,
    pub description: String,
    #[serde(default)]
    pub estimated_tokens: u64,
    #[serde(default)]
    pub steps: Vec<PlanStep>,
    pub root: Option<PlanNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub tool: String,
    pub args: serde_json::Value,
    #[serde(default)]
    pub depends_on: Vec<usize>,
    #[serde(default)]
    pub dry_run_first: bool,
}

impl CandidatePlan {
    pub fn get_ast(&self) -> PlanNode {
        if let Some(ref root) = self.root {
            root.clone()
        } else {
            // Compile flat steps list to AST Sequence
            let mut nodes = Vec::new();
            for step in &self.steps {
                nodes.push(PlanNode::Step {
                    tool: step.tool.clone(),
                    args: step.args.clone(),
                    dry_run_first: step.dry_run_first,
                });
            }
            PlanNode::Sequence { nodes }
        }
    }
}

/// Validation result for a plan after simulation and verification.
#[derive(Debug, Clone)]
pub struct PlanValidation {
    pub plan_id: String,
    pub valid: bool,
    pub issues: Vec<String>,
    pub score: f64,
    pub token_cost: u64,
    pub evpi: f64,
}

// ═══════════════════════════════════════════════════════
// Static Verifier (Pre-execution Constraint Validation)
// ═══════════════════════════════════════════════════════

pub struct StaticVerifier {
    pub risk_ceiling: f64,
}

impl StaticVerifier {
    pub fn new(risk_ceiling: f64) -> Self {
        Self { risk_ceiling }
    }

    pub async fn verify(&self, node: &PlanNode, workspace_root: &Path) -> Result<Vec<String>> {
        let mut issues = Vec::new();
        let mut simulated_created_paths = HashSet::new();
        self.verify_node(node, workspace_root, &mut issues, &mut simulated_created_paths).await?;
        Ok(issues)
    }

    async fn verify_node(
        &self,
        node: &PlanNode,
        workspace_root: &Path,
        issues: &mut Vec<String>,
        simulated_created_paths: &mut HashSet<PathBuf>,
    ) -> Result<()> {
        match node {
            PlanNode::Step { tool, args, .. } => {
                // 1. Risk ceiling enforcement
                let tool_risk = match tool.as_str() {
                    "shell" => 0.9,
                    "codeact" => 0.8,
                    "write_file" => 0.6,
                    "apply_patch" => 0.5,
                    _ => 0.1,
                };
                if tool_risk > self.risk_ceiling {
                    issues.push(format!("Risk ceiling violation: Tool '{}' has risk {} which exceeds ceiling {}", tool, tool_risk, self.risk_ceiling));
                }

                // 2. Dangerous shell command patterns
                if tool == "shell" {
                    if let Some(cmd) = args.get("command").and_then(|c| c.as_str()) {
                        let dangerous = ["rm -rf /", "sudo ", "chmod 777", "mkfs", "dd if="];
                        for pattern in dangerous {
                            if cmd.contains(pattern) {
                                issues.push(format!("Dangerous command pattern detected: '{}'", pattern));
                            }
                        }

                        // Heuristic symbolic package / workspace creation
                        if cmd.contains("cargo new ") {
                            if let Some(pos) = cmd.find("cargo new ") {
                                let sub = &cmd[pos + 10..];
                                let name = sub.split_whitespace().next().unwrap_or("");
                                if !name.is_empty() {
                                    let pkg_root = resolve_path(workspace_root, name);
                                    simulated_created_paths.insert(pkg_root.join("Cargo.toml"));
                                    simulated_created_paths.insert(pkg_root.join("src/lib.rs"));
                                    simulated_created_paths.insert(pkg_root.join("src/main.rs"));
                                }
                            }
                        }
                    }
                }

                // 3. Symbolic creation registry to solve the "Time-Paradox" bug
                if tool == "write_file" || tool == "apply_patch" {
                    if let Some(path_str) = args.get("path").and_then(|p| p.as_str()) {
                        let resolved = resolve_path(workspace_root, path_str);
                        simulated_created_paths.insert(resolved);
                    }
                }

                // 4. Hallucinated path check (reading non-existent files)
                if tool == "read_file" {
                    if let Some(path_str) = args.get("path").and_then(|p| p.as_str()) {
                        let resolved = resolve_path(workspace_root, path_str);
                        if !resolved.exists() && !simulated_created_paths.contains(&resolved) {
                            issues.push(format!("Hallucinated path: File '{}' does not exist", path_str));
                        }
                    }
                }

                // 5. Patch applicability dry-run (incorporates symbolic tracking)
                if tool == "apply_patch" {
                    if let Some(path_str) = args.get("path").and_then(|p| p.as_str()) {
                        if let Some(patch_str) = args.get("patch").and_then(|p| p.as_str()) {
                            let resolved = resolve_path(workspace_root, path_str);
                            if resolved.exists() {
                                if let Ok(original) = tokio::fs::read_to_string(&resolved).await {
                                    if let Ok(patch) = diffy::Patch::from_str(patch_str) {
                                        if let Err(e) = diffy::apply(&original, &patch) {
                                            issues.push(format!("Patch dry-run failed for '{}': {}", path_str, e));
                                        }
                                    } else {
                                        issues.push(format!("Invalid patch syntax for '{}'", path_str));
                                    }
                                }
                            } else if !simulated_created_paths.contains(&resolved) {
                                issues.push(format!("Patch target path does not exist: '{}'", path_str));
                            }
                        }
                    }
                }
            }
            PlanNode::Sequence { nodes } | PlanNode::Parallel { nodes } => {
                for child in nodes {
                    Box::pin(self.verify_node(child, workspace_root, issues, simulated_created_paths)).await?;
                }
            }
            PlanNode::Conditional { then_branch, else_branch, .. } => {
                Box::pin(self.verify_node(then_branch, workspace_root, issues, simulated_created_paths)).await?;
                if let Some(else_b) = else_branch {
                    Box::pin(self.verify_node(else_b, workspace_root, issues, simulated_created_paths)).await?;
                }
            }
            PlanNode::Retry { node, .. } | PlanNode::Verify { node, .. } | PlanNode::Gate { node, .. } => {
                Box::pin(self.verify_node(node, workspace_root, issues, simulated_created_paths)).await?;
            }
        }
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════
// Plan Cache & Environment Fingerprinting
// ═══════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedPlan {
    pub plan_id: String,
    pub task: String,
    pub plan: CandidatePlan,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub fingerprint: String,
    pub success_count: u32,
    pub failure_count: u32,
}

impl CachedPlan {
    pub fn freshness(&self) -> f64 {
        let elapsed_secs = chrono::Utc::now().signed_duration_since(self.created_at).num_seconds() as f64;
        let half_life_secs = 7.0 * 24.0 * 60.0 * 60.0; // 1 week
        let lambda = 2.0f64.ln() / half_life_secs;
        (-lambda * elapsed_secs).exp()
    }

    pub fn success_rate(&self) -> f64 {
        let total = (self.success_count + self.failure_count) as f64;
        if total == 0.0 {
            0.5
        } else {
            self.success_count as f64 / total
        }
    }

    pub fn score(&self, current_fingerprint: &str) -> f64 {
        if self.fingerprint != current_fingerprint {
            return 0.0;
        }
        self.success_rate() * self.freshness()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlanCache {
    pub entries: Vec<CachedPlan>,
}

impl PlanCache {
    pub fn load() -> Self {
        let path = dirs::home_dir().unwrap_or_default().join(".pharmakon/plan_cache.json");
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(path) {
                if let Ok(cache) = serde_json::from_str(&content) {
                    return cache;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> Result<()> {
        let path = dirs::home_dir().unwrap_or_default().join(".pharmakon/plan_cache.json");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Evict lowest-score entries when over the limit.
    fn enforce_limit(&mut self, max: usize) {
        if self.entries.len() <= max {
            return;
        }
        // Sort by score ascending (lowest first) and truncate
        self.entries.sort_by(|a, b| {
            a.score("")
                .partial_cmp(&b.score(""))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let remove = self.entries.len() - max;
        self.entries.drain(0..remove);
        log::info!(
            "PlanCache: evicted {} entries (limit: {})",
            remove,
            max
        );
    }

    pub fn find_plan(&self, task: &str, fingerprint: &str) -> Option<CandidatePlan> {
        let mut best_entry: Option<&CachedPlan> = None;
        let mut best_score = 0.0;

        for entry in &self.entries {
            let sim = self.calculate_task_similarity(&entry.task, task);
            if sim >= 0.50 {
                let score = entry.score(fingerprint) * sim;
                if score > best_score && score > 0.1 {
                    best_score = score;
                    best_entry = Some(entry);
                }
            }
        }

        best_entry.map(|e| e.plan.clone())
    }

    fn calculate_task_similarity(&self, a: &str, b: &str) -> f64 {
        let tokens_a = self.tokenize_and_normalize(a);
        let tokens_b = self.tokenize_and_normalize(b);
        let jaccard = self.jaccard_similarity(&tokens_a, &tokens_b);
        let trigram = self.trigram_similarity(a, b);
        0.7 * jaccard + 0.3 * trigram
    }

    fn tokenize_and_normalize(&self, text: &str) -> std::collections::HashSet<String> {
        let stop_words: std::collections::HashSet<&str> = [
            "the", "a", "to", "for", "with", "in", "an", "of", "and", "is", "on", "at", "by", "from", "as"
        ].iter().cloned().collect();

        text.to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { ' ' })
            .collect::<String>()
            .split_whitespace()
            .filter(|w| !stop_words.contains(w) && w.len() > 1)
            .map(|w| w.to_string())
            .collect()
    }

    fn jaccard_similarity(&self, set_a: &std::collections::HashSet<String>, set_b: &std::collections::HashSet<String>) -> f64 {
        if set_a.is_empty() && set_b.is_empty() {
            return 1.0;
        }
        if set_a.is_empty() || set_b.is_empty() {
            return 0.0;
        }
        let intersection: std::collections::HashSet<_> = set_a.intersection(set_b).cloned().collect();
        let union: std::collections::HashSet<_> = set_a.union(set_b).cloned().collect();
        intersection.len() as f64 / union.len() as f64
    }

    fn extract_trigrams(&self, text: &str) -> std::collections::HashSet<String> {
        let normalized = text.to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect::<String>();
        let mut trigrams = std::collections::HashSet::new();
        for word in normalized.split_whitespace() {
            let chars: Vec<char> = word.chars().collect();
            if chars.len() < 3 {
                trigrams.insert(word.to_string());
            } else {
                for window in chars.windows(3) {
                    trigrams.insert(window.iter().collect());
                }
            }
        }
        trigrams
    }

    fn trigram_similarity(&self, a: &str, b: &str) -> f64 {
        let set_a = self.extract_trigrams(a);
        let set_b = self.extract_trigrams(b);
        if set_a.is_empty() && set_b.is_empty() {
            return 1.0;
        }
        if set_a.is_empty() || set_b.is_empty() {
            return 0.0;
        }
        let intersection = set_a.intersection(&set_b).count();
        let union = set_a.union(&set_b).count();
        intersection as f64 / union as f64
    }

    pub fn record_result(&mut self, task: &str, plan: &CandidatePlan, success: bool, fingerprint: String) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.plan_id == plan.id) {
            if success {
                entry.success_count += 1;
            } else {
                entry.failure_count += 1;
            }
            entry.created_at = chrono::Utc::now();
            entry.fingerprint = fingerprint;
        } else {
            self.entries.push(CachedPlan {
                plan_id: plan.id.clone(),
                task: task.to_string(),
                plan: plan.clone(),
                created_at: chrono::Utc::now(),
                fingerprint,
                success_count: if success { 1 } else { 0 },
                failure_count: if success { 0 } else { 1 },
            });
        }
        // Enforce max entries (200) — evict lowest-score plans
        const MAX_PLAN_CACHE_ENTRIES: usize = 200;
        self.enforce_limit(MAX_PLAN_CACHE_ENTRIES);
        let _ = self.save();

        // 🕸️ Non-blocking background thread Pattern Mining & AOT Compilation to prevent thread stalls!
        if success {
            let self_cloned = self.clone();
            tokio::spawn(async move {
                log::info!("Background Thread: Starting pattern mining & AOT compilation...");
                let miner = crate::orchestration::pattern_miner::PatternMiner::new();
                let lib = miner.mine_patterns(&self_cloned);
                if lib.save().is_ok() {
                    let aot = crate::orchestration::aot::AotCompiler::new_with_thresholds(2, 0.90);
                    let _ = aot.compile_and_cache(&lib);
                    log::info!("Background Thread: Pattern mining & AOT compilation completed successfully!");
                }
            });
        }
    }
}

pub fn get_environment_fingerprint(workspace_root: &Path) -> String {
    let mut fingerprint = String::new();
    if let Ok(meta) = std::fs::metadata(workspace_root.join("Cargo.toml")) {
        if let Ok(modified) = meta.modified() {
            fingerprint.push_str(&format!("{:?}", modified));
        }
    }
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(workspace_root)
        .output() {
        if output.status.success() {
            fingerprint.push_str(String::from_utf8_lossy(&output.stdout).trim());
        }
    }
    crate::event_log::short_hash(&fingerprint)
}

// ═══════════════════════════════════════════════════════
// Failure Taxonomy & Strategic Feedback
// ═══════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FailureKind {
    SyntaxError,
    Timeout,
    DangerousCommand,
    PathHallucination,
    PermissionDenied,
    PatchFailed,
    DependencyUnmet,
    ToolExecutionError,
    LogicalFailure,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Recoverability {
    Recoverable,
    StrategicRetry,
    Terminal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanFailure {
    pub kind: FailureKind,
    pub description: String,
    pub recoverability: Recoverability,
    pub feedback_for_planner: String,
}

impl FailureKind {
    pub fn classify(err_msg: &str, tool: &str) -> PlanFailure {
        let err_lower = err_msg.to_lowercase();
        if err_lower.contains("syntax") || err_lower.contains("expected") || err_lower.contains("unresolved import") {
            PlanFailure {
                kind: FailureKind::SyntaxError,
                description: err_msg.to_string(),
                recoverability: Recoverability::Recoverable,
                feedback_for_planner: format!("A syntax error occurred during {}: {}. Please fix the syntax or imports.", tool, err_msg),
            }
        } else if err_lower.contains("timeout") || err_lower.contains("timed out") {
            PlanFailure {
                kind: FailureKind::Timeout,
                description: err_msg.to_string(),
                recoverability: Recoverability::Recoverable,
                feedback_for_planner: format!("The step {} timed out. Consider simplifying the operation or running in background.", tool),
            }
        } else if err_lower.contains("blocked") || err_lower.contains("denied by policy") || err_lower.contains("rm -rf /") {
            PlanFailure {
                kind: FailureKind::DangerousCommand,
                description: err_msg.to_string(),
                recoverability: Recoverability::Terminal,
                feedback_for_planner: format!("Terminal Safety Error: The command or tool {} was blocked. Do not attempt this action again.", tool),
            }
        } else if err_lower.contains("does not exist") || err_lower.contains("not found") || err_lower.contains("no such file") {
            PlanFailure {
                kind: FailureKind::PathHallucination,
                description: err_msg.to_string(),
                recoverability: Recoverability::Recoverable,
                feedback_for_planner: format!("Path Hallucination: The path specified in {} does not exist. Verify paths using read_dir or find before referencing them.", tool),
            }
        } else if err_lower.contains("permission denied") || err_lower.contains("access denied") {
            PlanFailure {
                kind: FailureKind::PermissionDenied,
                description: err_msg.to_string(),
                recoverability: Recoverability::StrategicRetry,
                feedback_for_planner: format!("Permission Denied: Access to resource in {} was denied. Attempt using a different approach or workspace directory.", tool),
            }
        } else if err_lower.contains("patch") || err_lower.contains("hunk failed") || err_lower.contains("conflict") {
            PlanFailure {
                kind: FailureKind::PatchFailed,
                description: err_msg.to_string(),
                recoverability: Recoverability::Recoverable,
                feedback_for_planner: format!("Patch Applicability Error: The unified patch failed to apply to the target file. Check lines, context, and use fresh read_file to regenerate accurate diff hunk headers."),
            }
        } else if err_lower.contains("dependency") || err_lower.contains("missing crate") || err_lower.contains("could not find") {
            PlanFailure {
                kind: FailureKind::DependencyUnmet,
                description: err_msg.to_string(),
                recoverability: Recoverability::Recoverable,
                feedback_for_planner: format!("Dependency Missing: A required package or library for {} is missing. Please add the dependency to Cargo.toml or equivalent config first.", tool),
            }
        } else if err_lower.contains("cargo check failed") || err_lower.contains("test failed") || err_lower.contains("assertion failed") {
            PlanFailure {
                kind: FailureKind::LogicalFailure,
                description: err_msg.to_string(),
                recoverability: Recoverability::Recoverable,
                feedback_for_planner: format!("Logical Verification Gate failed: Cargo check or test validation was not satisfied after running {}. Refine the code logic and ensure correctness.", tool),
            }
        } else {
            PlanFailure {
                kind: FailureKind::ToolExecutionError,
                description: err_msg.to_string(),
                recoverability: Recoverability::StrategicRetry,
                feedback_for_planner: format!("Tool runtime error in {}: {}. Retry using alternative tools.", tool, err_msg),
            }
        }
    }
}

// ═══════════════════════════════════════════════════════
// Plan Generation (World Model Planner V2 with Schema-Force)
// ═══════════════════════════════════════════════════════

/// Generate candidate plans using the model with schema-forced tool output.
pub async fn generate_candidate_plans(
    agent: &Agent,
    task: &str,
    context: &str,
    max_plans: usize,
) -> Result<Vec<CandidatePlan>> {
    let tool_def = crate::model::ToolDefinition {
        r#type: "function".to_string(),
        function: crate::model::FunctionDefinition {
            name: "plan_generation".to_string(),
            description: "Submit a structured and optimized tree action plan containing candidate execution steps for the World Model.".to_string(),
            parameters: crate::orchestration::plan_generation::PlanGenerationTool::new().parameters(),
        },
    };

    let model = {
        let m = agent.model.lock().await;
        (*m).clone()
    };

    let mut plans = Vec::new();

    for i in 1..=max_plans {
        log::info!("Generating plan variation #{} using schema-forced tool call...", i);
        let prompt = format!(
            "You are a strategic planner for an autonomous coding agent. Generate candidate action plan variant #{}.\n\
             You MUST call the 'plan_generation' tool to submit your structured action plan with AST nodes.\n\
             \n\
             Task: {}\n\
             \n\
             Context: {}",
             i, task, context
        );

        let request = CompletionRequest {
            messages: vec![Message {
                role: "user".to_string(),
                content: Some(MessageContent::Text(prompt)),
                ..Default::default()
            }],
            temperature: Some(0.3 + (i as f32) * 0.1), // Vary temperature for diverse candidates
            max_tokens: Some(1536), // reduced from 3072 — plans don't need that many tokens
            tools: Some(vec![tool_def.clone()]),
        };

        match model.complete(request).await {
            Ok(response) => {
                let mut parsed_plan = None;
                if let Some(tool_calls) = response.tool_calls {
                    for tc in tool_calls {
                        if tc.function.name == "plan_generation" {
                            if let Ok(p) = serde_json::from_str::<CandidatePlan>(&tc.function.arguments) {
                                parsed_plan = Some(p);
                                break;
                            }
                        }
                    }
                }

                // Fallback: parse raw text content
                if parsed_plan.is_none() {
                    if let Some(text) = response.content.as_ref().and_then(|c| c.as_text()) {
                        let json_text = if let Some(start) = text.find('{') {
                            let end = text.rfind('}').unwrap_or(text.len());
                            &text[start..end + 1]
                        } else {
                            text
                        };
                        if let Ok(p) = serde_json::from_str::<CandidatePlan>(json_text.trim()) {
                            parsed_plan = Some(p);
                        }
                    }
                }

                if let Some(mut plan) = parsed_plan {
                    plan.id = format!("plan-{}", i);
                    plans.push(plan);
                }
            }
            Err(e) => {
                log::warn!("Plan generation variant #{} failed: {}", i, e);
            }
        }
    }

    if plans.is_empty() {
        return Err(anyhow!("Zero plans generated successfully"));
    }

    log::info!("WorldModel V2: generated {} schema-compliant plan(s)", plans.len());
    Ok(plans)
}

// ═══════════════════════════════════════════════════════
// Rigorous Bayesian Scoring
// ═══════════════════════════════════════════════════════

fn calculate_static_bayesian_score(
    agent: &Agent,
    plan: &CandidatePlan,
    ast: &PlanNode,
    valid: bool,
) -> (f64, f64) {
    let prior_prob = {
        let lib = agent.skill_library.lock().unwrap();
        let category = match ast {
            PlanNode::Step { tool, .. } => tool.clone(),
            _ => "compound".to_string(),
        };
        let matches: Vec<_> = lib.entries.iter().filter(|e| e.category == category).collect();
        if matches.is_empty() {
            0.8
        } else {
            let successes = matches.iter().filter(|e| e.label.is_success()).count() as f64;
            successes / matches.len() as f64
        }
    };

    let likelihood_multiplier = {
        let lib = agent.skill_library.lock().unwrap();
        let query_shots = lib.query_few_shots(&plan.description, 3);
        if query_shots.is_empty() {
            0.5
        } else {
            // Dynamic similarity metrics aggregation using genome success rates
            let total_score: f64 = query_shots.iter().map(|s| s.genome.success_rate as f64).sum();
            (total_score / query_shots.len() as f64).clamp(0.1, 1.0)
        }
    };

    // Rigorous Bayesian probability updates: P(Success | Sim)
    let posterior_prob = {
        let numerator = prior_prob * likelihood_multiplier;
        let denominator = numerator + (1.0 - prior_prob) * (1.0 - likelihood_multiplier);
        if denominator > 0.0 {
            (numerator / denominator).clamp(0.01, 0.99)
        } else {
            prior_prob
        }
    };

    // Calculate toolchain risk based on AST structure
    fn calculate_risk(node: &PlanNode) -> f64 {
        match node {
            PlanNode::Step { tool, .. } => {
                if tool == "shell" || tool == "codeact" {
                    0.15
                } else {
                    0.05
                }
            }
            PlanNode::Sequence { nodes } | PlanNode::Parallel { nodes } => {
                nodes.iter().map(calculate_risk).sum()
            }
            PlanNode::Conditional { then_branch, else_branch, .. } => {
                let mut r = calculate_risk(then_branch);
                if let Some(else_b) = else_branch {
                    r += calculate_risk(else_b);
                }
                r
            }
            PlanNode::Retry { node, .. } | PlanNode::Verify { node, .. } | PlanNode::Gate { node, .. } => {
                calculate_risk(node)
            }
        }
    }

    let toolchain_risk = calculate_risk(ast).min(0.4);
    let raw_score = (posterior_prob - toolchain_risk).max(0.0);

    let final_score = if valid {
        raw_score + 0.3
    } else {
        raw_score * 0.2
    };

    // Mathematical dynamic EVPI (Expected Value of Perfect Information)
    // Formula scales with the entropy of the probability estimate and computational cost bounds
    let entropy = -posterior_prob * posterior_prob.ln() - (1.0 - posterior_prob) * (1.0 - posterior_prob).ln();
    let safe_entropy = if entropy.is_nan() { 0.1 } else { entropy };
    let calculated_evpi = (safe_entropy * (plan.estimated_tokens as f64 / 1000.0)).clamp(0.01, 2.5);

    (final_score, calculated_evpi)
}

// ═══════════════════════════════════════════════════════
// Real-world Execution and Commit with Rollback
// ═══════════════════════════════════════════════════════

pub fn execute_node<'a>(
    agent: &'a Agent,
    node: &'a PlanNode,
    workspace_root: &'a Path,
    snapshotted_files: &'a mut Vec<(PathBuf, String)>,
) -> BoxFuture<'a, Result<String>> {
    async move {
        match node {
            PlanNode::Step { tool, args, dry_run_first } => {
                // 1. 🛑 CodeActGate Pre-Flight Interceptor & Redirect Gate:
                // Auto-route low-level shell commands or codeact wrappers to high-performance, safe tools!
                use crate::orchestration::tool_scheduler::{CodeActGate, RedirectTarget};
                if let Some(redirect) = CodeActGate::should_redirect(tool, args) {
                    let redirected_node = match redirect {
                        RedirectTarget::ListDir { path } => {
                            log::info!("⚡ CodeActGate: Intercepted raw '{}' execution. Redirecting to structured, high-performance 'list_dir' tool.", tool);
                            PlanNode::Step {
                                tool: "list_dir".to_string(),
                                args: serde_json::json!({ "path": path }),
                                dry_run_first: *dry_run_first,
                            }
                        }
                        RedirectTarget::ReadFile { path } => {
                            log::info!("⚡ CodeActGate: Intercepted raw '{}' execution. Redirecting to structured 'view_file' tool.", tool);
                            PlanNode::Step {
                                tool: "view_file".to_string(),
                                args: serde_json::json!({ "path": path }),
                                dry_run_first: *dry_run_first,
                            }
                        }
                        RedirectTarget::GrepFiles { query, path } => {
                            log::info!("⚡ CodeActGate: Intercepted raw '{}' execution. Redirecting to structured 'grep_search' tool.", tool);
                            PlanNode::Step {
                                tool: "grep_search".to_string(),
                                args: serde_json::json!({ "query": query, "path": path }),
                                dry_run_first: *dry_run_first,
                            }
                        }
                    };
                    return execute_node(agent, &redirected_node, workspace_root, snapshotted_files).await;
                }

                // 2. 🛡️ Tool Scheduler Pre-execute: Enforce exploration budget and cooldown/co-relation policies!
                if let Err(e) = agent.tool_scheduler.pre_execute(tool, args) {
                    log::warn!("🚫 ToolScheduler Blocked Execution: {}", e);
                    return Err(e);
                }

                if tool == "write_file" || tool == "apply_patch" {
                    if let Some(path_str) = args.get("path").and_then(|s| s.as_str()) {
                        let resolved = resolve_path(workspace_root, path_str);
                        if resolved.exists() {
                            if let Ok(snap_id) = agent.snapshot_store.snapshot_file(&resolved).await {
                                snapshotted_files.push((resolved, snap_id));
                            }
                        }
                    }
                }

                let result = match tool.as_str() {
                    "codeact" => {
                        let engine = crate::orchestration::codeact::CodeActEngine::new(workspace_root.to_path_buf());
                        let script = args.get("script").and_then(|s| s.as_str()).unwrap_or("");
                        let script_cloned = script.to_string();

                        // ⏱️ Problem ①: Enforce strict CodeAct timeout in spawn_blocking task
                        let timeout_res = tokio::time::timeout(std::time::Duration::from_secs(30), async move {
                            tokio::task::spawn_blocking(move || {
                                engine.execute(&script_cloned)
                            }).await
                        }).await;

                        match timeout_res {
                            Ok(Ok(r)) => {
                                if r.success { Ok(r.output) } else { Err(anyhow!(r.error.unwrap_or_default())) }
                            }
                            Ok(Err(join_err)) => Err(anyhow!("CodeAct execution task panicked: {}", join_err)),
                            Err(_) => Err(anyhow!("CodeAct execution timed out after 30 seconds")),
                        }
                    }
                    "shell" => {
                        let cmd = args.get("command").and_then(|s| s.as_str()).unwrap_or("");
                        let cmd_future = tokio::process::Command::new("sh").arg("-c").arg(cmd)
                            .current_dir(workspace_root)
                            .output();

                        // ⏱️ Problem ①: Enforce strict Shell timeout
                        match tokio::time::timeout(std::time::Duration::from_secs(30), cmd_future).await {
                            Ok(Ok(output)) => {
                                if output.status.success() {
                                    Ok(String::from_utf8_lossy(&output.stdout).to_string())
                                } else {
                                    Err(anyhow!("shell failed: {}", String::from_utf8_lossy(&output.stderr)))
                                }
                            }
                            Ok(Err(e)) => Err(anyhow!("shell execution failed: {}", e)),
                            Err(_) => Err(anyhow!("shell execution timed out after 30 seconds")),
                        }
                    }
                    _ => {
                        let t_opt = {
                            let mut reg = agent.registry.lock().await;
                            reg.hydrate(tool)
                        };
                        match t_opt {
                            Some(t) => {
                                t.call(args.clone()).await
                                    .map_err(|e| anyhow!("{}", e.0))
                            }
                            None => Err(anyhow!("Tool not found: {}", tool)),
                        }
                    }
                };
                result
            }
            PlanNode::Sequence { nodes } => {
                let mut last_out = String::new();
                for child in nodes {
                    last_out = execute_node(agent, child, workspace_root, snapshotted_files).await?;
                }
                Ok(last_out)
            }
            PlanNode::Parallel { nodes } => {
                // Transactional Resource Locking / Write-Set Collision predictions to prevent race condition hell!
                let mut write_sets = std::collections::HashSet::new();
                let mut has_collision = false;

                for node in nodes {
                    fn extract_write_paths(n: &PlanNode, root: &Path, paths: &mut std::collections::HashSet<PathBuf>) {
                        match n {
                            PlanNode::Step { tool, args, .. } => {
                                if tool == "write_file" || tool == "apply_patch" {
                                    if let Some(path_str) = args.get("path").and_then(|s| s.as_str()) {
                                        paths.insert(resolve_path(root, path_str));
                                    }
                                }
                            }
                            PlanNode::Sequence { nodes } | PlanNode::Parallel { nodes } => {
                                for c in nodes {
                                    extract_write_paths(c, root, paths);
                                }
                            }
                            PlanNode::Conditional { then_branch, else_branch, .. } => {
                                extract_write_paths(then_branch, root, paths);
                                if let Some(e) = else_branch {
                                    extract_write_paths(e, root, paths);
                                }
                            }
                            PlanNode::Retry { node, .. } | PlanNode::Verify { node, .. } | PlanNode::Gate { node, .. } => {
                                extract_write_paths(node, root, paths);
                            }
                        }
                    }

                    let mut node_write_set = std::collections::HashSet::new();
                    extract_write_paths(node, workspace_root, &mut node_write_set);

                    for p in node_write_set {
                        if !write_sets.insert(p) {
                            has_collision = true;
                        }
                    }
                }

                if has_collision {
                    log::warn!("Parallel execution resource collision predicted (Race condition danger)! Serializing execution safely.");
                    let mut combined = String::new();
                    for child in nodes {
                        let out = execute_node(agent, child, workspace_root, snapshotted_files).await?;
                        combined.push_str(&out);
                        combined.push('\n');
                    }
                    return Ok(combined);
                }

                // Real concurrent parallel execution using tokio::spawn multithreading!
                let mut handles = Vec::new();
                for child in nodes {
                    let agent_cloned = agent.clone();
                    let child_cloned = child.clone();
                    let workspace_cloned = workspace_root.to_path_buf();
                    
                    let handle = tokio::spawn(async move {
                        let mut local_snapshots = Vec::new();
                        let res = execute_node(&agent_cloned, &child_cloned, &workspace_cloned, &mut local_snapshots).await;
                        (res, local_snapshots)
                    });
                    handles.push(handle);
                }

                let results = futures::future::join_all(handles).await;
                let mut combined = String::new();
                
                for res_wrap in results {
                    match res_wrap {
                        Ok((Ok(out), local_snaps)) => {
                            combined.push_str(&out);
                            combined.push('\n');
                            // Propagate snapshots up to the transactional system
                            for (path, snap_id) in local_snaps {
                                snapshotted_files.push((path, snap_id));
                            }
                        }
                        Ok((Err(err), local_snaps)) => {
                            // Rollback local snapshots immediately
                            for (path, snap_id) in local_snaps {
                                agent.snapshot_store.restore(&snap_id, &path).await.ok();
                            }
                            return Err(err);
                        }
                        Err(join_err) => {
                            return Err(anyhow!("Parallel spawn task panicked: {}", join_err));
                        }
                    }
                }
                Ok(combined)
            }
            PlanNode::Conditional { condition_script, then_branch, else_branch } => {
                let engine = crate::orchestration::codeact::CodeActEngine::new(workspace_root.to_path_buf());
                let r = engine.execute(condition_script);
                if r.success && r.output.trim() == "true" {
                    execute_node(agent, then_branch, workspace_root, snapshotted_files).await
                } else if let Some(else_b) = else_branch {
                    execute_node(agent, else_b, workspace_root, snapshotted_files).await
                } else {
                    Ok("Condition false, skipped".to_string())
                }
            }
            PlanNode::Retry { node, max_attempts } => {
                let mut last_err = anyhow!("Retry limit 0");
                for attempt in 1..=*max_attempts {
                    log::info!("Executing AST node (attempt {}/{})", attempt, max_attempts);
                    match execute_node(agent, node, workspace_root, snapshotted_files).await {
                        Ok(out) => return Ok(out),
                        Err(e) => {
                            last_err = e;
                        }
                    }
                }
                Err(last_err)
            }
            PlanNode::Verify { node, assertion_script } => {
                let out = execute_node(agent, node, workspace_root, snapshotted_files).await?;
                if assertion_script == "cargo_success" {
                    if run_cargo_check(workspace_root).await {
                        Ok(out)
                    } else {
                        Err(anyhow!("Verification failed: cargo check broke"))
                    }
                } else {
                    let engine = crate::orchestration::codeact::CodeActEngine::new(workspace_root.to_path_buf());
                    let r = engine.execute(assertion_script);
                    if r.success && r.output.trim() == "true" {
                        Ok(out)
                    } else {
                        Err(anyhow!("Verification failed"))
                    }
                }
            }
            PlanNode::Gate { gate_name, node } => {
                log::info!("Passing gate: {}", gate_name);
                execute_node(agent, node, workspace_root, snapshotted_files).await
            }
        }
    }.boxed()
}

pub async fn execute_world_model(
    agent: &Agent,
    _session_id: &str,
    task: &str,
) -> Result<String> {
    let workspace_root = std::env::current_dir().unwrap_or_default();
    let fingerprint = get_environment_fingerprint(&workspace_root);

    let mut cache = PlanCache::load();
    let mut previous_failure_feedbacks = Vec::new();

    // Re-planning / Reflect retry loops (fixes FailureTaxonomy feedback disappearing)
    for replan_attempt in 1..=3 {
        log::info!("WorldModel V2: Planning & Evaluation attempt {}/3...", replan_attempt);

        let mut active_plan = None;

        // 1. Check AOT compiled binaries first (absolute highest performance path)
        let reloader = crate::orchestration::aot::AotHotReloader::new();
        if let Some(aot_plan) = reloader.try_hot_load(task) {
            log::info!("WorldModel V2: AOT pre-compiled binary loaded for task: '{}'", task);
            active_plan = Some(aot_plan);
        } else {
            // 2. Look up cached plan or template pattern
            if let Some(cached) = cache.find_plan(task, &fingerprint) {
                log::info!("WorldModel V2: cached plan found for task: '{}'", task);
                active_plan = Some(cached);
            } else {
                let pattern_lib = crate::orchestration::pattern_miner::PatternLibrary::load();
                if let Some(instantiated) = pattern_lib.instantiate_match(task) {
                    log::info!("WorldModel V2: mined template matched and instantiated for task: '{}'", task);
                    active_plan = Some(instantiated);
                }
            }
        }

        // Incorporate historical breakage context feedback dynamically
        let cumulative_context = if previous_failure_feedbacks.is_empty() {
            "".to_string()
        } else {
            format!(
                "⚠️ CRITICAL PREVIOUS ATTEMPT FAILURE FEEDBACKS:\n{}\n\
                 Analyze these failure modes and do not generate matching failing AST patterns.",
                previous_failure_feedbacks.join("\n")
            )
        };

        let plans = if let Some(plan) = active_plan {
            vec![plan]
        } else {
            generate_candidate_plans(agent, task, &cumulative_context, 3).await?
        };

        // 2. Perform static verification, compilation and static scoring (no simulation/cloning!)
        let mut validations = Vec::new();
        
        // Strict dynamic risk ceiling setting to enforce toolchains limits!
        let verifier = StaticVerifier::new(0.75);
        let compiler = crate::orchestration::PlanCompiler::new();

        for mut plan in plans {
            let raw_ast = plan.get_ast();
            
            // Compile and optimize AST!
            let compiled_ast = compiler.compile(raw_ast);
            plan.root = Some(compiled_ast.clone());

            let verify_issues = verifier.verify(&compiled_ast, &workspace_root).await?;
            let valid = verify_issues.is_empty();

            if !valid {
                log::warn!("Plan '{}' failed pre-execution verification: {:?}", plan.id, verify_issues);
                continue;
            }

            let (score, evpi) = calculate_static_bayesian_score(agent, &plan, &compiled_ast, valid);

            let val = PlanValidation {
                plan_id: plan.id.clone(),
                valid,
                issues: verify_issues,
                score,
                token_cost: compiler.estimate_token_cost(&compiled_ast),
                evpi, // Calculated dynamically using Bayesian estimate entropy
            };

            validations.push((plan, val));
        }

        // Sort by score descending
        validations.sort_by(|a, b| b.1.score.partial_cmp(&a.1.score).unwrap_or(std::cmp::Ordering::Equal));

        if validations.is_empty() {
            log::warn!("All candidate plan verifications failed on attempt {}", replan_attempt);
            previous_failure_feedbacks.push(format!("Attempt {} failed: Plan validation checks rejected all candidates.", replan_attempt));
            continue;
        }

        log::info!(
            "WorldModel V2: Selected best compiled plan '{}' with static score {:.3} (EVPI: {:.4})",
            validations[0].0.id,
            validations[0].1.score,
            validations[0].1.evpi
        );

        // 3. Speculative Parallel Execution (Phase 8)
        if validations.len() >= 2 {
            log::info!("WorldModel V2: Multi-candidate plans available. Enabling Speculative Parallel Execution!");
            let executor = crate::orchestration::speculative::SpeculativeExecutor::new(
                crate::orchestration::speculative::SpeculativeMode::WorkspaceSandbox,
                &workspace_root
            );
            let plan_a = validations[0].0.clone();
            let plan_b = validations[1].0.clone();
            match executor.execute_speculative(agent, plan_a, plan_b).await {
                Ok(spec_res) => {
                    log::info!("WorldModel V2: Speculative parallel execution succeeded!");
                    cache.record_result(task, &validations[0].0, true, fingerprint.clone());
                    return Ok(format!(
                        "✅ Speculative Parallel Execution finished successfully:\n{}",
                        spec_res
                    ));
                }
                Err(e) => {
                    log::warn!("WorldModel V2: Speculative parallel execution failed ({}). Falling back to sequential execution.", e);
                }
            }
        }

        // Checkpoint the Git status checkpoint to ensure Forensic Rollbacks
        let git_checkpoint_untracked_modified = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&workspace_root)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        if !git_checkpoint_untracked_modified.is_empty() {
            log::info!("Forensic Checkpoint: repository has uncommitted mutations:\n{}", git_checkpoint_untracked_modified);
        }

        // 4. Execution of the best plan sequentially
        let mut fallback_error = "Execution failed".to_string();

        for (plan, _val) in &validations {
            let ast = plan.get_ast();
            let mut snapshotted_files = Vec::new();

            log::info!("WorldModel V2: executing compiled plan '{}' on real workspace", plan.id);
            let exec_result = execute_node(agent, &ast, &workspace_root, &mut snapshotted_files).await;

            let cargo_ok = run_cargo_check(&workspace_root).await;

            if exec_result.is_ok() && cargo_ok {
                log::info!("WorldModel V2: Plan '{}' executed successfully", plan.id);
                cache.record_result(task, plan, true, fingerprint.clone());
                return Ok(format!(
                    "✅ Plan '{}' executed successfully: {}\nOutput: {}",
                    plan.id, plan.description, exec_result.unwrap()
                ));
            }

            // Classification & forensic rollback on failure
            let error_msg = match &exec_result {
                Err(e) => e.to_string(),
                Ok(_) => "cargo check failed after execution".to_string(),
            };

            let failure = FailureKind::classify(&error_msg, "AST Executor");
            log::warn!(
                "WorldModel V2: Plan '{}' failed real-world execution. Error classified as {:?}. Recoverability: {:?}",
                plan.id, failure.kind, failure.recoverability
            );

            // Complete Forensic Rollback — revert all file system mutations!
            log::warn!("WorldModel V2: Initiating Forensic Rollback to remove all Ghost-Effects...");
            for (path, snap_id) in &snapshotted_files {
                if let Err(e) = agent.snapshot_store.restore(snap_id, path).await {
                    log::error!("WorldModel V2: Snapshot rollback failed for {}: {}", path.display(), e);
                }
            }

            // Revert all modified and untracked files to absolute pristine repository topology state!
            std::process::Command::new("git")
                .args(["checkout", "."])
                .current_dir(&workspace_root)
                .output()
                .ok();
            std::process::Command::new("git")
                .args(["clean", "-fd"])
                .current_dir(&workspace_root)
                .output()
                .ok();

            cache.record_result(task, plan, false, fingerprint.clone());

            fallback_error = failure.feedback_for_planner.clone();

            if matches!(failure.recoverability, Recoverability::Terminal) {
                return Err(anyhow!("Plan execution aborted (Terminal failure): {}", error_msg));
            }
        }

        // Register the feedback of this attempt failure mode to use during the next replanning loop!
        previous_failure_feedbacks.push(format!(
            "Failed Plan Option on attempt {}: {}",
            replan_attempt, fallback_error
        ));
    }

    Err(anyhow!(
        "All candidate plans and replanning attempts failed in the real workspace. Previous Failures:\n{}",
        previous_failure_feedbacks.join("\n")
    ))
}

// ═══════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════

async fn run_cargo_check(dir: &Path) -> bool {
    tokio::process::Command::new("cargo")
        .arg("check")
        .current_dir(dir)
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn resolve_path(root: &Path, path: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() { p.to_path_buf() } else { root.join(p) }
}

// ═══════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_serialization() {
        let plan = CandidatePlan {
            id: "test-1".into(),
            description: "Test plan".into(),
            estimated_tokens: 500,
            steps: vec![],
            root: Some(PlanNode::Step {
                tool: "codeact".into(),
                args: serde_json::json!({"script": "print('hello')"}),
                dry_run_first: false,
            }),
        };
        let json = serde_json::to_string(&plan).unwrap();
        let parsed: CandidatePlan = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "test-1");
    }

    #[test]
    fn test_resolve_path() {
        let root = Path::new("/workspace");
        assert_eq!(resolve_path(root, "src/main.rs"), PathBuf::from("/workspace/src/main.rs"));
        assert_eq!(resolve_path(root, "/tmp/absolute.rs"), PathBuf::from("/tmp/absolute.rs"));
    }

    #[tokio::test]
    async fn test_semantic_plan_cache() {
        let mut cache = PlanCache::default();
        let plan = CandidatePlan {
            id: "plan-login".into(),
            description: "Implement user authentication login endpoint".into(),
            estimated_tokens: 300,
            steps: vec![],
            root: Some(PlanNode::Step {
                tool: "write_file".into(),
                args: serde_json::json!({"path": "src/login.rs"}),
                dry_run_first: false,
            }),
        };
        cache.record_result("implement user authentication login endpoint", &plan, true, "hash123".to_string());

        // Now query with a semantically similar task
        let found = cache.find_plan("create user login authentication endpoint", "hash123");
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "plan-login");

        // Now query with an unrelated task
        let not_found = cache.find_plan("format disk and partition database table", "hash123");
        assert!(not_found.is_none());
    }
}
