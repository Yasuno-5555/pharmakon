//! Skill System — dynamic SKILL.md loading ala OpenClaw.
//!
//! Skills are domain-specific prompt + tool bundles stored as SKILL.md files
//! in a skills directory (~/.pharmakon/skills/ or project-local .pharmakon/skills/).
//! They are loaded on-demand via `load_skill` and inject specialized instructions
//! into the agent's working context.
//!
//! Structure:
//!   skills/
//!     github/
//!       SKILL.md          ← prompt + instructions for the skill
//!       tools.toml         ← optional tool configuration
//!     coding-agent/
//!       SKILL.md
//!     ...

use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A loaded skill with its prompt content and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Skill identifier (directory name).
    pub id: String,
    /// Human-readable name from SKILL.md frontmatter or filename.
    pub name: String,
    /// Full content of SKILL.md.
    pub content: String,
    /// Path to the skill directory.
    pub path: PathBuf,
    /// Companion files referenced in SKILL.md.
    pub companion_files: Vec<PathBuf>,
}

/// Registry of all available skills.
#[derive(Debug, Clone, Default)]
pub struct SkillRegistry {
    skills: HashMap<String, Skill>,
    /// Search paths for skills (ordered by priority).
    search_paths: Vec<PathBuf>,
}

impl SkillRegistry {
    /// Create a new registry with default search paths.
    pub fn new() -> Self {
        let mut paths = Vec::new();

        // Project-local skills
        if let Ok(cwd) = std::env::current_dir() {
            paths.push(cwd.join(".pharmakon").join("skills"));
        }

        // User-global skills
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".pharmakon").join("skills"));
        }

        Self {
            skills: HashMap::new(),
            search_paths: paths,
        }
    }

    /// Add a custom search path.
    pub fn add_search_path(&mut self, path: PathBuf) {
        self.search_paths.push(path);
    }

    /// Scan all search paths and index available skills.
    pub fn scan(&mut self) -> Result<usize> {
        let mut count = 0;
        for path in &self.search_paths.clone() {
            if path.exists() {
                if let Ok(entries) = std::fs::read_dir(path) {
                    for entry in entries.flatten() {
                        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                            if let Ok(skill) = self.load_skill_from_dir(&entry.path()) {
                                self.skills.insert(skill.id.clone(), skill);
                                count += 1;
                            }
                        }
                    }
                }
            }
        }
        Ok(count)
    }

    /// Load a single skill from a directory.
    fn load_skill_from_dir(&self, dir: &Path) -> Result<Skill> {
        let skill_md = dir.join("SKILL.md");
        if !skill_md.exists() {
            anyhow::bail!("No SKILL.md found in {}", dir.display());
        }

        let content = std::fs::read_to_string(&skill_md)
            .context(format!("Failed to read {}", skill_md.display()))?;

        let id = dir.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        // Extract name from first heading or use id
        let name = content.lines()
            .find(|l| l.starts_with("# "))
            .map(|l| l.trim_start_matches("# ").to_string())
            .unwrap_or_else(|| id.clone());

        // Find companion files (referenced as relative paths)
        let companion_files: Vec<PathBuf> = content.lines()
            .filter(|l| l.contains("scripts/") || l.contains("tools/") || l.contains("references/"))
            .filter_map(|l| {
                let trimmed = l.trim();
                if let Some(start) = trimmed.find('`') {
                    let end = trimmed[start+1..].find('`')?;
                    let path = &trimmed[start+1..start+1+end];
                    let full = dir.join(path);
                    if full.exists() { Some(full) } else { None }
                } else {
                    None
                }
            })
            .collect();

        Ok(Skill {
            id: id.clone(),
            name,
            content,
            path: dir.to_path_buf(),
            companion_files,
        })
    }

    /// Get a skill by id.
    pub fn get(&self, id: &str) -> Option<&Skill> {
        self.skills.get(id)
    }

    /// List all available skill ids.
    pub fn list_ids(&self) -> Vec<&String> {
        self.skills.keys().collect()
    }

    /// List all skills with names.
    pub fn list(&self) -> Vec<(&String, &String)> {
        self.skills.iter().map(|(id, s)| (id, &s.name)).collect()
    }

    /// Get a compact summary for prompt injection.
    pub fn catalog_summary(&self) -> String {
        if self.skills.is_empty() {
            return "No skills available.".to_string();
        }

        let mut out = String::from("## Available Skills\n");
        out.push_str("Use `load_skill <id>` to activate a skill's instructions.\n\n");
        for skill in self.skills.values() {
            // Extract first sentence of content for description
            let desc = skill.content.lines()
                .find(|l| !l.starts_with('#') && !l.is_empty())
                .unwrap_or(&skill.name)
                .trim()
                .to_string();
            let short_desc = if desc.len() > 100 {
                format!("{}...", &desc[..100])
            } else {
                desc
            };
            out.push_str(&format!("- **{}**: {}\n", skill.id, short_desc));
        }
        out
    }
}

/// Tool for loading a skill into the agent's active context.
/// Equivalent to OpenClaw's `load_skill`.
pub struct SkillLoaderTool {
    registry: std::sync::Arc<std::sync::Mutex<SkillRegistry>>,
}

impl SkillLoaderTool {
    pub fn new(registry: std::sync::Arc<std::sync::Mutex<SkillRegistry>>) -> Self {
        Self { registry }
    }

    pub fn load_skill_content(&self, skill_id: &str) -> Result<String> {
        let reg = self.registry.lock().unwrap();
        if let Some(skill) = reg.get(skill_id) {
            let mut output = format!(
                "## Skill: {} ({})\n\n{}",
                skill.name, skill.id, skill.content
            );
            if !skill.companion_files.is_empty() {
                output.push_str("\n\n### Companion Files\n");
                for f in &skill.companion_files {
                    output.push_str(&format!("- {}\n", f.display()));
                }
            }
            Ok(output)
        } else {
            // Try rescan
            drop(reg);
            let mut reg = self.registry.lock().unwrap();
            reg.scan().ok();
            if let Some(skill) = reg.get(skill_id) {
                Ok(format!(
                    "## Skill: {} ({})\n\n{}",
                    skill.name, skill.id, skill.content
                ))
            } else {
                let available: Vec<String> = reg.list_ids().iter().map(|s| s.to_string()).collect();
                anyhow::bail!(
                    "Skill '{}' not found. Available: {}",
                    skill_id,
                    available.join(", ")
                )
            }
        }
    }
}

#[async_trait::async_trait]
impl pharmakon_common::Tool for SkillLoaderTool {
    fn name(&self) -> &str { "load_skill" }

    fn description(&self) -> &str {
        "Load a domain-specific skill's instructions into the agent's working context. \
         Use to activate specialized knowledge (e.g., 'github', 'coding-agent'). \
         List available skills first with the catalog."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "skill_id": { "type": "string", "description": "Skill identifier to load" }
            },
            "required": ["skill_id"]
        })
    }

    async fn call(&self, args: serde_json::Value) -> pharmakon_common::AgentResult<String> {
        let skill_id = args["skill_id"].as_str().unwrap_or_default();
        self.load_skill_content(skill_id)
            .map_err(|e| pharmakon_common::AgentError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_registry_scan() {
        let tmp = tempfile::TempDir::new().unwrap();
        let skill_dir = tmp.path().join("test-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "# Test Skill\n\nThis is a test skill for GitHub operations.\n\nUse `scripts/setup.sh` to configure.",
        ).unwrap();

        let mut registry = SkillRegistry::new();
        registry.add_search_path(tmp.path().to_path_buf());
        let count = registry.scan().unwrap();
        assert_eq!(count, 1);

        let skill = registry.get("test-skill").unwrap();
        assert_eq!(skill.name, "Test Skill");
        assert!(skill.content.contains("GitHub operations"));
    }

    #[test]
    fn test_skill_catalog_summary() {
        let mut registry = SkillRegistry::new();
        registry.skills.insert(
            "github".to_string(),
            Skill {
                id: "github".to_string(),
                name: "GitHub Operations".to_string(),
                content: "# GitHub\n\nManage issues, PRs, and repositories.".to_string(),
                path: PathBuf::from("/tmp"),
                companion_files: vec![],
            },
        );

        let summary = registry.catalog_summary();
        assert!(summary.contains("github"));
        assert!(summary.contains("Manage issues"));
    }
}
