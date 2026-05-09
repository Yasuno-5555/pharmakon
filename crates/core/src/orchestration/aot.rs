//! ⚡ Ahead-of-Time (AOT) Plan Compilation — Phase 8
//!
//! Compiles high-frequency, highly successful AST templates into optimized serialized binaries
//! stored in the compilation cache (~/.pharmakon/compiled/) and provides utilities for
//! native Rust code crystallization and dynamic hot-reloading.

use anyhow::Result;
use std::path::PathBuf;

use crate::orchestration::pattern_miner::{PatternLibrary, PatternTemplate};
use crate::orchestration::world::CandidatePlan;

#[derive(Debug, Clone)]
pub struct AotCompiler {
    pub cache_dir: PathBuf,
    pub min_frequency: u32,
    pub min_success_rate: f64,
}

impl AotCompiler {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        Self {
            cache_dir: home.join(".pharmakon/compiled"),
            min_frequency: 50, // Default production threshold
            min_success_rate: 0.90,
        }
    }

    /// Alternate constructor for testing with lower thresholds
    pub fn new_with_thresholds(min_frequency: u32, min_success_rate: f64) -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        Self {
            cache_dir: home.join(".pharmakon/compiled"),
            min_frequency,
            min_success_rate,
        }
    }

    /// Compiles high-frequency, stable templates from the pattern library to disk as binaries.
    pub fn compile_and_cache(&self, pattern_lib: &PatternLibrary) -> Result<usize> {
        std::fs::create_dir_all(&self.cache_dir).ok();
        let mut count = 0;

        for template in &pattern_lib.templates {
            if template.frequency >= self.min_frequency && template.success_rate >= self.min_success_rate {
                let cache_file = self.cache_dir.join(format!("{}.bin", template.id));
                let serialized = serde_json::to_vec(template)?;
                std::fs::write(&cache_file, serialized)?;
                count += 1;
            }
        }

        Ok(count)
    }

    /// Generates native, crystallized Rust source code representing the template AST for compilation inside the binary hot path.
    pub fn generate_crystallized_rust(&self, template: &PatternTemplate) -> String {
        let mut code = String::new();
        code.push_str("//! 💎 Crystallized Native AST\n");
        code.push_str("//! Generated automatically by Pharmakon AOT Compiler.\n\n");
        code.push_str("use crate::orchestration::world::PlanNode;\n");
        code.push_str("use std::collections::HashMap;\n\n");
        code.push_str(&format!("/// Get crystallized plan for: {}\n", template.template_key));
        code.push_str(&format!("pub fn get_crystallized_{}() -> PlanNode {{\n", template.id.replace("-", "_")));
        code.push_str("    ");
        code.push_str(&format!("// Template Key: {}\n", template.template_key));
        code.push_str("    ");
        code.push_str(&format!("// Success Rate: {:.2}%, Frequency: {}\n", template.success_rate * 100.0, template.frequency));
        
        let root_code = format_plan_node_as_rust(&template.root_node, "    ");
        code.push_str(&format!("    {}\n", root_code));
        code.push_str("}\n");
        code
    }
}

/// Helper recursively converting a PlanNode AST into raw Rust syntax representation.
fn format_plan_node_as_rust(node: &crate::orchestration::world::PlanNode, indent: &str) -> String {
    let next_indent = format!("{}    ", indent);
    match node {
        crate::orchestration::world::PlanNode::Step { tool, args, dry_run_first } => {
            format!(
                "PlanNode::Step {{\n{}tool: \"{}\".to_string(),\n{}args: serde_json::json!({}),\n{}dry_run_first: {},\n{}}}",
                next_indent, tool, next_indent, args, next_indent, dry_run_first, indent
            )
        }
        crate::orchestration::world::PlanNode::Sequence { nodes } => {
            let elements: Vec<String> = nodes.iter().map(|n| format_plan_node_as_rust(n, &next_indent)).collect();
            format!(
                "PlanNode::Sequence {{\n{}nodes: vec![\n{}{}\n{}]\n{}}}",
                next_indent, next_indent, elements.join(&format!(",\n{}", next_indent)), next_indent, indent
            )
        }
        crate::orchestration::world::PlanNode::Parallel { nodes } => {
            let elements: Vec<String> = nodes.iter().map(|n| format_plan_node_as_rust(n, &next_indent)).collect();
            format!(
                "PlanNode::Parallel {{\n{}nodes: vec![\n{}{}\n{}]\n{}}}",
                next_indent, next_indent, elements.join(&format!(",\n{}", next_indent)), next_indent, indent
            )
        }
        crate::orchestration::world::PlanNode::Conditional { condition_script, then_branch, else_branch } => {
            let then_code = format_plan_node_as_rust(then_branch, &next_indent);
            let else_code = match else_branch {
                Some(b) => format!("Some(Box::new({}))", format_plan_node_as_rust(b, &next_indent)),
                None => "None".to_string(),
            };
            format!(
                "PlanNode::Conditional {{\n{}condition_script: \"{}\".to_string(),\n{}then_branch: Box::new({}),\n{}else_branch: {},\n{}}}",
                next_indent, condition_script, next_indent, then_code, next_indent, else_code, indent
            )
        }
        crate::orchestration::world::PlanNode::Retry { node, max_attempts } => {
            let inner = format_plan_node_as_rust(node, &next_indent);
            format!(
                "PlanNode::Retry {{\n{}node: Box::new({}),\n{}max_attempts: {},\n{}}}",
                next_indent, inner, next_indent, max_attempts, indent
            )
        }
        crate::orchestration::world::PlanNode::Verify { node, assertion_script } => {
            let inner = format_plan_node_as_rust(node, &next_indent);
            format!(
                "PlanNode::Verify {{\n{}node: Box::new({}),\n{}assertion_script: \"{}\".to_string(),\n{}}}",
                next_indent, inner, next_indent, assertion_script, indent
            )
        }
        crate::orchestration::world::PlanNode::Gate { gate_name, node } => {
            let inner = format_plan_node_as_rust(node, &next_indent);
            format!(
                "PlanNode::Gate {{\n{}gate_name: \"{}\".to_string(),\n{}node: Box::new({}),\n{}}}",
                next_indent, gate_name, next_indent, inner, indent
            )
        }
    }
}

pub struct AotHotReloader {
    pub cache_dir: PathBuf,
}

impl AotHotReloader {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        Self {
            cache_dir: home.join(".pharmakon/compiled"),
        }
    }

    /// Fast matcher that checks cached compiled binary templates before LLMs are contacted.
    pub fn try_hot_load(&self, task: &str) -> Option<CandidatePlan> {
        if !self.cache_dir.exists() {
            return None;
        }

        if let Ok(entries) = std::fs::read_dir(&self.cache_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "bin") {
                    if let Ok(bytes) = std::fs::read(&path) {
                        if let Ok(template) = serde_json::from_slice::<PatternTemplate>(&bytes) {
                            if let Ok(re) = regex::Regex::new(&template.task_regex) {
                                if let Some(captures) = re.captures(task) {
                                    let mut params = std::collections::HashMap::new();
                                    for key in &template.parameter_keys {
                                        if let Some(m) = captures.name(key) {
                                            params.insert(key.clone(), m.as_str().to_string());
                                        }
                                    }

                                    // Instantiates matching plan instantly!
                                    let instantiated_root = substitute_node(&template.root_node, &params);
                                    return Some(CandidatePlan {
                                        id: format!("aot-{}", &uuid::Uuid::new_v4().to_string()[..8]),
                                        description: format!("AOT compiled plan loaded for: {}", task),
                                        estimated_tokens: 10, // Maximum execution savings
                                        steps: Vec::new(),
                                        root: Some(instantiated_root),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        None
    }
}

fn substitute_node(node: &crate::orchestration::world::PlanNode, params: &std::collections::HashMap<String, String>) -> crate::orchestration::world::PlanNode {
    match node {
        crate::orchestration::world::PlanNode::Step { tool, args, dry_run_first } => {
            let sub_args = substitute_json_value(args, params);
            crate::orchestration::world::PlanNode::Step {
                tool: tool.clone(),
                args: sub_args,
                dry_run_first: *dry_run_first,
            }
        }
        crate::orchestration::world::PlanNode::Sequence { nodes } => {
            crate::orchestration::world::PlanNode::Sequence {
                nodes: nodes.iter().map(|n| substitute_node(n, params)).collect(),
            }
        }
        crate::orchestration::world::PlanNode::Parallel { nodes } => {
            crate::orchestration::world::PlanNode::Parallel {
                nodes: nodes.iter().map(|n| substitute_node(n, params)).collect(),
            }
        }
        crate::orchestration::world::PlanNode::Conditional { condition_script, then_branch, else_branch } => {
            crate::orchestration::world::PlanNode::Conditional {
                condition_script: substitute_string(condition_script, params),
                then_branch: Box::new(substitute_node(then_branch, params)),
                else_branch: else_branch.as_ref().map(|b| Box::new(substitute_node(b, params))),
            }
        }
        crate::orchestration::world::PlanNode::Retry { node, max_attempts } => {
            crate::orchestration::world::PlanNode::Retry {
                node: Box::new(substitute_node(node, params)),
                max_attempts: *max_attempts,
            }
        }
        crate::orchestration::world::PlanNode::Verify { node, assertion_script } => {
            crate::orchestration::world::PlanNode::Verify {
                node: Box::new(substitute_node(node, params)),
                assertion_script: substitute_string(assertion_script, params),
            }
        }
        crate::orchestration::world::PlanNode::Gate { gate_name, node } => {
            crate::orchestration::world::PlanNode::Gate {
                gate_name: substitute_string(gate_name, params),
                node: Box::new(substitute_node(node, params)),
            }
        }
    }
}

fn substitute_string(text: &str, params: &std::collections::HashMap<String, String>) -> String {
    let mut result = text.to_string();
    for (k, v) in params {
        let placeholder = format!("{{{}}}", k);
        result = result.replace(&placeholder, v);
    }
    result
}

fn substitute_json_value(val: &serde_json::Value, params: &std::collections::HashMap<String, String>) -> serde_json::Value {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::world::PlanNode;

    #[test]
    fn test_aot_compiler_and_reloader() {
        let root = PlanNode::Step {
            tool: "write_file".to_string(),
            args: serde_json::json!({ "path": "{param_0}", "content": "hello" }),
            dry_run_first: false,
        };

        let template = PatternTemplate {
            id: "test-template".to_string(),
            template_key: "write out {param_0}".to_string(),
            task_regex: r"^write\s+out\s+(?P<param_0>[a-zA-Z0-9_\-\.]+)$".to_string(),
            parameter_keys: vec!["param_0".to_string()],
            root_node: root,
            frequency: 5,
            success_rate: 0.95,
            score: 100.0,
        };

        let mut lib = PatternLibrary::default();
        lib.templates.push(template.clone());

        // Use custom threshold for testing
        let temp_cache_dir = std::env::temp_dir().join("pharmakon_test_compiled");
        std::fs::remove_dir_all(&temp_cache_dir).ok();

        let compiler = AotCompiler {
            cache_dir: temp_cache_dir.clone(),
            min_frequency: 1,
            min_success_rate: 0.90,
        };

        let compiled = compiler.compile_and_cache(&lib).unwrap();
        assert_eq!(compiled, 1);

        // Verify native Rust code generation syntax output
        let rust_code = compiler.generate_crystallized_rust(&template);
        assert!(rust_code.contains("pub fn get_crystallized_test_template()"));
        assert!(rust_code.contains("PlanNode::Step"));

        // Match loader
        let reloader = AotHotReloader {
            cache_dir: temp_cache_dir,
        };
        let hot_loaded = reloader.try_hot_load("write out index.js");
        assert!(hot_loaded.is_some());
        let plan = hot_loaded.unwrap();
        match plan.root.unwrap() {
            PlanNode::Step { args, .. } => {
                assert_eq!(args["path"].as_str().unwrap(), "index.js");
            }
            _ => panic!("Expected step node"),
        }
    }
}
