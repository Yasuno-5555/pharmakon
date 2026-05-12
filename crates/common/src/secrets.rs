use anyhow::{Result, anyhow};
use keyring::Entry;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub struct SecretStore {
    service: String,
}

impl Default for SecretStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretStore {
    pub fn new() -> Self {
        Self {
            service: "pharmakon".to_string(),
        }
    }

    fn get_fallback_path(&self) -> Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| anyhow!("Home directory not found"))?;
        let path = home.join(".pharmakon").join("secrets.json");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(path)
    }

    fn save_to_fallback(&self, name: &str, value: &str) -> Result<()> {
        let path = self.get_fallback_path()?;
        let mut secrets: HashMap<String, String> = if path.exists() {
            let content = fs::read_to_string(&path)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            HashMap::new()
        };
        secrets.insert(name.to_string(), value.to_string());
        let content = serde_json::to_string_pretty(&secrets)?;
        fs::write(&path, content)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = fs::metadata(&path) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o600);
                let _ = fs::set_permissions(&path, perms);
            }
        }
        Ok(())
    }

    fn get_from_fallback(&self, name: &str) -> Result<String> {
        let path = self.get_fallback_path()?;
        if !path.exists() {
            return Err(anyhow!("Secret storage not initialized (file not found)"));
        }
        let content = fs::read_to_string(&path)?;
        let secrets: HashMap<String, String> = serde_json::from_str(&content)?;
        secrets
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow!("Secret '{}' not found in storage", name))
    }

    pub fn set_secret(&self, name: &str, value: &str) -> Result<()> {
        // 1. Always save to fallback file for reliability
        if let Err(e) = self.save_to_fallback(name, value) {
            log::error!("Failed to save secret '{}' to fallback file: {}", name, e);
        }

        // 2. Also try keyring for an additional layer of security
        #[allow(clippy::collapsible_if)]
        if let Ok(entry) = Entry::new(&self.service, name) {
            if let Err(e) = entry.set_password(value) {
                log::warn!(
                    "Keyring set failed for '{}': {}. Fallback file will be used.",
                    name,
                    e
                );
            }
        }
        Ok(())
    }

    pub fn get_secret(&self, name: &str) -> Result<String> {
        // 1. Try keyring first
        if let Ok(password) = Entry::new(&self.service, name).and_then(|e| e.get_password()) {
            return Ok(password);
        }

        // 2. Fallback to file storage
        self.get_from_fallback(name)
    }

    pub fn delete_secret(&self, name: &str) -> Result<()> {
        if let Ok(entry) = Entry::new(&self.service, name) {
            let _ = entry.delete_credential();
        }

        let path = self.get_fallback_path()?;
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            let mut secrets: HashMap<String, String> =
                serde_json::from_str(&content).unwrap_or_default();
            secrets.remove(name);
            let content = serde_json::to_string_pretty(&secrets)?;
            fs::write(&path, content)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(metadata) = fs::metadata(&path) {
                    let mut perms = metadata.permissions();
                    perms.set_mode(0o600);
                    let _ = fs::set_permissions(&path, perms);
                }
            }
        }
        Ok(())
    }
}
