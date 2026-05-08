use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub mod topology;

/// `identity.yml`: The core persona and operational rules of the agent.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IdentityContext {
    pub name: String,
    pub version: String,
    pub purpose: String,
    pub traits: Vec<String>,
    pub core_directives: Vec<String>,
}

/// `user.yml`: Information about the user interacting with the agent.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserContext {
    pub name: Option<String>,
    pub preferences: HashMap<String, String>,
    pub environment: HashMap<String, String>,
    pub background: Option<String>,
}

/// Represents learned notes for a specific tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolNote {
    pub usage_guidelines: Vec<String>,
    pub known_quirks: Vec<String>,
}

/// `tools.yml`: Dynamic knowledge and notes about how to use tools effectively.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolContext {
    pub tool_notes: HashMap<String, ToolNote>,
    pub general_heuristics: Vec<String>,
}

/// Manages the loading, saving, and accessing of dynamic YAML contexts.
pub struct ContextManager {
    base_dir: PathBuf,
    pub identity: IdentityContext,
    pub user: UserContext,
    pub tools: ToolContext,
}

impl ContextManager {
    /// Initializes the manager, creating default files if they don't exist.
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Result<Self> {
        let base_dir = base_dir.as_ref().to_path_buf();
        if !base_dir.exists() {
            fs::create_dir_all(&base_dir).context("Failed to create context directory")?;
        }

        let mut manager = Self {
            base_dir,
            identity: IdentityContext::default(),
            user: UserContext::default(),
            tools: ToolContext::default(),
        };

        manager.load_all()?;
        Ok(manager)
    }

    pub fn load_all(&mut self) -> Result<()> {
        self.identity = self.load_yaml("identity.yml").unwrap_or_default();
        self.user = self.load_yaml("user.yml").unwrap_or_default();
        self.tools = self.load_yaml("tools.yml").unwrap_or_default();

        // Enforce default workspace
        let mut modified = false;
        if !self.user.environment.contains_key("default_workspace") {
            let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));
            let default_ws = home.join(".pharmakon").join("workspace");
            self.user.environment.insert("default_workspace".to_string(), default_ws.to_string_lossy().to_string());
            modified = true;
        }

        if modified {
            let _ = self.save_all();
        }

        Ok(())
    }

    pub fn save_all(&self) -> Result<()> {
        self.save_yaml("identity.yml", &self.identity)?;
        self.save_yaml("user.yml", &self.user)?;
        self.save_yaml("tools.yml", &self.tools)?;
        Ok(())
    }

    fn load_yaml<T: for<'de> Deserialize<'de>>(&self, filename: &str) -> Result<T> {
        let path = self.base_dir.join(filename);
        let content = fs::read_to_string(&path)?;
        let data: T = serde_yaml::from_str(&content)?;
        Ok(data)
    }

    fn save_yaml<T: Serialize>(&self, filename: &str, data: &T) -> Result<()> {
        let path = self.base_dir.join(filename);
        let content = serde_yaml::to_string(data)?;
        fs::write(&path, content)?;
        Ok(())
    }

    /// Renders the combined context for prompt injection.
    pub fn render_prompt_context(&self) -> String {
        let mut context = String::new();

        // 1. Identity
        context.push_str("### YOUR IDENTITY (identity.yml)\n");
        context.push_str(&format!("Name: {}\n", self.identity.name));
        context.push_str(&format!("Purpose: {}\n", self.identity.purpose));
        if !self.identity.traits.is_empty() {
            context.push_str(&format!("Traits: [{}]\n", self.identity.traits.join(", ")));
        }
        if !self.identity.core_directives.is_empty() {
            context.push_str("Core Directives:\n");
            for dir in &self.identity.core_directives {
                context.push_str(&format!("- {}\n", dir));
            }
        }
        context.push_str("\n");

        // 2. User Context
        context.push_str("### USER CONTEXT (user.yml)\n");
        if let Some(name) = &self.user.name {
            context.push_str(&format!("User Name: {}\n", name));
        }
        if let Some(bg) = &self.user.background {
            context.push_str(&format!("Background: {}\n", bg));
        }
        if !self.user.preferences.is_empty() {
            context.push_str("Preferences:\n");
            for (k, v) in &self.user.preferences {
                context.push_str(&format!("  - {}: {}\n", k, v));
            }
        }
        if !self.user.environment.is_empty() {
            context.push_str("Environment:\n");
            for (k, v) in &self.user.environment {
                context.push_str(&format!("  - {}: {}\n", k, v));
            }
        }
        context.push_str("\n");

        // 3. Tool Context (Dynamic Learned Usage)
        if !self.tools.general_heuristics.is_empty() || !self.tools.tool_notes.is_empty() {
            context.push_str("### LEARNED TOOL USAGE (tools.yml)\n");
            if !self.tools.general_heuristics.is_empty() {
                context.push_str("General Heuristics:\n");
                for h in &self.tools.general_heuristics {
                    context.push_str(&format!("- {}\n", h));
                }
            }
            if !self.tools.tool_notes.is_empty() {
                context.push_str("Specific Tool Notes:\n");
                for (tool, notes) in &self.tools.tool_notes {
                    context.push_str(&format!("- **{}**:\n", tool));
                    for guideline in &notes.usage_guidelines {
                        context.push_str(&format!("  - Guideline: {}\n", guideline));
                    }
                    for quirk in &notes.known_quirks {
                        context.push_str(&format!("  - Quirk: {}\n", quirk));
                    }
                }
            }
            context.push_str("\n");
        }

        context
    }
}
