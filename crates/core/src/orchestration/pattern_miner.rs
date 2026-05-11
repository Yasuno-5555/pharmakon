//! 🕸️ Cross-Task Pattern Mining — Phase 8
//!
//! Analyzes historically successful plans from PlanCache, discovers structural similarity,
//! abstracts differences into dynamic parameter templates, and instantiates pre-verified plans
//! for future matching tasks.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use regex::Regex;

use crate::orchestration::world::{CandidatePlan, PlanNode, PlanCache};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternTemplate {
    pub id: String,
    pub template_key: String,       // e.g. "add logging to {param_0}"
    pub task_regex: String,         // e.g. r"^add logging to (?P<param_0>[a-zA-Z0-9_\-\.]+)$"
    pub parameter_keys: Vec<String>, // e.g. ["param_0"]
    pub root_node: PlanNode,         // AST node structure with "{param_0}" placeholders
    pub frequency: u32,
    pub success_rate: f64,
    pub score: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PatternLibrary {
    pub templates: Vec<PatternTemplate>,
}

impl PatternLibrary {
    pub fn load() -> Self {
        let path = dirs::home_dir().unwrap_or_default().join(".pharmakon/pattern_library.json");
        if path.exists()
            && let Ok(content) = std::fs::read_to_string(path)
                && let Ok(lib) = serde_json::from_str(&content) {
                    return lib;
                }
        Self::default()
    }

    pub fn save(&self) -> Result<()> {
        let path = dirs::home_dir().unwrap_or_default().join(".pharmakon/pattern_library.json");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Attempts to match a task description against our compiled regex templates and instantiate an AST.
    pub fn instantiate_match(&self, task: &str) -> Option<CandidatePlan> {
        for template in &self.templates {
            if let Ok(re) = Regex::new(&template.task_regex)
                && let Some(captures) = re.captures(task) {
                    let mut params = HashMap::new();
                    for key in &template.parameter_keys {
                        if let Some(m) = captures.name(key) {
                            params.insert(key.clone(), m.as_str().to_string());
                        }
                    }

                    // Perform parameter substitution on the root node
                    let instantiated_root = substitute_node(&template.root_node, &params);

                    return Some(CandidatePlan {
                        id: format!("instantiated-{}", &uuid::Uuid::new_v4().to_string()[..8]),
                        description: format!("Instantiated template for: {}", task),
                        estimated_tokens: 100, // Pre-optimized templated path
                        steps: Vec::new(),
                        root: Some(instantiated_root),
                    });
                }
        }
        None
    }
}

/// Recursively substitute parameter placeholders (e.g. "{param_0}") in a PlanNode.
fn substitute_node(node: &PlanNode, params: &HashMap<String, String>) -> PlanNode {
    match node {
        PlanNode::Script { language, code, timeout_secs } => {
            let substituted_code = substitute_string(code, params);
            PlanNode::Script {
                language: *language,
                code: substituted_code,
                timeout_secs: *timeout_secs,
            }
        }
        PlanNode::Step { tool, args, dry_run_first } => {
            let substituted_args = substitute_json_value(args, params);
            PlanNode::Step {
                tool: tool.clone(),
                args: substituted_args,
                dry_run_first: *dry_run_first,
            }
        }
        PlanNode::Sequence { nodes } => {
            let substituted = nodes.iter().map(|n| substitute_node(n, params)).collect();
            PlanNode::Sequence { nodes: substituted }
        }
        PlanNode::Parallel { nodes } => {
            let substituted = nodes.iter().map(|n| substitute_node(n, params)).collect();
            PlanNode::Parallel { nodes: substituted }
        }
        PlanNode::Conditional { condition, then_branch, else_branch } => {
            let sub_condition = match condition {
                crate::orchestration::world::Condition::FileExists { path } => {
                    let path_str = substitute_string(&path.to_string_lossy(), params);
                    crate::orchestration::world::Condition::FileExists { path: std::path::PathBuf::from(path_str) }
                }
                crate::orchestration::world::Condition::CargoCheckSuccess => {
                    crate::orchestration::world::Condition::CargoCheckSuccess
                }
                crate::orchestration::world::Condition::VerifySuccess { strategy } => {
                    let sub_strategy = strategy.as_ref().map(|s| match s {
                        crate::orchestration::world::VerifyStrategy::Shell(cmd) => {
                            crate::orchestration::world::VerifyStrategy::Shell(substitute_string(cmd, params))
                        }
                        _ => s.clone(),
                    });
                    crate::orchestration::world::Condition::VerifySuccess { strategy: sub_strategy }
                }
                crate::orchestration::world::Condition::Script { script } => {
                    crate::orchestration::world::Condition::Script {
                        script: substitute_string(script, params),
                    }
                }
                crate::orchestration::world::Condition::Legacy(script) => {
                    crate::orchestration::world::Condition::Legacy(substitute_string(script, params))
                }
            };
            PlanNode::Conditional {
                condition: sub_condition,
                then_branch: Box::new(substitute_node(then_branch, params)),
                else_branch: else_branch.as_ref().map(|b| Box::new(substitute_node(b, params))),
            }
        }
        PlanNode::Retry { node, max_attempts } => {
            PlanNode::Retry {
                node: Box::new(substitute_node(node, params)),
                max_attempts: *max_attempts,
            }
        }
        PlanNode::Verify { node, assertion_script } => {
            PlanNode::Verify {
                node: Box::new(substitute_node(node, params)),
                assertion_script: substitute_string(assertion_script, params),
            }
        }
        PlanNode::Gate { gate_name, node } => {
            PlanNode::Gate {
                gate_name: substitute_string(gate_name, params),
                node: Box::new(substitute_node(node, params)),
            }
        }
    }
}

fn substitute_string(text: &str, params: &HashMap<String, String>) -> String {
    let mut result = text.to_string();
    for (k, v) in params {
        let placeholder = format!("{{{}}}", k);
        result = result.replace(&placeholder, v);
    }
    result
}

fn substitute_json_value(val: &serde_json::Value, params: &HashMap<String, String>) -> serde_json::Value {
    match val {
        serde_json::Value::String(s) => serde_json::Value::String(substitute_string(s, params)),
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(|v| substitute_json_value(v, params)).collect())
        }
        serde_json::Value::Object(obj) => {
            let mut new_obj = serde_json::Map::new();
            for (k, v) in obj {
                new_obj.insert(k.clone(), substitute_json_value(v, params));
            }
            serde_json::Value::Object(new_obj)
        }
        _ => val.clone(),
    }
}

pub struct PatternMiner;

impl Default for PatternMiner {
    fn default() -> Self {
        Self::new()
    }
}

impl PatternMiner {
    pub fn new() -> Self {
        Self
    }

    /// Automatically discover recurring task patterns from historically successful cache entries.
    pub fn mine_patterns(&self, plan_cache: &PlanCache) -> PatternLibrary {
        let mut library = PatternLibrary::default();

        // Filter successful plans
        let successful_plans: Vec<_> = plan_cache.entries.iter()
            .filter(|e| e.success_count > 0 && e.plan.root.is_some())
            .collect();

        if successful_plans.len() < 2 {
            return library; // Need at least two successful tasks to draw comparison
        }

        let mut discovered: HashMap<String, PatternTemplate> = HashMap::new();

        // Compare pairs of tasks to find abstractions
        for i in 0..successful_plans.len() {
            for j in (i + 1)..successful_plans.len() {
                let plan_a = successful_plans[i];
                let plan_b = successful_plans[j];

                // Verify structural match (e.g. same sequence of tools)
                if !is_structurally_equal(plan_a.plan.root.as_ref(), plan_b.plan.root.as_ref()) {
                    continue;
                }

                if let Some((template_key, regex_pattern, param_map)) = abstract_task_descriptions(&plan_a.task, &plan_b.task) {
                    let root_node = plan_a.plan.root.as_ref().unwrap();

                    // Generalize string arguments in the AST node
                    let generalized_root = generalize_node(root_node, &param_map);

                    let freq = plan_a.success_count + plan_b.success_count;
                    let success_rate = (plan_a.success_rate() + plan_b.success_rate()) / 2.0;
                    let freshness = (plan_a.freshness() + plan_b.freshness()) / 2.0;
                    let score = freq as f64 * success_rate * freshness;

                    let id = format!("pattern-{}", &uuid::Uuid::new_v4().to_string()[..8]);
                    let template = PatternTemplate {
                        id,
                        template_key: template_key.clone(),
                        task_regex: regex_pattern,
                        parameter_keys: vec!["param_0".to_string()],
                        root_node: generalized_root,
                        frequency: freq,
                        success_rate,
                        score,
                    };

                    discovered.insert(template_key, template);
                }
            }
        }

        library.templates = discovered.into_values().collect();
        // Sort by score descending
        library.templates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        library
    }
}

/// Abstract AST node values containing the specific word with a dynamic placeholder
fn generalize_node(node: &PlanNode, params: &HashMap<String, String>) -> PlanNode {
    match node {
        PlanNode::Script { language, code, timeout_secs } => {
            PlanNode::Script {
                language: *language,
                code: generalize_string(code, params),
                timeout_secs: *timeout_secs,
            }
        }
        PlanNode::Step { tool, args, dry_run_first } => {
            PlanNode::Step {
                tool: tool.clone(),
                args: generalize_json_value(args, params),
                dry_run_first: *dry_run_first,
            }
        }
        PlanNode::Sequence { nodes } => {
            PlanNode::Sequence { nodes: nodes.iter().map(|n| generalize_node(n, params)).collect() }
        }
        PlanNode::Parallel { nodes } => {
            PlanNode::Parallel { nodes: nodes.iter().map(|n| generalize_node(n, params)).collect() }
        }
        PlanNode::Conditional { condition, then_branch, else_branch } => {
            let gen_condition = match condition {
                crate::orchestration::world::Condition::FileExists { path } => {
                    let path_str = generalize_string(&path.to_string_lossy(), params);
                    crate::orchestration::world::Condition::FileExists { path: std::path::PathBuf::from(path_str) }
                }
                crate::orchestration::world::Condition::CargoCheckSuccess => {
                    crate::orchestration::world::Condition::CargoCheckSuccess
                }
                crate::orchestration::world::Condition::VerifySuccess { strategy } => {
                    let gen_strategy = strategy.as_ref().map(|s| match s {
                        crate::orchestration::world::VerifyStrategy::Shell(cmd) => {
                            crate::orchestration::world::VerifyStrategy::Shell(generalize_string(cmd, params))
                        }
                        _ => s.clone(),
                    });
                    crate::orchestration::world::Condition::VerifySuccess { strategy: gen_strategy }
                }
                crate::orchestration::world::Condition::Script { script } => {
                    crate::orchestration::world::Condition::Script {
                        script: generalize_string(script, params),
                    }
                }
                crate::orchestration::world::Condition::Legacy(script) => {
                    crate::orchestration::world::Condition::Legacy(generalize_string(script, params))
                }
            };
            PlanNode::Conditional {
                condition: gen_condition,
                then_branch: Box::new(generalize_node(then_branch, params)),
                else_branch: else_branch.as_ref().map(|b| Box::new(generalize_node(b, params))),
            }
        }
        PlanNode::Retry { node, max_attempts } => {
            PlanNode::Retry {
                node: Box::new(generalize_node(node, params)),
                max_attempts: *max_attempts,
            }
        }
        PlanNode::Verify { node, assertion_script } => {
            PlanNode::Verify {
                node: Box::new(generalize_node(node, params)),
                assertion_script: generalize_string(assertion_script, params),
            }
        }
        PlanNode::Gate { gate_name, node } => {
            PlanNode::Gate {
                gate_name: generalize_string(gate_name, params),
                node: Box::new(generalize_node(node, params)),
            }
        }
    }
}

fn generalize_string(text: &str, params: &HashMap<String, String>) -> String {
    let mut result = text.to_string();
    for (val, placeholder) in params {
        result = result.replace(val, placeholder);
    }
    result
}

fn generalize_json_value(val: &serde_json::Value, params: &HashMap<String, String>) -> serde_json::Value {
    match val {
        serde_json::Value::String(s) => serde_json::Value::String(generalize_string(s, params)),
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(|v| generalize_json_value(v, params)).collect())
        }
        serde_json::Value::Object(obj) => {
            let mut new_obj = serde_json::Map::new();
            for (k, v) in obj {
                new_obj.insert(k.clone(), generalize_json_value(v, params));
            }
            serde_json::Value::Object(new_obj)
        }
        _ => val.clone(),
    }
}

/// Verify if two subtrees have the same sequence / layout of steps.
fn is_structurally_equal(a: Option<&PlanNode>, b: Option<&PlanNode>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(PlanNode::Script { language: l_a, .. }), Some(PlanNode::Script { language: l_b, .. })) => l_a == l_b,
        (Some(PlanNode::Step { tool: t_a, .. }), Some(PlanNode::Step { tool: t_b, .. })) => t_a == t_b,
        (Some(PlanNode::Sequence { nodes: n_a }), Some(PlanNode::Sequence { nodes: n_b })) => {
            n_a.len() == n_b.len() && n_a.iter().zip(n_b).all(|(x, y)| is_structurally_equal(Some(x), Some(y)))
        }
        (Some(PlanNode::Parallel { nodes: n_a }), Some(PlanNode::Parallel { nodes: n_b })) => {
            n_a.len() == n_b.len() && n_a.iter().zip(n_b).all(|(x, y)| is_structurally_equal(Some(x), Some(y)))
        }
        (Some(PlanNode::Conditional { then_branch: t_a, else_branch: e_a, .. }), Some(PlanNode::Conditional { then_branch: t_b, else_branch: e_b, .. })) => {
            is_structurally_equal(Some(t_a), Some(t_b)) && match (e_a, e_b) {
                (None, None) => true,
                (Some(x), Some(y)) => is_structurally_equal(Some(x), Some(y)),
                _ => false
            }
        }
        (Some(PlanNode::Retry { node: n_a, .. }), Some(PlanNode::Retry { node: n_b, .. })) => {
            is_structurally_equal(Some(n_a), Some(n_b))
        }
        (Some(PlanNode::Verify { node: n_a, .. }), Some(PlanNode::Verify { node: n_b, .. })) => {
            is_structurally_equal(Some(n_a), Some(n_b))
        }
        (Some(PlanNode::Gate { node: n_a, .. }), Some(PlanNode::Gate { node: n_b, .. })) => {
            is_structurally_equal(Some(n_a), Some(n_b))
        }
        _ => false,
    }
}

/// Compares two task sentences. If they are identical except for exactly one word position,
/// abstract that word into "{param_0}" and construct regex matching.
/// Returns (template_key, regex_pattern, value_to_placeholder_map).
fn abstract_task_descriptions(a: &str, b: &str) -> Option<(String, String, HashMap<String, String>)> {
    let tokens_a: Vec<&str> = a.split_whitespace().collect();
    let tokens_b: Vec<&str> = b.split_whitespace().collect();

    if tokens_a.len() != tokens_b.len() || tokens_a.is_empty() {
        return None;
    }

    let mut diff_idx = None;
    for i in 0..tokens_a.len() {
        if tokens_a[i] != tokens_b[i] {
            if diff_idx.is_some() {
                return None; // Differs by more than one word -> too different
            }
            diff_idx = Some(i);
        }
    }

    if let Some(idx) = diff_idx {
        let word_a = tokens_a[idx];
        let word_b = tokens_b[idx];

        // Ensure both look like variables/filenames (alphanumeric with dots/dashes/underscores)
        let is_valid_var = |w: &str| w.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_');
        if !is_valid_var(word_a) || !is_valid_var(word_b) {
            return None;
        }

        let mut template_tokens = tokens_a.clone();
        template_tokens[idx] = "{param_0}";
        let template_key = template_tokens.join(" ");

        let mut regex_tokens = Vec::new();
        for (i, token) in tokens_a.iter().enumerate() {
            if i == idx {
                regex_tokens.push(r"(?P<param_0>[a-zA-Z0-9_\-\.]+)".to_string());
            } else {
                regex_tokens.push(regex::escape(token));
            }
        }
        let regex_pattern = format!("^{}$", regex_tokens.join(r"\s+"));

        let mut map = HashMap::new();
        map.insert(word_a.to_string(), "{param_0}".to_string());
        map.insert(word_b.to_string(), "{param_0}".to_string());

        Some((template_key, regex_pattern, map))
    } else {
        None // Sentences are identical -> no abstraction needed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::world::CachedPlan;

    #[test]
    fn test_abstract_task_descriptions() {
        let res = abstract_task_descriptions("add logging to auth.rs", "add logging to main.rs").unwrap();
        assert_eq!(res.0, "add logging to {param_0}");
        assert_eq!(res.1, r"^add\s+logging\s+to\s+(?P<param_0>[a-zA-Z0-9_\-\.]+)$");
        assert_eq!(res.2.get("auth.rs").unwrap(), "{param_0}");
    }

    #[test]
    fn test_pattern_mining_and_instantiation() {
        let node_a = PlanNode::Step {
            tool: "write_file".to_string(),
            args: serde_json::json!({ "path": "auth.rs", "content": "hello" }),
            dry_run_first: false,
        };

        let node_b = PlanNode::Step {
            tool: "write_file".to_string(),
            args: serde_json::json!({ "path": "main.rs", "content": "hello" }),
            dry_run_first: false,
        };

        let mut cache = PlanCache::default();
        cache.entries.push(CachedPlan {
            plan_id: "plan-a".to_string(),
            task: "add logging to auth.rs".to_string(),
            plan: CandidatePlan {
                id: "plan-a".to_string(),
                description: "Plan A".to_string(),
                estimated_tokens: 100,
                steps: Vec::new(),
                root: Some(node_a),
            },
            created_at: chrono::Utc::now(),
            fingerprint: "test-f".to_string(),
            success_count: 5,
            failure_count: 0,
        });

        cache.entries.push(CachedPlan {
            plan_id: "plan-b".to_string(),
            task: "add logging to main.rs".to_string(),
            plan: CandidatePlan {
                id: "plan-b".to_string(),
                description: "Plan B".to_string(),
                estimated_tokens: 100,
                steps: Vec::new(),
                root: Some(node_b),
            },
            created_at: chrono::Utc::now(),
            fingerprint: "test-f".to_string(),
            success_count: 3,
            failure_count: 0,
        });

        let miner = PatternMiner::new();
        let lib = miner.mine_patterns(&cache);

        assert_eq!(lib.templates.len(), 1);
        let template = &lib.templates[0];
        assert_eq!(template.template_key, "add logging to {param_0}");

        // Now test instantiation on a brand new task description!
        let instantiated = lib.instantiate_match("add logging to utils.rs");
        assert!(instantiated.is_some());
        let plan = instantiated.unwrap();
        match plan.root.unwrap() {
            PlanNode::Step { tool, args, .. } => {
                assert_eq!(tool, "write_file");
                assert_eq!(args["path"].as_str().unwrap(), "utils.rs");
            }
            _ => panic!("Expected step node"),
        }
    }
}
