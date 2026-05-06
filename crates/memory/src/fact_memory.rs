use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Belief {
    pub id: String,
    pub claim: String,
    pub confidence: f32,
    pub evidence_sources: Vec<String>,
    pub contradictions: Vec<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

pub struct BeliefSystem {
    path: PathBuf,
    beliefs: HashMap<String, Belief>,
}

impl BeliefSystem {
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().expect("Could not find home directory");
        let path = home.join(".pharmakon").join("beliefs.json");

        let beliefs = if path.exists() {
            let content = fs::read_to_string(&path)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            HashMap::new()
        };

        Ok(Self { path, beliefs })
    }

    pub fn add_belief(&mut self, claim: &str, confidence: f32, source: &str) -> Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let belief = Belief {
            id: id.clone(),
            claim: claim.to_string(),
            confidence,
            evidence_sources: vec![source.to_string()],
            contradictions: Vec::new(),
            timestamp: chrono::Utc::now(),
        };
        self.beliefs.insert(id, belief);
        self.save()?;
        Ok(())
    }

    pub fn update_belief_confidence(&mut self, id: &str, delta: f32) -> Result<()> {
        if let Some(belief) = self.beliefs.get_mut(id) {
            belief.confidence = (belief.confidence + delta).clamp(0.0, 1.0);
            self.save()?;
        }
        Ok(())
    }

    pub fn get_belief(&self, id: &str) -> Option<&Belief> {
        self.beliefs.get(id)
    }

    pub fn all_beliefs(&self) -> Vec<Belief> {
        self.beliefs.values().cloned().collect()
    }

    fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(&self.beliefs)?;
        fs::write(&self.path, content)?;
        Ok(())
    }
}
