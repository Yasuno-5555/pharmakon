use anyhow::{Result, anyhow};
use once_cell::sync::Lazy;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingPairing {
    pub channel: String,
    pub sender: String,
    pub code: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

pub struct PairingManager {
    allowed_senders: Mutex<HashSet<String>>, // Format: "channel:sender"
    pending_pairings: Mutex<HashMap<String, PendingPairing>>, // Key: "channel:sender"
}

static INSTANCE: Lazy<PairingManager> = Lazy::new(|| PairingManager {
    allowed_senders: Mutex::new(HashSet::new()),
    pending_pairings: Mutex::new(HashMap::new()),
});

impl PairingManager {
    pub fn global() -> &'static Self {
        let instance = &*INSTANCE;
        let _ = instance.load();
        instance
    }

    fn get_save_path() -> std::path::PathBuf {
        let home = dirs::home_dir().expect("Could not find home directory");
        home.join(".pharmakon").join("allowed_senders.json")
    }

    pub fn load(&self) -> Result<()> {
        let path = Self::get_save_path();
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            let senders: HashSet<String> = serde_json::from_str(&content)?;
            let mut lock = self.allowed_senders.lock().unwrap();
            *lock = senders;
        }
        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::get_save_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let lock = self.allowed_senders.lock().unwrap();
        let content = serde_json::to_string_pretty(&*lock)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    fn get_pending_path() -> std::path::PathBuf {
        let home = dirs::home_dir().expect("Could not find home directory");
        home.join(".pharmakon").join("pending_pairings.json")
    }

    pub fn load_pending(&self) -> Result<()> {
        let path = Self::get_pending_path();
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(path) {
                if let Ok(pending) = serde_json::from_str(&content) {
                    let mut lock = self.pending_pairings.lock().unwrap();
                    *lock = pending;
                }
            }
        }
        Ok(())
    }

    pub fn save_pending(&self) -> Result<()> {
        let path = Self::get_pending_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let lock = self.pending_pairings.lock().unwrap();
        let content = serde_json::to_string_pretty(&*lock)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn is_allowed(&self, channel: &str, sender: &str) -> bool {
        let key = format!("{}:{}", channel, sender);
        self.allowed_senders.lock().unwrap().contains(&key)
    }

    pub fn initiate_pairing(&self, channel: &str, sender: &str) -> String {
        let _ = self.load_pending();
        let key = format!("{}:{}", channel, sender);
        let code = format!("{:06}", rand::thread_rng().gen_range(0..1000000));
        let pairing = PendingPairing {
            channel: channel.to_string(),
            sender: sender.to_string(),
            code: code.clone(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(10),
        };
        self.pending_pairings.lock().unwrap().insert(key, pairing);
        let _ = self.save_pending();
        code
    }

    pub fn approve_pairing(&self, channel: &str, code: &str) -> Result<String> {
        let _ = self.load_pending();
        let mut pending = self.pending_pairings.lock().unwrap();
        let mut found_key = None;

        for (key, p) in pending.iter() {
            if p.channel == channel && p.code == code {
                if p.expires_at < chrono::Utc::now() {
                    return Err(anyhow!("Pairing code expired"));
                }
                found_key = Some(key.clone());
                break;
            }
        }

        if let Some(key) = found_key {
            pending.remove(&key);
            drop(pending);
            let _ = self.save_pending();

            self.allowed_senders.lock().unwrap().insert(key.clone());
            let _ = self.save();
            Ok(key)
        } else {
            Err(anyhow!("Invalid pairing code"))
        }
    }

    pub fn list_allowed(&self) -> Vec<String> {
        self.allowed_senders
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect()
    }

    pub fn revoke_pairing(&self, channel: &str, sender: &str) -> Result<()> {
        let key = format!("{}:{}", channel, sender);
        if self.allowed_senders.lock().unwrap().remove(&key) {
            self.save()?;
            Ok(())
        } else {
            Err(anyhow!("Pairing not found"))
        }
    }

    pub fn add_allowed_sender(&self, channel: &str, sender: &str) {
        let key = format!("{}:{}", channel, sender);
        self.allowed_senders.lock().unwrap().insert(key);
        let _ = self.save();
    }
}
