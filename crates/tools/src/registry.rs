use std::sync::Arc;
use pharmakon_common::{Tool, AgentModel, CommitmentPersistence, SoulManager, Event};
use tokio::sync::broadcast;
use crate::*;

pub struct ToolRegistry;

pub struct ToolDependencies {
    pub model: Option<Arc<dyn AgentModel>>,
    pub store: Option<Arc<dyn CommitmentPersistence>>,
    pub soul_manager: Option<Arc<dyn SoulManager>>,
    pub event_tx: Option<broadcast::Sender<Event>>,
}

impl ToolRegistry {
    pub fn get_tool(name: &str, deps: &ToolDependencies) -> Option<Arc<dyn Tool>> {
        match name {
            "browser" => Some(Arc::new(BrowserTool::new(None))),
            "brave_search" => {
                let api_key = std::env::var("BRAVE_API_KEY").ok()?;
                Some(Arc::new(BraveSearchTool::new(api_key)))
            }
            "shell" => Some(Arc::new(ShellTool)),
            "read_file" => Some(Arc::new(FileReadTool)),
            "terminal" => Some(Arc::new(TerminalTool::new())),
            "screenshot" => Some(Arc::new(ScreenshotTool)),
            "camera" => Some(Arc::new(CameraTool)),
            "web_fetch" => Some(Arc::new(WebFetchTool::new())),
            "link_understanding" => Some(Arc::new(LinkUnderstandingTool::new())),
            "media_understanding" => {
                deps.model.as_ref().map(|m| Arc::new(MediaUnderstandingTool::new(m.clone())) as Arc<dyn Tool>)
            }
            "canvas" => {
                deps.event_tx.as_ref().map(|tx| Arc::new(CanvasTool::new(tx.clone())) as Arc<dyn Tool>)
            }
            "commitment" => {
                deps.store.as_ref().map(|s| Arc::new(CommitmentTool::new(s.clone())) as Arc<dyn Tool>)
            }
            "context_connector" => Some(Arc::new(ContextConnectorTool::new())),
            "soul_manager" => {
                deps.soul_manager.as_ref().map(|m| Arc::new(SoulTool::new(m.clone())) as Arc<dyn Tool>)
            }
            _ => None,
        }
    }
}
