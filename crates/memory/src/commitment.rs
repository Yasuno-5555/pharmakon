use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Commitment {
    pub id: String,
    pub description: String,
    pub deadline: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub status: CommitmentStatus,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum CommitmentStatus {
    Pending,
    InProgress,
    Completed,
    Cancelled,
    Failed(String),
}

pub struct CommitmentStore {
    path: PathBuf,
}

impl CommitmentStore {
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().expect("Could not find home directory");
        let path = home.join(".pharmakon").join("commitments.json");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(Self { path })
    }

    pub fn save(&self, commitments: &[Commitment]) -> Result<()> {
        let content = serde_json::to_string_pretty(commitments)?;
        fs::write(&self.path, content)?;
        Ok(())
    }

    pub fn load(&self) -> Result<Vec<Commitment>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&self.path)?;
        Ok(serde_json::from_str(&content)?)
    }

    pub fn add(&self, commitment: Commitment) -> Result<()> {
        let mut all = self.load()?;
        all.push(commitment);
        self.save(&all)
    }

    pub fn update_status(&self, id: &str, status: CommitmentStatus) -> Result<()> {
        let mut all = self.load()?;
        if let Some(c) = all.iter_mut().find(|c| c.id == id) {
            c.status = status;
        }
        self.save(&all)
    }
}
