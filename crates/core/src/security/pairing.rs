use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use anyhow::{Result, anyhow};
use serde::{Serialize, Deserialize};
use once_cell::sync::Lazy;
use rand::Rng;

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

static INSTANCE: Lazy<PairingManager> = Lazy::new(|| {
    PairingManager {
        allowed_senders: Mutex::new(HashSet::new()),
        pending_pairings: Mutex::new(HashMap::new()),
    }
});

impl PairingManager {
    pub fn global() -> &'static Self {
        &*INSTANCE
    }

    pub fn is_allowed(&self, channel: &str, sender: &str) -> bool {
        let key = format!("{}:{}", channel, sender);
        self.allowed_senders.lock().unwrap().contains(&key)
    }

    pub fn initiate_pairing(&self, channel: &str, sender: &str) -> String {
        let key = format!("{}:{}", channel, sender);
        let code = format!("{:06}", rand::thread_rng().gen_range(0..1000000));
        let pairing = PendingPairing {
            channel: channel.to_string(),
            sender: sender.to_string(),
            code: code.clone(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(10),
        };
        self.pending_pairings.lock().unwrap().insert(key, pairing);
        code
    }

    pub fn approve_pairing(&self, channel: &str, code: &str) -> Result<String> {
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
            self.allowed_senders.lock().unwrap().insert(key.clone());
            Ok(key)
        } else {
            Err(anyhow!("Invalid pairing code"))
        }
    }

    pub fn add_allowed_sender(&self, channel: &str, sender: &str) {
        let key = format!("{}:{}", channel, sender);
        self.allowed_senders.lock().unwrap().insert(key);
    }
}
