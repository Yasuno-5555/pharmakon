use anyhow::{Result, Context};
use std::path::PathBuf;
use notify::{Watcher, RecursiveMode, Event};
use std::fs;
use pharmakon_tools::wasm_tool::WasmTool;
use std::sync::Arc;
use crate::agent::Agent;
use tokio::sync::Mutex;

pub mod markdown;
use markdown::{MarkdownSkill, MarkdownSkillContribution};

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
            if let Ok(event) = res {
                if event.kind.is_modify() || event.kind.is_create() {
                    log::info!("Skills directory changed, reloading...");
                    let agent_for_task = agent_clone.clone();
                    let skills_dir_for_task = skills_dir.clone();
                    
                    tokio::spawn(async move {
                        if let Err(e) = Self::load_skills_internal(&skills_dir_for_task, &agent_for_task).await {
                            log::error!("Failed to reload skills: {}", e);
                        }
                    });
                }
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

    async fn load_skills_internal(skills_dir: &PathBuf, agent: &Arc<Mutex<Agent>>) -> Result<()> {
        let entries = fs::read_dir(skills_dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            
            if path.extension().and_then(|s| s.to_str()) == Some("wasm") {
                log::info!("Loading WASM skill: {:?}", path);
                let wasm_bytes = fs::read(&path)?;
                let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").to_string();
                
                let tool = WasmTool::new(name, wasm_bytes);
                let mut agent_lock = agent.lock().await;
                agent_lock.add_tool(Arc::new(tool));
            } else if path.extension().and_then(|s| s.to_str()) == Some("md") {
                log::info!("Loading Markdown skill: {:?}", path);
                let content = fs::read_to_string(&path)?;
                match MarkdownSkill::parse(&content) {
                    Ok(skill) => {
                        let mut agent_lock = agent.lock().await;
                        agent_lock.prompt_manager.add_contribution(Box::new(MarkdownSkillContribution::new(
                            &skill.metadata.name,
                            &skill.content
                        )));
                        log::info!("Registered Markdown skill: {}", skill.metadata.name);
                    }
                    Err(e) => {
                        log::error!("Failed to parse markdown skill {:?}: {}", path, e);
                    }
                }
            }
        }
        Ok(())
    }
}
