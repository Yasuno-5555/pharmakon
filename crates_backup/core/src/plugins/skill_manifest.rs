use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub entrypoint: String,
    pub permissions: Vec<Permission>,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    Network,
    Filesystem(String),
    Environment(String),
    SystemInfo,
}

impl SkillManifest {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("Skill name cannot be empty".to_string());
        }
        if self.entrypoint.is_empty() {
            return Err("Skill entrypoint cannot be empty".to_string());
        }
        Ok(())
    }
}
