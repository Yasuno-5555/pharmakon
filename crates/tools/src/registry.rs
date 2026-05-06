use crate::*;
use pharmakon_common::{AgentModel, CommitmentPersistence, Event, SoulManager, Tool};
use std::sync::Arc;
use tokio::sync::broadcast;

pub struct ToolRegistry;

pub struct ToolDependencies {
    pub model: Option<Arc<dyn AgentModel>>,
    pub store: Option<Arc<dyn CommitmentPersistence>>,
    pub soul_manager: Option<Arc<dyn SoulManager>>,
    pub event_tx: Option<broadcast::Sender<Event>>,
    pub nexus: Option<Arc<pharmakon_memory::weaver::KnowledgeNexus>>,
}

impl ToolRegistry {
    pub fn get_tool(name: &str, deps: &ToolDependencies) -> Option<Arc<dyn Tool>> {
        match name {
            "browser" => Some(Arc::new(browser::BrowserTool::new(None))),
            "brave_search" => {
                let api_key = std::env::var("BRAVE_API_KEY").ok()?;
                Some(Arc::new(web_search::BraveSearchTool::new(api_key)))
            }
            "shell" => Some(Arc::new(terminal::ShellTool)),
            "read_file" => Some(Arc::new(files::FileReadTool)),
            "terminal" => Some(Arc::new(terminal::TerminalTool::new())),
            "screenshot" => Some(Arc::new(media::capture::ScreenshotTool)),
            "camera" => Some(Arc::new(media::capture::CameraTool)),
            "web_fetch" => Some(Arc::new(web_fetch::WebFetchTool::new())),
            "link_understanding" => {
                Some(Arc::new(link_understanding::LinkUnderstandingTool::new()))
            }
            "media_understanding" => deps.model.as_ref().map(|m| {
                Arc::new(media_understanding::MediaUnderstandingTool::new(
                    m.clone(),
                    deps.nexus.clone(),
                )) as Arc<dyn Tool>
            }),
            "canvas" => deps
                .event_tx
                .as_ref()
                .map(|tx| Arc::new(canvas::CanvasTool::new(tx.clone())) as Arc<dyn Tool>),
            "commitment" => deps.store.as_ref().map(|s| {
                Arc::new(commitment_tool::CommitmentTool::new(s.clone())) as Arc<dyn Tool>
            }),
            "context_connector" => Some(Arc::new(connectors::ContextConnectorTool::new())),
            "soul_manager" => deps
                .soul_manager
                .as_ref()
                .map(|m| Arc::new(soul_tool::SoulTool::new(m.clone())) as Arc<dyn Tool>),
            "ingest_ast_knowledge" => deps.nexus.as_ref().map(|n| {
                Arc::new(ast_ingest::ASTKnowledgeIngestTool::new(n.clone())) as Arc<dyn Tool>
            }),
            _ => None,
        }
    }
}
