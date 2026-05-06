use crate::system_prompt::SystemPromptContribution;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MarkdownSkillMetadata {
    pub name: String,
    pub description: String,
}

pub struct MarkdownSkill {
    pub metadata: MarkdownSkillMetadata,
    pub content: String,
}

impl MarkdownSkill {
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        if !text.starts_with("---") {
            return Err(anyhow::anyhow!("Missing frontmatter in markdown skill"));
        }

        let parts: Vec<&str> = text.splitn(3, "---").collect();
        if parts.len() < 3 {
            return Err(anyhow::anyhow!("Invalid markdown skill format"));
        }

        let metadata: MarkdownSkillMetadata = serde_yaml::from_str(parts[1])?;
        let content = parts[2].trim().to_string();

        Ok(Self { metadata, content })
    }
}

pub struct MarkdownSkillContribution {
    pub name: String,
    pub content: String,
}

impl MarkdownSkillContribution {
    pub fn new(name: &str, content: &str) -> Self {
        Self {
            name: name.to_string(),
            content: content.to_string(),
        }
    }
}

impl SystemPromptContribution for MarkdownSkillContribution {
    fn name(&self) -> &str {
        &self.name
    }
    fn get_content(&self) -> String {
        self.content.clone()
    }
}
