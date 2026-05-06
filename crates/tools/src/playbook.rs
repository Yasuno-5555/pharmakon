use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool};
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

pub struct PlaybookTool;

impl PlaybookTool {
    pub fn new() -> Self {
        Self
    }
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

    fn get_builtin_playbooks() -> Vec<(&'static str, &'static str)> {
        vec![
            (
                "rust_refactor",
                "1. Use `ingest_ast_knowledge` to index the target module structure.\n2. Query `Knowledge Nexus` for dependent functions and trait implementations.\n3. Create a new implementation in a separate module if possible.\n4. Update all call sites using `grep_search` and `edit_file`.\n5. Run `cargo check` and `cargo test` to verify functional parity.",
            ),
            (
                "deep_research",
                "1. Start by defining the `current_goal` in the `Research Notebook`.\n2. Use `smart_search` (Knowledge Nexus) to find initial technical anchors.\n3. Use `web_fetch` or `brave_search` to gather external context if needed.\n4. Verify facts and document them in the `Research Notebook`.\n5. Iterate until all `pending_questions` are resolved or a `dead_end` is reached.",
            ),
            (
                "security_audit",
                "1. List all dependencies and check for known vulnerabilities.\n2. Search for hardcoded secrets or API keys using `grep_search`.\n3. Verify file permissions of sensitive configurations.\n4. Review any use of `unsafe` or raw pointers in the codebase.",
            ),
            (
                "bug_hunt",
                "1. Reproduce the bug with a minimal test case.\n2. Use `diagnostic` tools to trace the failure.\n3. Use `Knowledge Nexus` to find related logic blocks that might be affected.\n4. Set `semantic_anchors` at suspected logic points.\n5. Fix the bug and verify with the reproduction test.",
            ),
        ]
    }

    pub fn list_names() -> Vec<String> {
        let mut names = Vec::new();
        for (name, _) in Self::get_builtin_playbooks() {
            names.push(name.to_string());
        }

        let recipes_dir = std::path::PathBuf::from(".pharmakon/recipes");
        if recipes_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&recipes_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                            let name_str = name.to_string();
                            if !names.contains(&name_str) {
                                names.push(name_str);
                            }
                        }
                    }
                }
            }
        }
        names
    }
}

#[async_trait]
impl Tool for PlaybookTool {
    fn name(&self) -> &str {
        "playbook"
    }
    fn description(&self) -> &str {
        "Manage and execute pre-defined workflows (recipes). Supports variables and context injection."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["list", "load", "inject"], "description": "list: show playbooks, load: get instructions, inject: permanently add to your current session context." },
                "name": { "type": "string", "description": "The name of the playbook." },
                "variables": { "type": "object", "description": "Variables to replace in the playbook (e.g. {\"target\": \"User\"})" }
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
            "list" => {
                let mut playbooks = Vec::new();

                // Built-ins
                for (name, _) in Self::get_builtin_playbooks() {
                    playbooks.push(format!("{} (built-in)", name));
                }

                // Local files
                if recipes_dir.exists() {
                    let entries =
                        fs::read_dir(&recipes_dir).map_err(|e| AgentError(e.to_string()))?;
                    for entry in entries {
                        if let Ok(e) = entry {
                            let path = e.path();
                            if path.is_file() {
                                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                                    playbooks.push(name.to_string());
                                }
                            }
                        }
                    }
                }

                if playbooks.is_empty() {
                    Ok("No playbooks found.".to_string())
                } else {
                    Ok(format!(
                        "### Available Playbooks\n\n{}",
                        playbooks
                            .iter()
                            .map(|p| format!("- {}", p))
                            .collect::<Vec<_>>()
                            .join("\n")
                    ))
                }
            }
            "load" | "inject" => {
                let name = args["name"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing playbook name".to_string()))?;

                // Try built-ins first
                let mut content = Self::get_builtin_playbooks()
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, c)| c.to_string());

                // Try local files
                if content.is_none() {
                    let mut path = recipes_dir.join(name);
                    if !path.exists() {
                        for ext in &["json", "yaml", "md", "txt"] {
                            let p = recipes_dir.join(format!("{}.{}", name, ext));
                            if p.exists() {
                                path = p;
                                break;
                            }
                        }
                    }
                    if path.exists() {
                        content = fs::read_to_string(path).ok();
                    }
                }

                let raw_content =
                    content.ok_or_else(|| AgentError(format!("Playbook '{}' not found.", name)))?;
                let processed_content = Self::apply_variables(&raw_content, variables);

                if action == "inject" {
                    // Note: This output tells the agent that it should internalize the instructions.
                    // The core logic of "inject" is actually handled by the agent seeing this response.
                    Ok(format!(
                        "### INJECTED PLAYBOOK: {}\n\nSystem Instruction: You are now strictly following the '{}' playbook. Internalize the following steps as your primary mission for this session:\n\n{}",
                        name, name, processed_content
                    ))
                } else {
                    Ok(format!(
                        "### PLAYBOOK: {}\n\nInstructions:\n{}\n\nPlease follow these steps to complete the task.",
                        name, processed_content
                    ))
                }
            }
            _ => Err(AgentError("Invalid action".to_string())),
        }
    }
}
