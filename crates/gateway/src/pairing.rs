use anyhow::{Result, anyhow};
use rand::Rng;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub struct PairingManager {
    pending_pairings: HashMap<String, String>, // channel_id -> code
    approved_users: Vec<String>,               // list of user_ids
    storage_path: PathBuf,
}

impl Default for PairingManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PairingManager {
    pub fn new() -> Self {
        let home = dirs::home_dir().expect("Could not find home directory");
        let storage_path = home.join(".pharmakon").join("approved_users.json");

        let approved_users = if storage_path.exists() {
            let content = fs::read_to_string(&storage_path).unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Vec::new()
        };

        Self {
            pending_pairings: HashMap::new(),
            approved_users,
            storage_path,
        }
    }

    pub fn generate_code(&mut self, channel_id: &str) -> String {
        let code: String = (0..6)
            .map(|_| rand::thread_rng().gen_range(0..10).to_string())
            .collect();
        self.pending_pairings
            .insert(channel_id.to_string(), code.clone());
        log::info!("Generated pairing code {} for channel {}", code, channel_id);
        code
    }

    pub fn approve(&mut self, channel_id: &str, code: &str) -> Result<()> {
        if let Some(expected_code) = self.pending_pairings.get(channel_id)
            && expected_code == code
        {
            if !self.approved_users.contains(&channel_id.to_string()) {
                self.approved_users.push(channel_id.to_string());
                self.save()?;
            }
            self.pending_pairings.remove(channel_id);
            return Ok(());
        }
        Err(anyhow!("Invalid pairing code"))
    }

    pub fn is_approved(&self, channel_id: &str) -> bool {
        self.approved_users.contains(&channel_id.to_string())
    }

    fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(&self.approved_users)?;
        fs::write(&self.storage_path, content)?;
        Ok(())
    }
}
