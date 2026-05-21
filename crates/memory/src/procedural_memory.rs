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
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
        let path = home.join(".pharmakon").join("procedures.json");
        Self::new_with_path(path)
    }

    pub fn new_with_path(path: PathBuf) -> Result<Self> {
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

    pub fn record_failure(&mut self, intent: &str) -> Result<()> {
        let entry = self
            .procedures
            .entry(intent.to_string())
            .or_insert_with(|| Procedure {
                id: uuid::Uuid::new_v4().to_string(),
                intent: intent.to_string(),
                steps: Vec::new(),
                success_count: 0,
                failure_count: 0,
                last_used: chrono::Utc::now(),
            });

        entry.failure_count += 1;
        entry.last_used = chrono::Utc::now();
        self.save()?;
        Ok(())
    }

    pub fn get_procedure(&self, intent: &str) -> Option<&Procedure> {
        self.procedures.get(intent)
    }

    fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(&self.procedures)?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_procedural_store_record() {
        let unique_id = uuid::Uuid::new_v4().to_string();
        let path = std::env::temp_dir().join(format!("procedures_test_{}.json", unique_id));

        let mut store = ProceduralStore::new_with_path(path.clone()).unwrap();

        let steps = vec!["Step 1".to_string(), "Step 2".to_string()];
        assert!(store.record_success("compile_code", steps.clone()).is_ok());

        {
            let proc = store.get_procedure("compile_code").unwrap();
            assert_eq!(proc.intent, "compile_code");
            assert_eq!(proc.success_count, 1);
            assert_eq!(proc.failure_count, 0);
            assert_eq!(proc.steps, steps);
        }

        assert!(store.record_failure("compile_code").is_ok());
        {
            let proc = store.get_procedure("compile_code").unwrap();
            assert_eq!(proc.success_count, 1);
            assert_eq!(proc.failure_count, 1);
        }

        let reloaded_store = ProceduralStore::new_with_path(path.clone()).unwrap();
        let reloaded_proc = reloaded_store.get_procedure("compile_code").unwrap();
        assert_eq!(reloaded_proc.success_count, 1);
        assert_eq!(reloaded_proc.failure_count, 1);

        let _ = std::fs::remove_file(path);
    }
}
