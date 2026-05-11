use pharmakon_common::{AgentModel, CommitmentPersistence, Event, SoulManager, Tool, ToolMeta};
pub use pharmakon_common::tool_meta_catalog::ToolMetaCatalog;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

pub struct ToolDependencies {
    pub model: Option<Arc<dyn AgentModel>>,
    pub store: Option<Arc<dyn CommitmentPersistence>>,
    pub soul_manager: Option<Arc<dyn SoulManager>>,
    pub event_tx: Option<broadcast::Sender<Event>>,
    pub nexus: Option<Arc<pharmakon_memory::weaver::KnowledgeNexus>>,
    pub vision_stream:
        Option<Arc<tokio::sync::Mutex<crate::media::vision_stream::VisionRingBuffer>>>,
    pub total_tokens: Option<Arc<std::sync::atomic::AtomicU64>>,
    pub total_cost: Option<Arc<tokio::sync::Mutex<f64>>>,
}

pub struct ToolMetaRegistry {
    pub catalog: ToolMetaCatalog,
    loaded: HashMap<String, Arc<dyn Tool>>,
    deps: ToolDependencies,
}

impl ToolMetaRegistry {
    pub fn new(deps: ToolDependencies) -> Self {
        let catalog = crate::tool_meta_catalog::build_default_catalog();
        Self {
            catalog,
            loaded: HashMap::new(),
            deps,
        }
    }

    /// Search for relevant tools using BM25.
    pub fn search(&self, query: &str, top_k: usize) -> Vec<ToolMeta> {
        self.catalog
            .search(query, top_k)
            .into_iter()
            .map(|r| r.meta)
            .collect()
    }

    /// Hydrate (load) a tool by name on demand.
    pub fn hydrate(&mut self, name: &str) -> Option<Arc<dyn Tool>> {
        if let Some(tool) = self.loaded.get(name) {
            return Some(tool.clone());
        }

        let tool = self.get_tool_internal(name)?;
        self.loaded.insert(name.to_string(), tool.clone());
        Some(tool)
    }

    /// Get all metadata in the catalog.
    pub fn all_metadata(&self) -> &[ToolMeta] {
        self.catalog.all()
    }

    /// Get a compact summary of the catalog for prompt injection.
    pub fn catalog_summary(&self) -> String {
        self.catalog.catalog_summary()
    }

    /// Update tool dependencies.
    pub fn update_deps<F>(&mut self, f: F)
    where
        F: FnOnce(&mut ToolDependencies),
    {
        f(&mut self.deps);
        // Clear loaded cache to force re-hydration with new deps if needed?
        // Or should we keep them? Usually deps change only during initialization.
        self.loaded.clear();
    }

    pub fn add_tool(&mut self, tool: Arc<dyn Tool>) {
        self.loaded.insert(tool.name().to_string(), tool);
    }

    pub fn get_loaded(&self) -> Vec<Arc<dyn Tool>> {
        self.loaded.values().cloned().collect()
    }

    pub fn get_tool(&mut self, name: &str) -> Option<Arc<dyn Tool>> {
        self.hydrate(name)
    }

    fn get_tool_internal(&self, name: &str) -> Option<Arc<dyn Tool>> {
        let deps = &self.deps;
        match name {
            "browser" => Some(Arc::new(crate::browser::BrowserTool::new(None))),
            "native_gui_emulator" => Some(Arc::new(crate::gui::NativeGuiEmulatorTool::new())),
            "brave_search" => {
                let api_key = std::env::var("BRAVE_API_KEY").ok()?;
                Some(Arc::new(crate::web_search::BraveSearchTool::new(api_key)))
            }
            "grep_files" | "grep_search" => Some(Arc::new(crate::code::GrepSearchTool)),
            "git_status" => Some(Arc::new(crate::git::GitStatusTool)),
            "git_diff" => Some(Arc::new(crate::git::GitDiffTool)),
            "git_add" => Some(Arc::new(crate::git::GitAddTool)),
            "git_commit" => Some(Arc::new(crate::git::GitCommitTool)),
            "git_log" => Some(Arc::new(crate::git::GitLogTool)),
            "git_branch" => Some(Arc::new(crate::git::GitBranchTool)),
            "gemini_search" => Some(Arc::new(crate::web_search::GoogleSearchTool)),
            "lsp" => Some(Arc::new(crate::lsp::LspTool::new())),
            "shell" => Some(Arc::new(crate::terminal::ShellTool)),
            "read_file" => Some(Arc::new(crate::files::FileReadTool)),
            "write_file" => Some(Arc::new(crate::files::FileWriteTool)),
            "replace_content" | "replace_file_content" => Some(Arc::new(crate::code::StrictReplaceContentTool)),
            "view_file" => Some(Arc::new(crate::code::ViewFileTool)),
            "list_dir" => Some(Arc::new(crate::code::ListDirTool)),
            "apply_patch" => Some(Arc::new(crate::files::ApplyPatchTool)),
            "terminal" => Some(Arc::new(crate::terminal::TerminalTool::new())),
            "run_background" => Some(Arc::new(crate::terminal::BackgroundRunTool::new())),
            "get_process_status" => Some(Arc::new(crate::terminal::ProcessStatusTool::new())),
            "send_command_input" => Some(Arc::new(crate::terminal::SendCommandInputTool::new())),
            "run_command" | "command" => Some(Arc::new(crate::terminal::ShellTool)),
            "screenshot" => Some(Arc::new(crate::media::capture::ScreenshotTool)),
            "camera" => Some(Arc::new(crate::media::capture::CameraTool)),
            "generate_image" => Some(Arc::new(crate::media::ImageGenTool::new())),
            "web_fetch" => Some(Arc::new(crate::web_fetch::WebFetchTool::new())),
            "custom_scout" => Some(Arc::new(crate::search::custom_scout::CustomScoutTool)),
            "discover_tools" => Some(Arc::new(crate::tool_discovery::DiscoverToolsTool::new())),
            "hydrate_context" => Some(Arc::new(crate::memory_hydration::HydrateContextTool::new())),
            "playbook" => Some(Arc::new(crate::playbook::PlaybookTool::new())),
            "search" | "web_search" => Some(Arc::new(crate::web_search::SearchDispatcherTool::new())),
            "duckduckgo_search" => Some(Arc::new(crate::web_search::DuckDuckGoSearchTool::new())),
            "google_search" => Some(Arc::new(crate::web_search::GoogleSearchTool)),
            "repomap" | "get_repo_map" => Some(Arc::new(crate::repomap::RepoMapTool::new())),
            "task_tracker" => Some(Arc::new(crate::project_management::TaskTrackerTool::new())),
            "self_diagnostic" => Some(Arc::new(crate::diagnostic::DiagnosticTool {
                vision_stream: deps.vision_stream.clone(),
                telemetry: None,
                mcp_stats_source: "internal".to_string(),
                total_tokens: deps.total_tokens.clone(),
                total_cost: deps.total_cost.clone(),
            })),
            "workspace_perception" => {
                Some(Arc::new(crate::workspace::WorkspacePerceptionTool::new()))
            }
            "subagent" => deps.model.as_ref().map(|_m| {
                Arc::new(crate::subagent::SubAgentTool::new(Arc::new(
                    crate::subagent::NoopSpawner,
                ))) as Arc<dyn Tool>
            }),
            "link_understanding" => Some(Arc::new(
                crate::link_understanding::LinkUnderstandingTool::new(),
            )),
            "media_understanding" => deps.model.as_ref().map(|m| {
                Arc::new(crate::media_understanding::MediaUnderstandingTool::new(
                    m.clone(),
                    deps.nexus.clone(),
                )) as Arc<dyn Tool>
            }),
            "canvas" => deps
                .event_tx
                .as_ref()
                .map(|tx| Arc::new(crate::canvas::CanvasTool::new(tx.clone())) as Arc<dyn Tool>),
            "commitment" => deps.store.as_ref().map(|s| {
                Arc::new(crate::commitment_tool::CommitmentTool::new(s.clone())) as Arc<dyn Tool>
            }),
            "context_connector" => Some(Arc::new(crate::connectors::ContextConnectorTool::new())),
            "soul_manager" => deps
                .soul_manager
                .as_ref()
                .map(|m| Arc::new(crate::soul_tool::SoulTool::new(m.clone())) as Arc<dyn Tool>),
            "ingest_ast_knowledge" => deps.nexus.as_ref().map(|n| {
                Arc::new(crate::ast_ingest::ASTKnowledgeIngestTool::new(n.clone())) as Arc<dyn Tool>
            }),
            "checkpoint" => Some(Arc::new(crate::checkpoint::CheckpointTool)),
            "reflect" => Some(Arc::new(crate::reflection::ReflectionTool)),
            "route_tools" => Some(Arc::new(crate::orchestration::ToolRouterTool)),
            "memory_management" => Some(Arc::new(crate::memory_mgmt::MemoryManagementTool)),
            "regret_minimization" => Some(Arc::new(crate::cognitive::RegretMinimizationTool)),
            "temporal_awareness" => Some(Arc::new(crate::cognitive::TemporalAwarenessTool)),
            "failure_prediction" => Some(Arc::new(crate::cognitive::FailurePredictionTool)),
            "proactive_self_optimization" => {
                Some(Arc::new(crate::cognitive::ProactiveSelfOptimizationTool))
            }
            "ephemeral_red_team" => Some(Arc::new(crate::orchestration::EphemeralRedTeamTool)),
            "fractal_swarm" => Some(Arc::new(crate::orchestration::FractalSwarmTool)),
            "pharmakon_task" => Some(Arc::new(crate::orchestration::PharmakonTaskTool)),
            _ => None,
        }
    }
}

// Keep the old ToolRegistry for backward compatibility if needed, but it's better to migrate.
pub struct ToolRegistry;

impl ToolRegistry {
    pub fn get_tool(name: &str, deps: &ToolDependencies) -> Option<Arc<dyn Tool>> {
        // This is now just a wrapper around a temporary registry or the logic relocated above.
        let registry = ToolMetaRegistry::new(ToolDependencies {
            model: deps.model.clone(),
            store: deps.store.clone(),
            soul_manager: deps.soul_manager.clone(),
            event_tx: deps.event_tx.clone(),
            nexus: deps.nexus.clone(),
            vision_stream: deps.vision_stream.clone(),
            total_tokens: deps.total_tokens.clone(),
            total_cost: deps.total_cost.clone(),
        });
        registry.get_tool_internal(name)
    }
}