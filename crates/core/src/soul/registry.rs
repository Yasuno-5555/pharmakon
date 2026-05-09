use crate::soul::Soul;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub struct SoulRegistry {
    souls: HashMap<String, Soul>,
    souls_dir: PathBuf,
}

impl SoulRegistry {
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().context("Could not find home directory")?;
        let souls_dir = home.join(".pharmakon").join("souls");

        if !souls_dir.exists() {
            fs::create_dir_all(&souls_dir)?;
            // Create a default soul file
            let default_soul = Soul::default_soul();
            let yaml = serde_yaml::to_string(&default_soul)?;
            fs::write(souls_dir.join("default.yaml"), yaml)?;
        }

        let mut registry = Self {
            souls: HashMap::new(),
            souls_dir,
        };

        registry.reload()?;
        Ok(registry)
    }

    pub fn reload(&mut self) -> Result<()> {
        let entries = fs::read_dir(&self.souls_dir)?;
        self.souls.clear();

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|s| s.to_str())
                && (ext == "yaml" || ext == "yml")
                && let Ok(soul) = Soul::load_from_file(&path) {
                    let name = path.file_stem().unwrap().to_str().unwrap().to_string();
                    self.souls.insert(name, soul);
                }
        }
        Ok(())
    }

    pub fn get_soul(&self, name: &str) -> Option<&Soul> {
        self.souls.get(name)
    }

    pub fn list_souls(&self) -> Vec<String> {
        self.souls.keys().cloned().collect()
    }
}
