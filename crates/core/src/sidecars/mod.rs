pub mod browser_sandbox;
pub mod telemetry_capture;
use std::process::{Child, Command};
use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub struct SidecarManager {
    processes: Arc<Mutex<HashMap<String, Child>>>,
}

impl SidecarManager {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn start_sidecar(&self, name: String, command: String, args: Vec<String>) -> Result<()> {
        let child = Command::new(command)
            .args(args)
            .spawn()?;
        
        let mut processes = self.processes.lock().map_err(|_| anyhow!("Mutex error"))?;
        processes.insert(name, child);
        Ok(())
    }

    pub fn stop_sidecar(&self, name: &str) -> Result<()> {
        let mut processes = self.processes.lock().map_err(|_| anyhow!("Mutex error"))?;
        if let Some(mut child) = processes.remove(name) {
            child.kill()?;
        }
        Ok(())
    }
}
