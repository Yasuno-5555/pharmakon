//! Rhai Skill Library — Self-improving CodeAct skill store.
//!
//! Architecture:
//!   Dream Mode (background) → LLM writes scripts → verify → label → store
//!   Success → KnowledgeNexus "acquired skill" + few-shot injection
//!   Failure → AntiPattern extraction + positive guidance injection
//!   Primitive Darwinism → experimental → stable → core → deprecated → removed
//!
//! 2027-2028 Roadmap concepts implemented:
//!   Skill Genome System — capability/failure/cost metadata per script
//!   Composite Skills — merge primitives into higher-order functions
//!   Trajectory Compression — extract safe_refactor() patterns from traces
//!   Skill Crystallization — suggest Rhai→Rust native translation

use pharmakon_common::agent_types::MessageContent;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

// ═══════════════════════════════════════════════════════════
// Data Structures
// ═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Label {
    Success { verified_by: String },
    ParseError { line: usize, message: String },
    RuntimeError { message: String },
    Timeout,
    WrongOutput { expected: String, got: String },
    Skipped { reason: String },
}

impl Label {
    pub fn is_success(&self) -> bool { matches!(self, Label::Success { .. }) }
    pub fn is_failure(&self) -> bool { !self.is_success() }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabeledScript {
    pub id: String,
    pub task_description: String,
    pub script: String,
    pub label: Label,
    pub category: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub function_signature: Option<String>,
    pub usage_count: usize,
    pub lifecycle: PrimitiveStage,
    pub genome: SkillGenome,
}

/// Skill Genome — quantitative metadata for Darwinian selection and composition.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillGenome {
    pub capabilities: Vec<String>,
    pub failure_modes: Vec<FailureMode>,
    pub token_cost: usize,
    pub cpu_micros: u64,
    pub success_rate: f32,
    pub run_count: usize,
    pub composability_score: f32,
    pub requires: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureMode { pub mode: String, pub count: usize }

/// Composite Skill — merged from two or more primitives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositeSkill {
    pub id: String, pub name: String,
    pub sources: Vec<String>, pub script: String,
    pub description: String, pub genome: SkillGenome,
}

/// Trajectory Compression — high-level pattern from raw traces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressedPattern {
    pub pattern_name: String, pub signature: String,
    pub description: String, pub occurrence_count: usize,
    pub generalized_script: String,
}

/// Skill Crystallization — Rhai ready for Rust native translation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrystallizationCandidate {
    pub skill_id: String, pub rhai_signature: String,
    pub suggested_rust_name: String, pub confidence: f32, pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PrimitiveStage {
    Experimental, Stable, Core, Deprecated, Removed,
}

impl Default for PrimitiveStage {
    fn default() -> Self { PrimitiveStage::Experimental }
}

impl PrimitiveStage {
    pub fn promote(&mut self) {
        *self = match self { Self::Experimental => Self::Stable, Self::Stable => Self::Core, _ => return };
    }
    pub fn demote(&mut self) {
        *self = match self { Self::Core => Self::Stable, Self::Stable => Self::Experimental, Self::Experimental => Self::Deprecated, _ => return };
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AntiPattern {
    pub id: String,
    pub mistake: String,
    pub correct_guidance: String,
    pub error_pattern: String,
    pub frequency: usize,
    pub example: String,
    pub category: AntiPatternCategory,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AntiPatternCategory {
    SyntaxError, TypeMismatch, MissingFunction, AsyncMisuse, IteratorMisuse, OwnershipError, Other,
}

impl std::fmt::Display for AntiPatternCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DreamTask {
    pub description: String, pub category: String, pub mock_data: Option<String>,
}

// ═══════════════════════════════════════════════════════════
// Skill Library
// ═══════════════════════════════════════════════════════════

pub struct RhaiSkillLibrary {
    pub entries: Vec<LabeledScript>,
    pub anti_patterns: Vec<AntiPattern>,
    pub composite_skills: Vec<CompositeSkill>,
    pub compressed_patterns: Vec<CompressedPattern>,
    pub task_queue: VecDeque<DreamTask>,
    pub max_entries: usize,
    pub max_anti_patterns: usize,
}

impl RhaiSkillLibrary {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(), anti_patterns: Vec::new(),
            composite_skills: Vec::new(), compressed_patterns: Vec::new(),
            task_queue: VecDeque::new(), max_entries: 1000, max_anti_patterns: 50,
        }
    }

    pub fn add(&mut self, script: LabeledScript) {
        if script.label.is_failure() { self.extract_anti_pattern(&script); }
        else { self.promote_similar(&script); }
        self.entries.push(script);
        self.prune();
    }

    pub fn query_few_shots(&self, task: &str, k: usize) -> Vec<&LabeledScript> {
        let mut scored: Vec<(&LabeledScript, usize)> = self.entries.iter()
            .filter(|s| s.label.is_success() && s.lifecycle != PrimitiveStage::Removed)
            .map(|s| (s, keyword_overlap(task, &s.task_description)))
            .collect();
        scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.usage_count.cmp(&a.0.usage_count)));
        scored.truncate(k);
        scored.into_iter().map(|(s, _)| s).collect()
    }

    pub fn core_primitives(&self) -> Vec<&LabeledScript> {
        self.entries.iter().filter(|s| s.lifecycle == PrimitiveStage::Core && s.label.is_success()).collect()
    }

    pub fn active_anti_patterns(&self) -> Vec<&AntiPattern> {
        self.anti_patterns.iter().filter(|ap| ap.frequency >= 3).collect()
    }

    pub fn record_usage(&mut self, script_id: &str) {
        if let Some(entry) = self.entries.iter_mut().find(|s| s.id == script_id) {
            entry.usage_count += 1;
            if entry.usage_count > 10 && entry.lifecycle == PrimitiveStage::Experimental { entry.lifecycle.promote(); }
            if entry.usage_count > 50 && entry.lifecycle == PrimitiveStage::Stable { entry.lifecycle.promote(); }
        }
    }

    pub fn decay(&mut self) {
        for entry in &mut self.entries {
            if entry.usage_count < 2 && entry.lifecycle == PrimitiveStage::Experimental { entry.lifecycle = PrimitiveStage::Deprecated; }
        }
        self.entries.retain(|e| e.lifecycle != PrimitiveStage::Removed);
    }

    pub fn build_few_shot_prompt(&self, task: &str) -> String {
        let few_shots = self.query_few_shots(task, 2);
        if few_shots.is_empty() { return String::new(); }
        let mut prompt = String::from("// ── Verified Examples ──\n");
        for (i, shot) in few_shots.iter().enumerate() {
            if let Some(ref sig) = shot.function_signature { prompt.push_str(&format!("// Skill #{}: {}\n", i + 1, sig)); }
            prompt.push_str(&format!("// Task: {}\n// ```rhai\n{}\n// ```\n\n", shot.task_description, shot.script));
        }
        prompt
    }

    pub fn build_anti_pattern_guidance(&self) -> String {
        let patterns = self.active_anti_patterns();
        if patterns.is_empty() { return String::new(); }
        let mut guidance = String::from("// ── Rhai Best Practices (auto-learned) ──\n");
        for ap in patterns.iter().take(5) { guidance.push_str(&format!("// ✅ {}\n", ap.correct_guidance)); }
        guidance
    }

    // ─── Genome Analysis ───

    /// Compose two successful primitives into a CompositeSkill.
    pub fn compose_skills(&mut self, a_id: &str, b_id: &str) -> Option<CompositeSkill> {
        let a = self.entries.iter().find(|s| s.id == a_id)?;
        let b = self.entries.iter().find(|s| s.id == b_id)?;
        if !a.label.is_success() || !b.label.is_success() { return None; }
        let name = format!("{}+{}", a_id, b_id).replace('-', "_");
        Some(CompositeSkill {
            id: uuid::Uuid::new_v4().to_string(), name,
            sources: vec![a_id.to_string(), b_id.to_string()],
            script: format!("// {}\n{}\n\n// {}\n{}", a.task_description, a.script, b.task_description, b.script),
            description: format!("Composite of: {}, {}", a.task_description, b.task_description),
            genome: SkillGenome::default(),
        })
    }

    /// Suggest Crystallization candidates (Rhai→Rust).
    pub fn suggest_crystallizations(&self) -> Vec<CrystallizationCandidate> {
        self.entries.iter()
            .filter(|s| s.lifecycle == PrimitiveStage::Core || s.lifecycle == PrimitiveStage::Stable)
            .filter(|s| s.label.is_success() && s.usage_count > 20)
            .map(|s| CrystallizationCandidate {
                skill_id: s.id.clone(),
                rhai_signature: s.function_signature.clone().unwrap_or_default(),
                suggested_rust_name: format!("crystallized_{}", s.id.replace('-', "_")),
                confidence: (s.usage_count as f32 / 100.0).min(1.0),
                reason: format!("Used {} times with stable success. Ready for Rust native compilation.", s.usage_count),
            })
            .collect()
    }

    // ─── Internal ───

    fn extract_anti_pattern(&mut self, script: &LabeledScript) {
        let (category, pattern_fragment) = classify_error(script);
        for ap in &mut self.anti_patterns {
            if keyword_overlap(&ap.error_pattern, &pattern_fragment) > 0 { ap.frequency += 1; return; }
        }
        let guidance = generate_positive_guidance(category.clone(), &pattern_fragment);
        self.anti_patterns.push(AntiPattern {
            id: uuid::Uuid::new_v4().to_string(), mistake: pattern_fragment.clone(),
            correct_guidance: guidance, error_pattern: pattern_fragment,
            frequency: 1, example: truncate(&script.script, 200), category,
        });
        if self.anti_patterns.len() > self.max_anti_patterns {
            self.anti_patterns.sort_by(|a, b| b.frequency.cmp(&a.frequency));
            self.anti_patterns.truncate(self.max_anti_patterns);
        }
    }

    fn promote_similar(&mut self, script: &LabeledScript) {
        for entry in &mut self.entries {
            if entry.label.is_success() && keyword_overlap(&entry.task_description, &script.task_description) > 2 {
                if entry.function_signature.is_none() && script.function_signature.is_some() {
                    entry.function_signature = script.function_signature.clone();
                }
            }
        }
    }

    fn prune(&mut self) {
        if self.entries.len() > self.max_entries {
            let mut successes: Vec<_> = self.entries.iter().filter(|e| e.label.is_success()).cloned().collect();
            let mut failures: Vec<_> = self.entries.iter().filter(|e| e.label.is_failure()).cloned().collect();
            successes.truncate(self.max_entries / 2);
            failures.truncate(self.max_entries / 2);
            self.entries = successes;
            self.entries.append(&mut failures);
        }
    }
}

// ═══════════════════════════════════════════════════════════
// Error Classification
// ═══════════════════════════════════════════════════════════

fn classify_error(script: &LabeledScript) -> (AntiPatternCategory, String) {
    match &script.label {
        Label::ParseError { message, .. } => {
            let lower = message.to_lowercase();
            if lower.contains("async") || lower.contains("await") { (AntiPatternCategory::AsyncMisuse, "async/await in Rhai".into()) }
            else if lower.contains("type") || lower.contains("expected") { (AntiPatternCategory::TypeMismatch, message.clone()) }
            else if lower.contains("not found") || lower.contains("undefined") { (AntiPatternCategory::MissingFunction, message.clone()) }
            else if lower.contains("iter") || lower.contains(".map") { (AntiPatternCategory::IteratorMisuse, message.clone()) }
            else { (AntiPatternCategory::SyntaxError, message.clone()) }
        }
        Label::RuntimeError { message } => {
            let lower = message.to_lowercase();
            if lower.contains("cannot index") { (AntiPatternCategory::TypeMismatch, message.clone()) }
            else { (AntiPatternCategory::Other, message.clone()) }
        }
        _ => (AntiPatternCategory::Other, "Unknown".into()),
    }
}

fn generate_positive_guidance(cat: AntiPatternCategory, _e: &str) -> String {
    match cat {
        AntiPatternCategory::AsyncMisuse => "Rhai is synchronous. No .await needed.".into(),
        AntiPatternCategory::TypeMismatch => "Rhai uses dynamic typing. Use `let x = value;` without annotations.".into(),
        AntiPatternCategory::MissingFunction => "Available: read_file, write_file, grep, shell, list_dir.".into(),
        AntiPatternCategory::IteratorMisuse => "Use for-loops: `for item in items { ... }` instead of .iter().map().".into(),
        AntiPatternCategory::SyntaxError => "Rhai syntax is Rust-like: let, fn, // comments, no semicolons required.".into(),
        AntiPatternCategory::OwnershipError => "Rhai handles ownership automatically. No borrow checker.".into(),
        AntiPatternCategory::Other => "Wrap in try-catch: `try { ... } catch { ... }`".into(),
    }
}

// ═══════════════════════════════════════════════════════════
// Verifier & Dream Mode
// ═══════════════════════════════════════════════════════════

pub async fn verify_script(task: &str, script: &str, output: &str, model: &Arc<dyn pharmakon_common::AgentModel>) -> Label {
    let prompt = format!("Task: {}\nScript:\n```\n{}\n```\nOutput:\n{}\n\nAccomplished? YES/NO:", truncate(task, 300), truncate(script, 500), truncate(output, 500));
    let request = pharmakon_common::CompletionRequest {
        messages: vec![pharmakon_common::Message { role: "user".into(), content: Some(pharmakon_common::MessageContent::Text(prompt)), ..Default::default() }],
        temperature: Some(0.0), max_tokens: Some(4), tools: None,
    };
    match model.complete(request).await {
        Ok(resp) => {
            let text = resp.content.as_ref().and_then(|c| c.as_text()).unwrap_or("").trim().to_uppercase();
            if text.contains("YES") { Label::Success { verified_by: model.name().to_string() } }
            else { Label::WrongOutput { expected: task.to_string(), got: output.to_string() } }
        }
        Err(e) => Label::Skipped { reason: format!("Verifier failed: {}", e) },
    }
}

pub async fn generate_dream_tasks(model: &Arc<dyn pharmakon_common::AgentModel>, count: usize) -> Vec<DreamTask> {
    let prompt = format!("Generate {} task descriptions for file/code automation. Categories: grep_and_transform, batch_rename, code_stats, text_processing, file_organization, config_merging, log_parsing, data_extraction. One per line: CATEGORY: description", count);
    let request = pharmakon_common::CompletionRequest {
        messages: vec![pharmakon_common::Message { role: "user".into(), content: Some(pharmakon_common::MessageContent::Text(prompt)), ..Default::default() }],
        temperature: Some(0.8), max_tokens: Some(500), tools: None,
    };
    match model.complete(request).await {
        Ok(resp) => {
            resp.content.as_ref().and_then(|c| c.as_text()).unwrap_or("")
                .lines().filter_map(|line| {
                    let parts: Vec<&str> = line.splitn(2, ": ").collect();
                    if parts.len() == 2 { Some(DreamTask { category: parts[0].trim().into(), description: parts[1].trim().into(), mock_data: None }) }
                    else { None }
                }).take(count).collect()
        }
        Err(_) => Vec::new(),
    }
}

pub fn build_codeact_system_prompt(library: &RhaiSkillLibrary, task: &str) -> String {
    let mut prompt = String::new();
    let apg = library.build_anti_pattern_guidance();
    if !apg.is_empty() { prompt.push_str(&apg); prompt.push('\n'); }
    let fs = library.build_few_shot_prompt(task);
    if !fs.is_empty() { prompt.push_str(&fs); }
    let core = library.core_primitives();
    if !core.is_empty() {
        prompt.push_str("// ── Verified Functions ──\n");
        for prim in core.iter().take(5) {
            if let Some(ref sig) = prim.function_signature { prompt.push_str(&format!("// fn {} — {}\n", sig, prim.task_description)); }
        }
        prompt.push_str("// Call these directly.\n\n");
    }
    prompt.push_str("// ── Available ──\n// read_file(path)->String, write_file(path,content), grep(pattern,dir)->[String], shell(cmd)->String, list_dir(path)->[String]\n\n");
    prompt
}

fn keyword_overlap(a: &str, b: &str) -> usize {
    let aw: std::collections::HashSet<_> = a.split_whitespace().map(|w| w.to_lowercase()).collect();
    let bw: std::collections::HashSet<_> = b.split_whitespace().map(|w| w.to_lowercase()).collect();
    aw.intersection(&bw).count()
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_string() } else { format!("{}...", &s[..max]) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_genome_default() {
        let g = SkillGenome::default();
        assert!(g.capabilities.is_empty());
        assert_eq!(g.success_rate, 0.0);
    }

    #[test]
    fn test_compose_skills() {
        let mut lib = RhaiSkillLibrary::new();
        let s1 = LabeledScript {
            id: "s1".into(), task_description: "grep files".into(), script: "let x = grep(\"p\", \".\")".into(),
            label: Label::Success { verified_by: "test".into() }, category: "grep".into(),
            timestamp: chrono::Utc::now(), function_signature: Some("grep_files(dir)".into()),
            usage_count: 10, lifecycle: PrimitiveStage::Core, genome: SkillGenome::default(),
        };
        let s2 = LabeledScript {
            id: "s2".into(), task_description: "write output".into(), script: "write_file(\"o\", x)".into(),
            label: Label::Success { verified_by: "test".into() }, category: "write".into(),
            timestamp: chrono::Utc::now(), function_signature: Some("write_output(path)".into()),
            usage_count: 10, lifecycle: PrimitiveStage::Core, genome: SkillGenome::default(),
        };
        lib.add(s1); lib.add(s2);
        let comp = lib.compose_skills("s1", "s2");
        assert!(comp.is_some());
    }
}
