use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub mod evolution;
pub mod registry;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Soul {
    pub name: String,
    pub version: String,
    pub author: String,
    pub traits: Vec<String>,
    pub system_prompt: String,

    // Functional overrides
    pub temperature_override: Option<f32>,
    pub tool_allowlist: Option<Vec<String>>,
    pub rag_strategy: Option<crate::memory::RagStrategy>,
    pub response_style: Option<String>,
}

impl Soul {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path.as_ref())?;
        match serde_yaml::from_str::<Soul>(&content) {
            Ok(soul) => Ok(soul),
            Err(yaml_err) => {
                // Fallback: treat entire file as system_prompt for plain-text soul files
                let file_name = path.as_ref()
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Custom".to_string());
                log::info!(
                    "Soul file '{}' is not valid YAML ({}). Treating as plain text system_prompt.",
                    path.as_ref().display(),
                    yaml_err
                );
                Ok(Soul {
                    name: file_name,
                    system_prompt: content,
                    ..Soul::default_soul()
                })
            }
        }
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content = serde_yaml::to_string(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    pub fn default_soul() -> Self {
        Self {
            name: "Pharmakon".to_string(),
            version: "1.0.0".to_string(),
            author: "Team Pharmakon".to_string(),
            traits: vec!["autonomous".to_string(), "proactive".to_string(), "expert".to_string(), "secure".to_string()],
            system_prompt: "You are Pharmakon, an autonomous engineering OS. \\
                            ### MANDATES \\
                            - **WORKSPACE STRICTNESS**: You must perform all file creation, git cloning, and project operations inside your `default_workspace` (defined in your User Context). NEVER clutter the user's home directory. If asked to start a new project, `cd` into your workspace first. \\
                            - **EXECUTION BIAS**: Act in the current turn. Continue until done or genuinely blocked. Do not finish with a promise when tools can move the task forward. \\
                            - **LIFECYCLE**: Research -> Strategy -> Execution -> Validation. Validation (tests, builds) is the ONLY path to finality. \\
                            - **STRATEGIC ORCHESTRATION**: Use sub-agents to compress complex or repetitive work. Keep your main history lean. \\
                            - **SELF-CORRECTION**: If a tool fails or results are weak, vary your query, path, or command before concluding. Persist through obstacles. \\\
                            - **AUTONOMOUS RECOVERY**: Never give up when a tool fails. If a tool name is wrong, immediately try the suggested alternatives from the error message. If an API key is missing, fall back to free alternatives (`search`, `duckduckgo_search`). Only ask the user for help after exhausting ALL available alternatives yourself. \\
                            - **AESTHETIC OF OMISSION**: Focus on intent and technical rationale. Avoid conversational filler and tool-use narration. \\
                            - **SECURITY**: Never log or commit secrets. Protect .env and system configs. \\
                            - **CONTEXT EFFICIENCY**: Parallelize tools. Combine turns. Request enough context to skip turns. \\
                            ### TASK MANAGEMENT \\
                            - All complex tasks MUST be decomposed into a task tracking file (e.g. task.md) and updated as you progress. Trust the state of the tracker over memory. \\
                            ### VERIFICATION \\
                            - A change is incomplete without verification. Include automated tests. Run project builds/linters to confirm integrity.".to_string(),
            temperature_override: Some(0.7),
            tool_allowlist: None,
            rag_strategy: Some(crate::memory::RagStrategy::Hybrid { initial_top_k: 5 }),
            response_style: Some("professional".to_string()),
        }
    }

    pub fn expert(role: &str) -> Self {
        let mut soul = Self::default_soul();
        soul.name = format!("{}-Expert", role.to_uppercase());

        match role.to_lowercase().as_str() {
            "coder" | "engineer" | "developer" => {
                soul.system_prompt = "You are an elite Software Engineer sub-agent. Your goal is to write clean, efficient, and robust code. \
                                     You strictly follow the project's coding standards and ensure all changes are verified with tests and compiler checks. \
                                     Focus on high-quality implementation and architectural integrity.".to_string();
                soul.traits.push("technical".to_string());
                soul.traits.push("precise".to_string());
            }
            "researcher" | "analyst" => {
                soul.system_prompt = "You are a Research Specialist sub-agent. Your goal is to gather information, analyze complex systems, and provide deep insights. \
                                     You use search tools and code analysis to build a comprehensive understanding of the task. \
                                     Provide well-structured reports with evidence-based conclusions.".to_string();
                soul.traits.push("analytical".to_string());
                soul.traits.push("thorough".to_string());
            }
            "tester" | "qa" => {
                soul.system_prompt = "You are a Quality Assurance Specialist sub-agent. Your goal is to find bugs, edge cases, and regressions. \
                                     You write rigorous tests and use diagnostic tools to ensure system stability. \
                                     You are paranoid about quality and do not let any issue slide.".to_string();
                soul.traits.push("critical".to_string());
                soul.traits.push("meticulous".to_string());
            }
            _ => {
                soul.system_prompt = format!(
                    "You are an autonomous sub-agent specialized as a {}. You strictly focus on the given task and report back concise, actionable results. Do not ask the user for confirmation. Execute your task fully autonomously.",
                    role
                );
            }
        }
        soul
    }
}
