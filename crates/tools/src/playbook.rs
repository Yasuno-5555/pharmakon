use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool};
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;

pub struct PlaybookTool;

impl Default for PlaybookTool {
    fn default() -> Self { Self::new() }
}

impl PlaybookTool {
    pub fn new() -> Self { Self }
}

/// Structured playbook metadata for discovery and suggestion.
struct PlaybookDef {
    name: &'static str,
    category: &'static str,
    description: &'static str,
    keywords: &'static [&'static str],
    tools: &'static [&'static str],
    content: &'static str,
}

impl PlaybookTool {
    fn apply_variables(content: &str, variables: &Value) -> String {
        let mut result = content.to_string();
        if let Some(obj) = variables.as_object() {
            for (key, val) in obj {
                let placeholder = format!("{{{{{}}}}}", key);
                let replacement = match val {
                    Value::String(s) => s.clone(),
                    _ => val.to_string(),
                };
                result = result.replace(&placeholder, &replacement);
            }
        }
        result
    }

    fn all_playbooks() -> Vec<PlaybookDef> {
        vec![
            // ── Research & Information ──
            PlaybookDef {
                name: "web_research",
                category: "Research",
                description: "Search the web and compile structured findings with citations",
                keywords: &["search", "google", "research", "lookup", "find information", "web", "internet"],
                tools: &["search", "web_fetch"],
                content: "GOAL: Gather information from the web and produce a structured report.\n\n1. Identify 2-4 specific search queries related to the topic.\n2. Use `search` with each query to discover relevant pages.\n3. For the most promising 2-3 results per query, use `web_fetch` to retrieve full content.\n4. Extract key facts, dates, and claims from each source.\n5. Cross-reference: verify critical claims across at least 2 sources.\n6. Produce a structured summary:\n   - **Topic**: one-line summary\n   - **Key Findings**: bulleted list with source URLs\n   - **Confidence**: high/medium/low per finding\n   - **Sources**: numbered list of all URLs consulted",
            },
            PlaybookDef {
                name: "deep_research",
                category: "Research",
                description: "Multi-source deep dive with source verification and structured output",
                keywords: &["deep research", "comprehensive", "thorough", "investigate", "analyze", "survey"],
                tools: &["search", "web_fetch", "duckduckgo_search"],
                content: "GOAL: Conduct exhaustive multi-source research on a complex topic.\n\n1. Define the research scope: what to include and exclude.\n2. Use `search` with 5-8 varied query formulations.\n3. For each promising result, use `web_fetch` to extract full article text.\n4. Categorize findings by theme/subtopic.\n5. For each category, cross-reference at least 3 sources.\n6. Identify contradictions between sources and note them.\n7. Produce:\n   - **Executive Summary** (3-5 sentences)\n   - **Detailed Findings** by category with source citations\n   - **Contradictions & Open Questions**\n   - **Source List** with brief credibility notes",
            },
            // ── Code Quality ──
            PlaybookDef {
                name: "code_review",
                category: "Quality",
                description: "Systematic code review with actionable feedback",
                keywords: &["review", "code review", "check", "audit code", "inspect"],
                tools: &["read_file", "grep_files", "shell", "view_file"],
                content: "GOAL: Review code for correctness, safety, and style.\n\n1. Identify the files to review (changed files, or specified targets).\n2. For each file, use `read_file` to load the full content.\n3. Check for:\n   - Logic errors (off-by-one, inverted conditions, missing edge cases)\n   - Error handling (unwrapped Results, missing ? operators, panic sites)\n   - Resource management (leaked file handles, unclosed connections)\n   - Concurrency issues (data races, deadlock potential)\n   - Input validation (untrusted data, injection vectors)\n4. Use `grep_files` to check for anti-patterns across the codebase.\n5. If applicable, run `shell` with `cargo check` or equivalent.\n6. Output: per-file PASS/NEEDS WORK with specific line references and suggested fixes.",
            },
            PlaybookDef {
                name: "security_audit",
                category: "Quality",
                description: "Security-focused audit of code and dependencies",
                keywords: &["security", "vulnerability", "audit", "secret", "unsafe", "exploit"],
                tools: &["grep_files", "read_file", "shell", "web_fetch"],
                content: "GOAL: Identify security vulnerabilities in the codebase.\n\n1. Use `shell` to list dependencies (cargo audit, npm audit, pip audit).\n2. Use `grep_files` with patterns:\n   - `(?i)(api_key|secret|password|token|credential)\\s*=` (hardcoded secrets)\n   - `unsafe\\s*\\{` (unsafe Rust blocks)\n   - `eval\\(|exec\\(|system\\(` (code injection risks)\n3. Review any `.env` or config files for committed secrets.\n4. Use `web_fetch` to check dependency CVEs if applicable.\n5. Rate each finding: CRITICAL / HIGH / MEDIUM / LOW.\n6. For each CRITICAL/HIGH, provide a concrete fix recommendation.",
            },
            // ── Development ──
            PlaybookDef {
                name: "rust_refactor",
                category: "Development",
                description: "Safe Rust refactoring with compiler-guided verification",
                keywords: &["refactor", "rust", "rewrite", "restructure", "cargo"],
                tools: &["read_file", "grep_files", "apply_patch", "write_file", "shell"],
                content: "GOAL: Refactor Rust code with minimal risk.\n\n1. Use `read_file` to understand the current module structure.\n2. Use `grep_files` to find all call sites and trait implementations.\n3. Plan the new structure before writing code.\n4. Create new module/impl incrementally using `write_file`.\n5. Update call sites using `apply_patch` (preferred) or `write_file`.\n6. After each change, run `shell` with `cargo check`.\n7. If check fails, fix errors immediately before proceeding.\n8. After all changes, run `cargo test`.\n9. DO NOT proceed to step 5 until step 4 compiles cleanly.",
            },
            PlaybookDef {
                name: "implement_feature",
                category: "Development",
                description: "End-to-end feature implementation workflow",
                keywords: &["implement", "feature", "build", "create", "add", "new endpoint", "new function"],
                tools: &["read_file", "write_file", "grep_files", "shell", "apply_patch"],
                content: "GOAL: Implement a new feature from specification to passing tests.\n\n1. Understand the codebase: use `read_file` on relevant existing modules.\n2. Use `grep_files` to find patterns similar to what you need to build.\n3. Write tests FIRST using `write_file` (TDD approach).\n4. Implement the minimum code to pass tests.\n5. Run `shell` with `cargo test` (or equivalent test runner).\n6. Iterate steps 4-5 until all tests pass.\n7. Use `write_file` to update documentation if applicable.\n8. Run final full test suite to check for regressions.",
            },
            PlaybookDef {
                name: "bug_hunt",
                category: "Development",
                description: "Systematic bug diagnosis and fix workflow",
                keywords: &["bug", "fix", "error", "crash", "broken", "debug", "issue", "problem"],
                tools: &["read_file", "grep_files", "shell", "write_file", "apply_patch"],
                content: "GOAL: Find and fix a bug with minimal collateral damage.\n\n1. Reproduce the bug: what exact input/state triggers it?\n2. Read the error output carefully — note file paths and line numbers.\n3. Use `read_file` on the failing module, focusing on the error site.\n4. Use `grep_files` to find ALL code paths that lead to the failing function.\n5. Add diagnostic `shell` commands or temporary logging to isolate the cause.\n6. Once root cause is identified, write the minimal fix.\n7. Verify the fix resolves the original bug.\n8. Run the full test suite to ensure no regressions.\n9. If the fix is >20 lines, consider whether a broader refactor is needed.",
            },
            // ── Infrastructure ──
            PlaybookDef {
                name: "dependency_update",
                category: "Infrastructure",
                description: "Safe dependency version update workflow",
                keywords: &["dependency", "update", "upgrade", "crate", "package", "version", "bump"],
                tools: &["read_file", "shell", "grep_files", "apply_patch"],
                content: "GOAL: Update project dependencies safely.\n\n1. Use `shell` to check current versions: `cargo outdated` or `npm outdated`.\n2. Read the changelog of the target dependency using `web_fetch`.\n3. Update version in Cargo.toml / package.json using `apply_patch`.\n4. Run `shell` with build command (cargo build / npm build).\n5. If build fails, check for breaking changes in the dependency's changelog.\n6. Fix any API breakage using `grep_files` to find all affected call sites.\n7. Run full test suite.\n8. If all passes, the update is complete. If not, roll back and report blockers.",
            },
            PlaybookDef {
                name: "project_setup",
                category: "Infrastructure",
                description: "Initialize or assess a project structure",
                keywords: &["setup", "init", "scaffold", "project", "workspace", "new project", "onboard"],
                tools: &["workspace_perception", "repomap", "read_file", "shell", "list_dir"],
                content: "GOAL: Understand and document a project's structure.\n\n1. Use `workspace_perception` to detect project type and structure.\n2. Use `repomap` to get a structural overview.\n3. Use `list_dir` to explore the top-level directory layout.\n4. Identify: build system, test framework, key entry points, config files.\n5. Use `read_file` on README, Cargo.toml/package.json, and main entry point.\n6. Produce:\n   - **Project Type**: language, framework, build system\n   - **Key Modules**: top 3-5 directories and their purpose\n   - **Entry Points**: main file, config, test runner\n   - **Dependencies**: count and notable ones\n   - **Quick Start**: commands to build, test, and run",
            },
            // ── Documentation ──
            PlaybookDef {
                name: "write_docs",
                category: "Documentation",
                description: "Generate or improve code documentation",
                keywords: &["document", "doc", "readme", "comment", "explain"],
                tools: &["read_file", "write_file", "grep_files"],
                content: "GOAL: Create or improve documentation for code.\n\n1. Use `read_file` to load the target module(s).\n2. Identify undocumented public items (functions, structs, traits).\n3. Write doc comments (/// or /** */) covering:\n   - What the item does (one line)\n   - Parameters and return values\n   - Panics and errors\n   - Usage example\n4. Use `write_file` or `apply_patch` to add documentation.\n5. Check for consistency: do related items use similar terminology?\n6. If the module lacks a module-level doc (//!), add one.",
            },
            // ── Data & Analysis ──
            PlaybookDef {
                name: "data_analysis",
                category: "Analysis",
                description: "Analyze codebase data: metrics, patterns, trends",
                keywords: &["analyze", "metrics", "statistics", "count", "measure", "profile", "benchmark"],
                tools: &["shell", "grep_files", "read_file", "codeact"],
                content: "GOAL: Extract quantitative insights from the codebase.\n\n1. Define what to measure (lines of code, test coverage, dependency count, etc).\n2. Use `shell` with tools like `tokei`, `cloc`, or `wc -l` for line counts.\n3. Use `grep_files` with patterns to count occurrences of patterns.\n4. For complex analysis, use `codeact` to write a Rhai script.\n5. Present results as structured data:\n   - Raw numbers with context\n   - Trends if historical data is available\n   - Comparison to benchmarks if applicable\n6. Suggest actionable insights based on the data.",
            },
            // ── General ──
            PlaybookDef {
                name: "general_task",
                category: "General",
                description: "Default structured workflow for any task",
                keywords: &["task", "help", "assist", "do", "please"],
                tools: &["read_file", "shell", "grep_files", "write_file"],
                content: "GOAL: Execute a general task with structured approach.\n\n1. Clarify: restate the task to confirm understanding.\n2. Assess: what information do you need? Use `read_file` or `search` as needed.\n3. Plan: list 3-5 concrete steps before executing.\n4. Execute: follow the plan, one step at a time.\n5. Verify: check that each step produced the expected result.\n6. Report: summarize what was done and the outcome.",
            },
        ]
    }

    fn get_builtin_playbooks() -> Vec<(&'static str, &'static str)> {
        Self::all_playbooks()
            .into_iter()
            .map(|p| (p.name, p.content))
            .collect()
    }

    /// Suggest playbooks matching a task description.
    fn suggest_for_task(task_description: &str) -> Vec<&'static PlaybookDef> {
        let lower = task_description.to_lowercase();
        let all = Self::all_playbooks();
        // Leak to get static refs — acceptable for built-in playbooks
        let all: &'static [PlaybookDef] = Box::leak(all.into_boxed_slice());

        let mut scored: Vec<(&PlaybookDef, usize)> = all
            .iter()
            .map(|p| {
                let score = p.keywords.iter()
                    .filter(|kw| lower.contains(&kw.to_lowercase()))
                    .count();
                (p, score)
            })
            .filter(|(_, s)| *s > 0)
            .collect();

        scored.sort_by(|a, b| b.1.cmp(&a.1));
        scored.into_iter().map(|(p, _)| p).collect()
    }

    pub fn list_names() -> Vec<String> {
        let mut names: Vec<String> = Self::all_playbooks()
            .into_iter()
            .map(|p| p.name.to_string())
            .collect();

        let recipes_dir = PathBuf::from(".pharmakon/recipes");
        if recipes_dir.exists()
            && let Ok(entries) = fs::read_dir(&recipes_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file()
                        && let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                            if !names.contains(&name.to_string()) {
                                names.push(name.to_string());
                            }
                        }
                }
            }
        names
    }
}

#[async_trait]
impl Tool for PlaybookTool {
    fn name(&self) -> &str { "playbook" }
    fn description(&self) -> &str {
        "Manage and execute pre-defined workflows (playbooks). Use `suggest` to find the right playbook for your task, `load` to read its steps, or `inject` to activate it for the session."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["suggest", "list", "load", "inject"],
                    "description": "suggest: find playbooks for a task, list: show all playbooks, load: read a playbook's steps, inject: activate the playbook for this session"
                },
                "query": { "type": "string", "description": "Task description for `suggest` action (e.g. 'search the web for Rust async patterns')" },
                "name": { "type": "string", "description": "Playbook name for `load` or `inject` actions" },
                "variables": { "type": "object", "description": "Template variables to substitute (e.g. {\"target\": \"User\"})" }
            },
            "required": ["action"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let action = args["action"].as_str().unwrap_or("list");
        let recipes_dir = PathBuf::from(".pharmakon/recipes");
        let default_vars = json!({});
        let variables = args.get("variables").unwrap_or(&default_vars);

        match action {
            "suggest" => {
                let query = args["query"].as_str().unwrap_or("");
                if query.is_empty() {
                    return Err(AgentError("Provide a `query` describing your task for suggestions.".into()));
                }
                let matches = Self::suggest_for_task(query);
                if matches.is_empty() {
                    Ok("No matching playbooks found. Try `list` to see all available playbooks.".into())
                } else {
                    let lines: Vec<String> = matches.iter().map(|p| {
                        format!(
                            "- **{}** [{}]: {} (uses: {})",
                            p.name,
                            p.category,
                            p.description,
                            p.tools.join(", ")
                        )
                    }).collect();
                    Ok(format!(
                        "### Suggested Playbooks for: \"{}\"\n\n{}\n\nUse `load` with the playbook name to see its steps, or `inject` to activate it.",
                        query,
                        lines.join("\n")
                    ))
                }
            }
            "list" => {
                let mut lines = vec![];
                lines.push("## Built-in Playbooks\n".into());
                for p in Self::all_playbooks() {
                    lines.push(format!(
                        "- **{}** [{}]: {}",
                        p.name, p.category, p.description
                    ));
                }

                // Local recipes
                if recipes_dir.exists() {
                    if let Ok(entries) = fs::read_dir(&recipes_dir) {
                        let mut custom = vec![];
                        for e in entries.flatten() {
                            let path = e.path();
                            if path.is_file()
                                && let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                                    custom.push(name.to_string());
                                }
                        }
                        if !custom.is_empty() {
                            lines.push("\n## Custom Recipes\n".into());
                            for name in &custom {
                                lines.push(format!("- {} (custom)", name));
                            }
                        }
                    }
                }

                if lines.len() <= 1 {
                    Ok("No playbooks found.".into())
                } else {
                    Ok(lines.join("\n"))
                }
            }
            "load" | "inject" => {
                let name = args["name"].as_str()
                    .ok_or_else(|| AgentError("Missing playbook `name`.".into()))?;

                let mut content = Self::all_playbooks()
                    .into_iter()
                    .find(|p| p.name == name)
                    .map(|p| p.content.to_string());

                if content.is_none() {
                    let mut path = recipes_dir.join(name);
                    if !path.exists() {
                        for ext in &["json", "yaml", "md", "txt"] {
                            let p = recipes_dir.join(format!("{}.{}", name, ext));
                            if p.exists() { path = p; break; }
                        }
                    }
                    if path.exists() {
                        content = fs::read_to_string(path).ok();
                    }
                }

                let raw = content
                    .ok_or_else(|| AgentError(format!("Playbook '{}' not found. Use `list` or `suggest` to see available playbooks.", name)))?;
                let processed = Self::apply_variables(&raw, variables);

                if action == "inject" {
                    Ok(format!(
                        "### INJECTED PLAYBOOK: {}\n\nSystem Instruction: You are now strictly following the '{}' playbook. Internalize the following steps as your primary mission for this session:\n\n{}",
                        name, name, processed
                    ))
                } else {
                    Ok(format!(
                        "### PLAYBOOK: {}\n\n{}\n\nUse `inject` with name='{}' to activate this playbook for the current session.",
                        name, processed, name
                    ))
                }
            }
            _ => Err(AgentError("Invalid action. Use: suggest, list, load, inject.".into())),
        }
    }
}
