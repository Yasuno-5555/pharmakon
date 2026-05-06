use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Fact {
    pub key: String,
    pub value: String,
    pub confidence: f32,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

pub struct FactMemory {
    path: PathBuf,
    facts: HashMap<String, Fact>,
}

impl FactMemory {
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().expect("Could not find home directory");
        let path = home.join(".pharmakon").join("facts.json");

        let facts = if path.exists() {
            let content = fs::read_to_string(&path)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            HashMap::new()
        };

        Ok(Self { path, facts })
    }

    pub fn set_fact(&mut self, key: &str, value: &str, confidence: f32) -> Result<()> {
        let fact = Fact {
            key: key.to_string(),
            value: value.to_string(),
            confidence,
            timestamp: chrono::Utc::now(),
        };
        self.facts.insert(key.to_string(), fact);
        self.save()?;
        Ok(())
    }

    pub fn get_fact(&self, key: &str) -> Option<&Fact> {
        self.facts.get(key)
    }

    pub fn all_facts(&self) -> Vec<Fact> {
        self.facts.values().cloned().collect()
    }

    fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(&self.facts)?;
        fs::write(&self.path, content)?;
        Ok(())
    }
}
