use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Procedure {
    pub id: String,
    pub intent: String,
    pub steps: Vec<String>,
    pub success_count: u32,
    pub failure_count: u32,
    pub last_used: chrono::DateTime<chrono::Utc>,
}

pub struct ProceduralStore {
    path: PathBuf,
    procedures: HashMap<String, Procedure>,
}

impl ProceduralStore {
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().expect("Could not find home directory");
        let path = home.join(".pharmakon").join("procedures.json");

        let procedures = if path.exists() {
            let content = fs::read_to_string(&path)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            HashMap::new()
        };

        Ok(Self { path, procedures })
    }

    pub fn record_success(&mut self, intent: &str, steps: Vec<String>) -> Result<()> {
        let entry = self
            .procedures
            .entry(intent.to_string())
            .or_insert_with(|| Procedure {
                id: uuid::Uuid::new_v4().to_string(),
                intent: intent.to_string(),
                steps: steps.clone(),
                success_count: 0,
                failure_count: 0,
                last_used: chrono::Utc::now(),
            });

        entry.success_count += 1;
        entry.steps = steps; // Update with latest successful steps
        entry.last_used = chrono::Utc::now();
        self.save()?;
        Ok(())
    }

    pub fn get_procedure(&self, intent: &str) -> Option<&Procedure> {
        self.procedures.get(intent)
    }

    fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(&self.procedures)?;
        fs::write(&self.path, content)?;
        Ok(())
    }
}
