use crate::agent::Agent;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Initialize all available tools on an Agent.
/// Shared between CLI and Gateway so both have access to the full tool suite.
pub async fn init_all_agent_tools(agent: &Agent) -> anyhow::Result<()> {
    use pharmakon_tools::terminal::{ShellTool, TerminalTool, BackgroundRunTool, ProcessStatusTool};
    use pharmakon_tools::files::{FileReadTool, FileWriteTool};
    use pharmakon_tools::code::{ViewFileTool, ListDirTool, CodeEditTool, MultiCodeEditTool, GrepSearchTool, FindDefinitionTool, PythonInterpreterTool, ApplyPatchTool};
    use pharmakon_tools::repomap::RepoMapTool;
    use pharmakon_tools::git::{GitStatusTool, GitDiffTool, GitCommitTool};
    use pharmakon_tools::browser::BrowserTool;
    use pharmakon_tools::web_fetch::WebFetchTool;
    use pharmakon_tools::web_search::{GoogleSearchTool, BraveSearchTool as WebSearchBraveSearchTool};
    use pharmakon_tools::memory_hydration::HydrateContextTool;
    use pharmakon_tools::playbook::PlaybookTool;
    use pharmakon_tools::project_management::TaskTrackerTool;
    use pharmakon_tools::workspace::WorkspacePerceptionTool;
    use pharmakon_tools::probe::EnvironmentProbeTool;
    use pharmakon_tools::link_understanding::LinkUnderstandingTool;
    use pharmakon_tools::quality::CargoQualityTool;
    use pharmakon_tools::tool_market::ToolMarketTool;
    use pharmakon_tools::checkpoint::CheckpointTool;
    use pharmakon_tools::reflection::ReflectionTool;
    use pharmakon_tools::orchestration::{ToolRouterTool, LoadToolsTool};
    use pharmakon_tools::memory_mgmt::MemoryManagementTool;
    use pharmakon_tools::context_mgmt::UpdateContextTool;
    use pharmakon_tools::codex::{
        DeterministicReplayTool, ContextBudgetOptimizerTool, DryRunTool, WorkspaceSnapshotTool,
        WebTaskTool, LocalModelRouterTool, SkillCompositionTool, FailureMemoryTool,
        ProactiveInterventionTool, CognitiveMirrorTool, IntentCompilerTool, RegretMinimizationTool,
        CounterfactualSimulatorTool, AttentionRouterTool, TemporalAwarenessTool, SoftDependencyGraphTool,
        AutonomyDialTool, FailurePredictionTool, AstLspBridgeTool, SpecFirstTestTool,
        SemanticConflictResolutionTool, TimeTravelDebuggerTool, NexusVisualizerTool,
        ProactiveSelfOptimizationTool, DiffSecurityAuditorTool, AstNativeMutationTool,
        MemoryActorStatusTool, GraphPrefetchTool,
        EphemeralRedTeamTool, NodeReplTool, CodexAutomationTool,
        CurrentTimeTool, WeatherLookupTool, FinanceLookupTool, SportsLookupTool, CodexCatalogTool,
    };

    let background_processes = Arc::new(Mutex::new(std::collections::HashMap::new()));

    // --- Core System Tools ---
    agent.add_tool(Arc::new(LoadToolsTool {
        active_categories: agent.active_categories.clone(),
    })).await;
    agent.add_tool(Arc::new(UpdateContextTool)).await;

    // --- Shell & Terminal ---
    agent.add_tool(Arc::new(ShellTool)).await;
    agent.add_tool(Arc::new(TerminalTool::new())).await;
    agent.add_tool(Arc::new(BackgroundRunTool {
        active_processes: background_processes.clone(),
    })).await;
    agent.add_tool(Arc::new(ProcessStatusTool {
        active_processes: background_processes,
        retry_counts: Arc::new(Mutex::new(std::collections::HashMap::new())),
    })).await;

    // --- File & Code ---
    agent.add_tool(Arc::new(FileReadTool)).await;
    agent.add_tool(Arc::new(FileWriteTool)).await;
    agent.add_tool(Arc::new(ViewFileTool)).await;
    agent.add_tool(Arc::new(ListDirTool)).await;
    agent.add_tool(Arc::new(CodeEditTool)).await;
    agent.add_tool(Arc::new(MultiCodeEditTool)).await;
    agent.add_tool(Arc::new(GrepSearchTool)).await;
    agent.add_tool(Arc::new(FindDefinitionTool)).await;
    agent.add_tool(Arc::new(PythonInterpreterTool)).await;
    agent.add_tool(Arc::new(ApplyPatchTool)).await;

    // --- Git & Repo ---
    agent.add_tool(Arc::new(RepoMapTool::new())).await;
    agent.add_tool(Arc::new(GitStatusTool)).await;
    agent.add_tool(Arc::new(GitDiffTool)).await;
    agent.add_tool(Arc::new(GitCommitTool)).await;

    // --- Browser & Web ---
    agent.add_tool(Arc::new(BrowserTool::new(None))).await;
    agent.add_tool(Arc::new(WebFetchTool::new())).await;
    agent.add_tool(Arc::new(WebSearchBraveSearchTool::new("".to_string()))).await;
    agent.add_tool(Arc::new(GoogleSearchTool)).await;
    agent.add_tool(Arc::new(pharmakon_tools::search::custom_scout::CustomScoutTool)).await;

    // --- Knowledge & Context ---
    agent.add_tool(Arc::new(HydrateContextTool::new())).await;
    agent.add_tool(Arc::new(PlaybookTool::new())).await;
    agent.add_tool(Arc::new(WorkspacePerceptionTool::new())).await;
    agent.add_tool(Arc::new(EnvironmentProbeTool::new())).await;
    agent.add_tool(Arc::new(LinkUnderstandingTool::new())).await;
    agent.add_tool(Arc::new(TaskTrackerTool::new())).await;

    // --- Quality & Tool Management ---
    agent.add_tool(Arc::new(CargoQualityTool)).await;
    agent.add_tool(Arc::new(ToolMarketTool)).await;

    // --- Trajectory & Intelligence ---
    let agent_weak = Arc::downgrade(&Arc::new(agent.clone()));
    agent.add_tool(Arc::new(crate::trajectory::tool::ExecutionTraceTool::new(agent_weak.clone()))).await;
    agent.add_tool(Arc::new(crate::trajectory::tool::ToolReliabilityTool::new(agent_weak.clone()))).await;
    agent.add_tool(Arc::new(crate::trajectory::tool::InsightTool::new(agent_weak.clone()))).await;
    agent.add_tool(Arc::new(crate::trajectory::tool::SemanticGrepTool::new(agent_weak.clone()))).await;
    agent.add_tool(Arc::new(crate::orchestration::mcts::MctsSimulatorTool::new(agent_weak.clone()))).await;
    agent.add_tool(Arc::new(crate::orchestration::rlfc::RlfcTool::new(agent_weak.clone()))).await;

    // --- CodeAct ---
    agent.add_tool(Arc::new(crate::orchestration::codeact::CodeActTool::new(
        std::env::current_dir().unwrap_or_default(),
    ))).await;

    // --- Swarm ---
    let swarm_manager = Arc::new(crate::orchestration::swarm::SwarmManager::new(
        Arc::new(Mutex::new(agent.clone())),
    ));
    agent.add_tool(Arc::new(crate::orchestration::swarm::SwarmTool::new(swarm_manager.clone(), 0))).await;
    agent.add_tool(Arc::new(crate::orchestration::swarm::FractalSwarmTool::new(swarm_manager, 0).with_economy(agent.economy.clone()))).await;

    // --- Codex Suite ---
    agent.add_tool(Arc::new(DeterministicReplayTool)).await;
    agent.add_tool(Arc::new(ContextBudgetOptimizerTool)).await;
    agent.add_tool(Arc::new(DryRunTool)).await;
    agent.add_tool(Arc::new(WorkspaceSnapshotTool)).await;
    agent.add_tool(Arc::new(WebTaskTool)).await;
    agent.add_tool(Arc::new(LocalModelRouterTool)).await;
    agent.add_tool(Arc::new(SkillCompositionTool)).await;
    agent.add_tool(Arc::new(FailureMemoryTool)).await;
    agent.add_tool(Arc::new(ProactiveInterventionTool)).await;
    agent.add_tool(Arc::new(CognitiveMirrorTool)).await;
    agent.add_tool(Arc::new(IntentCompilerTool)).await;
    agent.add_tool(Arc::new(RegretMinimizationTool)).await;
    agent.add_tool(Arc::new(CounterfactualSimulatorTool)).await;
    agent.add_tool(Arc::new(AttentionRouterTool)).await;
    agent.add_tool(Arc::new(TemporalAwarenessTool)).await;
    agent.add_tool(Arc::new(SoftDependencyGraphTool)).await;
    agent.add_tool(Arc::new(AutonomyDialTool)).await;
    agent.add_tool(Arc::new(FailurePredictionTool)).await;
    agent.add_tool(Arc::new(AstLspBridgeTool)).await;
    agent.add_tool(Arc::new(SpecFirstTestTool)).await;
    agent.add_tool(Arc::new(SemanticConflictResolutionTool)).await;
    agent.add_tool(Arc::new(TimeTravelDebuggerTool)).await;
    agent.add_tool(Arc::new(NexusVisualizerTool)).await;
    agent.add_tool(Arc::new(ProactiveSelfOptimizationTool)).await;
    agent.add_tool(Arc::new(DiffSecurityAuditorTool)).await;
    agent.add_tool(Arc::new(AstNativeMutationTool)).await;
    agent.add_tool(Arc::new(MemoryActorStatusTool)).await;
    agent.add_tool(Arc::new(GraphPrefetchTool)).await;
    agent.add_tool(Arc::new(EphemeralRedTeamTool)).await;
    agent.add_tool(Arc::new(NodeReplTool)).await;
    agent.add_tool(Arc::new(CodexAutomationTool)).await;
    agent.add_tool(Arc::new(CurrentTimeTool)).await;
    agent.add_tool(Arc::new(WeatherLookupTool)).await;
    agent.add_tool(Arc::new(FinanceLookupTool)).await;
    agent.add_tool(Arc::new(SportsLookupTool)).await;
    agent.add_tool(Arc::new(CodexCatalogTool)).await;

    // --- MCP ---
    agent.add_tool(Arc::new(crate::mcp_tool::ConnectMcpServerTool {
        registry: agent.registry.clone(),
    })).await;

    // --- Phase 3 / Reflection ---
    agent.add_tool(Arc::new(CheckpointTool)).await;
    agent.add_tool(Arc::new(ReflectionTool)).await;
    agent.add_tool(Arc::new(ToolRouterTool)).await;

    // --- Memory Management (if nexus is available) ---
    if agent.knowledge_nexus.is_some() {
        agent.add_tool(Arc::new(MemoryManagementTool)).await;
    }

    log::info!("All agent tools initialized.");
    Ok(())
}
