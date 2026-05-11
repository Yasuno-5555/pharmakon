use crate::agent::Agent;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Initialize all available tools on an Agent.
/// Shared between CLI and Gateway so both have access to the full tool suite.
pub async fn init_all_agent_tools(agent: &Agent) -> anyhow::Result<()> {
    use pharmakon_tools::terminal::{ShellTool, TerminalTool, BackgroundRunTool, ProcessStatusTool};
    use pharmakon_tools::code::{ViewFileTool, ListDirTool, StrictReplaceContentTool, GrepSearchTool, FindDefinitionTool, PythonInterpreterTool};
    use pharmakon_tools::files::{ApplyPatchTool, FileReadTool, FileWriteTool};
    use pharmakon_tools::repomap::RepoMapTool;
    use pharmakon_tools::git::{GitAddTool, GitBranchTool, GitCommitTool, GitDiffTool, GitLogTool, GitStatusTool};
    use pharmakon_tools::browser::BrowserTool;
    use pharmakon_tools::web_fetch::WebFetchTool;
    use pharmakon_tools::web_search::{DuckDuckGoSearchTool, GoogleSearchTool, BraveSearchTool as WebSearchBraveSearchTool, SearchDispatcherTool};
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
    use pharmakon_tools::cognitive::{
        TemporalAwarenessTool, FailurePredictionTool, ProactiveSelfOptimizationTool, RegretMinimizationTool,
    };
    use pharmakon_tools::orchestration::EphemeralRedTeamTool;
    use pharmakon_tools::media::ImageGenTool;
    use pharmakon_tools::NativeGuiEmulatorTool;
    use pharmakon_tools::tool_discovery::DiscoverToolsTool;


    // --- Core System Tools ---
    agent.add_tool(Arc::new(DiscoverToolsTool::default())).await;
    agent.add_tool(Arc::new(LoadToolsTool {
        active_categories: agent.active_categories.clone(),
    })).await;
    agent.add_tool(Arc::new(UpdateContextTool)).await;

    // --- Shell & Terminal ---
    agent.add_tool(Arc::new(ShellTool)).await;
    agent.add_tool(Arc::new(TerminalTool::new())).await;
    agent.add_tool(Arc::new(BackgroundRunTool::new())).await;
    agent.add_tool(Arc::new(ProcessStatusTool::new())).await;

    // --- File & Code ---
    agent.add_tool(Arc::new(FileReadTool)).await;
    agent.add_tool(Arc::new(FileWriteTool)).await;
    agent.add_tool(Arc::new(ViewFileTool)).await;
    agent.add_tool(Arc::new(ListDirTool)).await;
    agent.add_tool(Arc::new(StrictReplaceContentTool)).await;
    agent.add_tool(Arc::new(GrepSearchTool)).await;
    agent.add_tool(Arc::new(FindDefinitionTool)).await;
    agent.add_tool(Arc::new(PythonInterpreterTool)).await;
    agent.add_tool(Arc::new(ApplyPatchTool)).await;

    // --- Git & Repo ---
    agent.add_tool(Arc::new(RepoMapTool::new())).await;
    agent.add_tool(Arc::new(GitStatusTool)).await;
    agent.add_tool(Arc::new(GitDiffTool)).await;
    agent.add_tool(Arc::new(GitAddTool)).await;
    agent.add_tool(Arc::new(GitCommitTool)).await;
    agent.add_tool(Arc::new(GitLogTool)).await;
    agent.add_tool(Arc::new(GitBranchTool)).await;

    // --- Browser & Web ---
    agent.add_tool(Arc::new(BrowserTool::new(None))).await;
    agent.add_tool(Arc::new(NativeGuiEmulatorTool::new())).await;
    agent.add_tool(Arc::new(WebFetchTool::new())).await;
    agent.add_tool(Arc::new(WebSearchBraveSearchTool::new("".to_string()))).await;
    agent.add_tool(Arc::new(GoogleSearchTool)).await;
    agent.add_tool(Arc::new(DuckDuckGoSearchTool::new())).await;
    agent.add_tool(Arc::new(SearchDispatcherTool::new())).await;
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

    // --- CodeAct (model-adaptive routing) ---
    let model_name = {
        let m = agent.model.lock().await;
        m.name().to_string()
    };
    agent.add_tool(Arc::new(crate::orchestration::codeact::CodeActTool::with_model_family(
        std::env::current_dir().unwrap_or_default(),
        &model_name,
    ))).await;

    // --- Swarm ---
    let swarm_manager = Arc::new(crate::orchestration::swarm::SwarmManager::new(
        Arc::new(Mutex::new(agent.clone())),
    ));
    agent.add_tool(Arc::new(crate::orchestration::swarm::SwarmTool::new(swarm_manager.clone(), 0))).await;
    agent.add_tool(Arc::new(crate::orchestration::swarm::FractalSwarmTool::new(swarm_manager, 0).with_economy(agent.economy.clone()))).await;

    // --- Unified Cognitive & Orchestration Suite ---
    agent.add_tool(Arc::new(TemporalAwarenessTool)).await;
    agent.add_tool(Arc::new(FailurePredictionTool)).await;
    agent.add_tool(Arc::new(ProactiveSelfOptimizationTool)).await;
    agent.add_tool(Arc::new(RegretMinimizationTool)).await;
    agent.add_tool(Arc::new(EphemeralRedTeamTool)).await;
    agent.add_tool(Arc::new(ImageGenTool::new())).await;

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

    // --- Cron / Automation ---
    let cron_mgr = Arc::new(crate::automation::cron::CronManager::new().await?);
    let agent_weak = Arc::downgrade(&Arc::new(Mutex::new(agent.clone())));
    agent.add_tool(Arc::new(crate::automation::cron_tool::CronTool::new(
        cron_mgr.clone(),
        agent_weak,
    ))).await;
    *agent.cron_manager.lock().unwrap() = Some(cron_mgr);

    log::info!("All agent tools initialized.");
    Ok(())
}
