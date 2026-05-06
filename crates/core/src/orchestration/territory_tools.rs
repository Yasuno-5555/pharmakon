use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool};
use serde_json::{Value, json};
use std::sync::Arc;

pub struct LockTerritoryTool {
    pub territory_manager: Arc<crate::orchestration::territory::TerritoryManager>,
}

#[async_trait]
impl Tool for LockTerritoryTool {
    fn name(&self) -> &str {
        "lock_territory"
    }
    fn description(&self) -> &str {
        "Lock a specific path (directory or file) to prevent other agents from working on it simultaneously. Essential for Symphony coordination."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "The path to lock (e.g., 'crates/core' or 'src/utils.rs')" }
            },
            "required": ["path"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let path = args["path"].as_str().unwrap_or_default();
        match self.territory_manager.lock_path(path).await {
            Ok(_) => Ok(format!(
                "Territory '{}' successfully locked for your session.",
                path
            )),
            Err(e) => Err(AgentError(e.to_string())),
        }
    }
}

pub struct UnlockTerritoryTool {
    pub territory_manager: Arc<crate::orchestration::territory::TerritoryManager>,
}

#[async_trait]
impl Tool for UnlockTerritoryTool {
    fn name(&self) -> &str {
        "unlock_territory"
    }
    fn description(&self) -> &str {
        "Unlock a previously locked path, making it available for other agents."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "The path to unlock" }
            },
            "required": ["path"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let path = args["path"].as_str().unwrap_or_default();
        self.territory_manager.unlock_path(path).await;
        Ok(format!(
            "Territory '{}' is now unlocked and available.",
            path
        ))
    }
}

pub struct ListTerritoriesTool {
    pub territory_manager: Arc<crate::orchestration::territory::TerritoryManager>,
}

#[async_trait]
impl Tool for ListTerritoriesTool {
    fn name(&self) -> &str {
        "list_territories"
    }
    fn description(&self) -> &str {
        "List all currently locked territories and their active agents."
    }
    fn parameters(&self) -> Value {
        json!({})
    }

    async fn call(&self, _args: Value) -> AgentResult<String> {
        let locks = self.territory_manager.get_all_locks().await;
        if locks.is_empty() {
            Ok("No active territory locks.".to_string())
        } else {
            Ok(format!(
                "### Active Territory Locks:\n- {}",
                locks.join("\n- ")
            ))
        }
    }
}
