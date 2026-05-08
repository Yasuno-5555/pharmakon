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
            "brave_search" => {
                let api_key = std::env::var("BRAVE_API_KEY").ok()?;
                Some(Arc::new(crate::web_search::BraveSearchTool::new(api_key)))
            }
            "google_search" => Some(Arc::new(crate::web_search::GoogleSearchTool)),
            "gemini_search" => Some(Arc::new(crate::web_search::GoogleSearchTool)),
            "lsp" => Some(Arc::new(crate::lsp::LspTool::new())),
            "shell" => Some(Arc::new(crate::terminal::ShellTool)),
            "read_file" => Some(Arc::new(crate::files::FileReadTool)),
            "write_file" => Some(Arc::new(crate::files::FileWriteTool)),
            "apply_patch" => Some(Arc::new(crate::files::ApplyPatchTool)),
            "terminal" => Some(Arc::new(crate::terminal::TerminalTool::new())),
            "screenshot" => Some(Arc::new(crate::media::capture::ScreenshotTool)),
            "camera" => Some(Arc::new(crate::media::capture::CameraTool)),
            "web_fetch" => Some(Arc::new(crate::web_fetch::WebFetchTool::new())),
            "custom_scout" => Some(Arc::new(crate::search::custom_scout::CustomScoutTool)),
            "discover_tools" => Some(Arc::new(crate::tool_discovery::DiscoverToolsTool::new())),
            "hydrate_context" => Some(Arc::new(crate::memory_hydration::HydrateContextTool::new())),
            "playbook" => Some(Arc::new(crate::playbook::PlaybookTool::new())),
            "repomap" => Some(Arc::new(crate::repomap::RepoMapTool::new())),
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
            "execution_trace" => Some(Arc::new(crate::codex::execution_trace::ExecutionTraceTool)),
            "deterministic_replay" => Some(Arc::new(crate::codex::deterministic_replay::DeterministicReplayTool)),
            "tool_reliability" => Some(Arc::new(crate::codex::tool_reliability::ToolReliabilityScoringTool)),
            "context_budget_optimizer" => Some(Arc::new(crate::codex::context_budget_optimizer::ContextBudgetOptimizerTool)),
            "dry_run" => Some(Arc::new(crate::codex::dry_run::DryRunTool)),
            "workspace_snapshot" => Some(Arc::new(crate::codex::workspace_snapshot::WorkspaceSnapshotTool)),
            "semantic_grep" => Some(Arc::new(crate::codex::semantic_grep::SemanticGrepTool)),
            "web_task" => Some(Arc::new(crate::codex::web_task::WebTaskTool)),
            "local_model_router" => Some(Arc::new(crate::codex::local_model_router::LocalModelRouterTool)),
            "skill_composition" => Some(Arc::new(crate::codex::skill_composition::SkillCompositionTool)),
            "failure_memory" => Some(Arc::new(crate::codex::failure_memory::FailureMemoryTool)),
            "proactive_intervention" => Some(Arc::new(crate::codex::proactive_intervention::ProactiveInterventionTool)),
            "cognitive_mirror" => Some(Arc::new(crate::codex::cognitive_mirror::CognitiveMirrorTool)),
            "intent_compiler" => Some(Arc::new(crate::codex::intent_compiler::IntentCompilerTool)),
            "regret_minimization" => Some(Arc::new(crate::codex::regret_minimization::RegretMinimizationTool)),
            "counterfactual_simulator" => Some(Arc::new(crate::codex::counterfactual_simulator::CounterfactualSimulatorTool)),
            "attention_router" => Some(Arc::new(crate::codex::attention_router::AttentionRouterTool)),
            "temporal_awareness" => Some(Arc::new(crate::codex::temporal_awareness::TemporalAwarenessTool)),
            "soft_dependency_graph" => Some(Arc::new(crate::codex::soft_dependency_graph::SoftDependencyGraphTool)),
            "autonomy_dial" => Some(Arc::new(crate::codex::autonomy_dial::AutonomyDialTool)),
            "failure_prediction" => Some(Arc::new(crate::codex::failure_prediction::FailurePredictionTool)),
            "ast_lsp_bridge" => Some(Arc::new(crate::codex::ast_lsp_bridge::AstLspBridgeTool)),
            "spec_first_test" => Some(Arc::new(crate::codex::spec_first_test::SpecFirstTestTool)),
            "semantic_conflict_resolution" => {
                Some(Arc::new(crate::codex::semantic_conflict_resolution::SemanticConflictResolutionTool))
            }
            "time_travel_debugger" => Some(Arc::new(crate::codex::time_travel_debugger::TimeTravelDebuggerTool)),
            "nexus_visualizer" => Some(Arc::new(crate::codex::nexus_visualizer::NexusVisualizerTool)),
            "proactive_self_optimization" => {
                Some(Arc::new(crate::codex::proactive_self_optimization::ProactiveSelfOptimizationTool))
            }
            "diff_security_auditor" => Some(Arc::new(crate::codex::diff_security_auditor::DiffSecurityAuditorTool)),
            "mutate_ast" => Some(Arc::new(crate::codex::ast_native_mutation::AstNativeMutationTool)),
            "mcts_simulator" => Some(Arc::new(crate::codex::mcts_simulator::MctsSimulatorTool)),
            "memory_actor_status" => Some(Arc::new(crate::codex::memory_actor_status::MemoryActorStatusTool)),
            "graph_prefetch" => Some(Arc::new(crate::codex::graph_prefetch::GraphPrefetchTool)),
            "rlfc" => Some(Arc::new(crate::codex::rlfc::RlfcTool)),
            "ephemeral_red_team" => Some(Arc::new(crate::codex::ephemeral_red_team::EphemeralRedTeamTool)),
            "fractal_swarm" => Some(Arc::new(crate::codex::fractal_swarm::FractalSwarmTool)),
            "node_repl" => Some(Arc::new(crate::codex::node_repl::NodeReplTool)),
            "automation" => Some(Arc::new(crate::codex::simple_tools::CodexAutomationTool)),
            "current_time" => Some(Arc::new(crate::codex::simple_tools::CurrentTimeTool)),
            "weather_lookup" => Some(Arc::new(crate::codex::simple_tools::WeatherLookupTool)),
            "finance_lookup" => Some(Arc::new(crate::codex::simple_tools::FinanceLookupTool)),
            "sports_lookup" => Some(Arc::new(crate::codex::simple_tools::SportsLookupTool)),
            "codex_tool_catalog" => Some(Arc::new(crate::codex::simple_tools::CodexCatalogTool)),
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