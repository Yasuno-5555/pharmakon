use crate::agent::Agent;
use anyhow::{Context, Result};
use notify::{Event, RecursiveMode, Watcher};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

pub mod markdown;

pub struct SkillLoader {
    skills_dir: PathBuf,
}

impl SkillLoader {
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().context("Could not find home directory")?;
        let skills_dir = home.join(".pharmakon").join("skills");
        if !skills_dir.exists() {
            fs::create_dir_all(&skills_dir)?;
        }
        Ok(Self { skills_dir })
    }

    pub async fn start_watching(&self, agent: Arc<Mutex<Agent>>) -> Result<()> {
        let skills_dir = self.skills_dir.clone();
        let agent_clone = agent.clone();

        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            if let Ok(event) = res
                && (event.kind.is_modify() || event.kind.is_create()) {
                    log::info!("Skills directory changed, reloading...");
                    let agent_for_task = agent_clone.clone();
                    let skills_dir_for_task = skills_dir.clone();

                    tokio::spawn(async move {
                        if let Err(e) =
                            Self::load_skills_internal(&skills_dir_for_task, &agent_for_task).await
                        {
                            log::error!("Failed to reload skills: {}", e);
                        }
                    });
                }
        })?;

        watcher.watch(&self.skills_dir, RecursiveMode::NonRecursive)?;

        // Initial load
        self.load_skills(&agent).await?;

        // Keep watcher alive
        Box::leak(Box::new(watcher));

        Ok(())
    }

    pub async fn load_skills(&self, agent: &Arc<Mutex<Agent>>) -> Result<()> {
        Self::load_skills_internal(&self.skills_dir, agent).await
    }

    async fn load_skills_internal(skills_dir: &PathBuf, _agent: &Arc<Mutex<Agent>>) -> Result<()> {
        let entries = fs::read_dir(skills_dir)?;
        for entry in entries {
            let entry = entry?;
            let _path = entry.path();

        }
        Ok(())
    }
}
