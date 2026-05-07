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
    pub vision_stream:
        Option<Arc<tokio::sync::Mutex<crate::media::vision_stream::VisionRingBuffer>>>,
    pub total_tokens: Option<Arc<std::sync::atomic::AtomicU64>>,
    pub total_cost: Option<Arc<tokio::sync::Mutex<f64>>>,
}

impl ToolRegistry {
    pub fn get_tool(name: &str, deps: &ToolDependencies) -> Option<Arc<dyn Tool>> {
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
            "execution_trace" => Some(Arc::new(crate::codex::ExecutionTraceTool)),
            "deterministic_replay" => Some(Arc::new(crate::codex::DeterministicReplayTool)),
            "tool_reliability" => Some(Arc::new(crate::codex::ToolReliabilityScoringTool)),
            "context_budget_optimizer" => Some(Arc::new(crate::codex::ContextBudgetOptimizerTool)),
            "dry_run" => Some(Arc::new(crate::codex::DryRunTool)),
            "workspace_snapshot" => Some(Arc::new(crate::codex::WorkspaceSnapshotTool)),
            "semantic_grep" => Some(Arc::new(crate::codex::SemanticGrepTool)),
            "web_task" => Some(Arc::new(crate::codex::WebTaskTool)),
            "local_model_router" => Some(Arc::new(crate::codex::LocalModelRouterTool)),
            "skill_composition" => Some(Arc::new(crate::codex::SkillCompositionTool)),
            "failure_memory" => Some(Arc::new(crate::codex::FailureMemoryTool)),
            "proactive_intervention" => Some(Arc::new(crate::codex::ProactiveInterventionTool)),
            "cognitive_mirror" => Some(Arc::new(crate::codex::CognitiveMirrorTool)),
            "intent_compiler" => Some(Arc::new(crate::codex::IntentCompilerTool)),
            "regret_minimization" => Some(Arc::new(crate::codex::RegretMinimizationTool)),
            "counterfactual_simulator" => Some(Arc::new(crate::codex::CounterfactualSimulatorTool)),
            "attention_router" => Some(Arc::new(crate::codex::AttentionRouterTool)),
            "temporal_awareness" => Some(Arc::new(crate::codex::TemporalAwarenessTool)),
            "soft_dependency_graph" => Some(Arc::new(crate::codex::SoftDependencyGraphTool)),
            "autonomy_dial" => Some(Arc::new(crate::codex::AutonomyDialTool)),
            "failure_prediction" => Some(Arc::new(crate::codex::FailurePredictionTool)),
            "ast_lsp_bridge" => Some(Arc::new(crate::codex::AstLspBridgeTool)),
            "spec_first_test" => Some(Arc::new(crate::codex::SpecFirstTestTool)),
            "semantic_conflict_resolution" => {
                Some(Arc::new(crate::codex::SemanticConflictResolutionTool))
            }
            "time_travel_debugger" => Some(Arc::new(crate::codex::TimeTravelDebuggerTool)),
            "nexus_visualizer" => Some(Arc::new(crate::codex::NexusVisualizerTool)),
            "proactive_self_optimization" => {
                Some(Arc::new(crate::codex::ProactiveSelfOptimizationTool))
            }
            "diff_security_auditor" => Some(Arc::new(crate::codex::DiffSecurityAuditorTool)),
            "mutate_ast" => Some(Arc::new(crate::codex::AstNativeMutationTool)),
            "mcts_simulator" => Some(Arc::new(crate::codex::MctsSimulatorTool)),
            "memory_actor_status" => Some(Arc::new(crate::codex::MemoryActorStatusTool)),
            "graph_prefetch" => Some(Arc::new(crate::codex::GraphPrefetchTool)),
            "rlfc" => Some(Arc::new(crate::codex::RlfcTool)),
            "ephemeral_red_team" => Some(Arc::new(crate::codex::EphemeralRedTeamTool)),
            "fractal_swarm" => Some(Arc::new(crate::codex::FractalSwarmTool)),
            "node_repl" => Some(Arc::new(crate::codex::NodeReplTool)),
            "automation" => Some(Arc::new(crate::codex::CodexAutomationTool)),
            "current_time" => Some(Arc::new(crate::codex::CurrentTimeTool)),
            "weather_lookup" => Some(Arc::new(crate::codex::WeatherLookupTool)),
            "finance_lookup" => Some(Arc::new(crate::codex::FinanceLookupTool)),
            "sports_lookup" => Some(Arc::new(crate::codex::SportsLookupTool)),
            "codex_tool_catalog" => Some(Arc::new(crate::codex::CodexCatalogTool)),
            _ => None,
        }
    }
}
