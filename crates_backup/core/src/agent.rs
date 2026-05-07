use crate::model::{
    AgentError, AgentErrorCode, AgentModel, CompletionRequest,
    Message, MessageContent, ToolDefinition,
};
use crate::system_prompt::SystemPromptManager;
use anyhow::{Result, anyhow};
use pharmakon_common::{Event, ToolRegistry};
use pharmakon_memory::BeliefSystem;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use tokio::sync::{Mutex, broadcast};

tokio::task_local! {
    pub static CURRENT_SESSION_ID: String;
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkingMemoryUnit {
    pub content: String,
    pub summary: Option<String>, // Micro-summary (1-2 lines)
    pub importance: f32,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub tokens: usize,  // Estimated tokens
    pub source: String, // Where it came from
}

pub struct SessionState {
    pub history: Vec<Message>,
    pub working_memory: Vec<WorkingMemoryUnit>,
    pub active_playbooks: Vec<(String, String)>, // (name, content)
    pub context_engine: Arc<Mutex<crate::memory::context_engine::ContextEngine>>,
}

pub struct Agent {
    pub model: Arc<Mutex<Arc<dyn AgentModel>>>,
    pub session_id: Arc<Mutex<String>>, // Current global session ID (legacy/default)
    pub session_states: Arc<Mutex<std::collections::HashMap<String, Arc<Mutex<SessionState>>>>>,
    pub prompt_manager: Arc<Mutex<SystemPromptManager>>,
    pub event_tx: broadcast::Sender<Event>,
    pub approval_tx: broadcast::Sender<(String, bool)>,
    pub trajectory: Arc<Mutex<crate::trajectory::Trajectory>>,
    pub compactor: Arc<Mutex<crate::memory::compactor::ContextCompactor>>,
    pub tools: Arc<Mutex<Vec<Arc<dyn pharmakon_common::Tool>>>>,
    pub hooks: Arc<crate::hooks::HookRegistry>,
    pub fact_memory: Option<Arc<Mutex<crate::memory::BeliefSystem>>>,
    pub semantic_search: Option<Arc<pharmakon_memory::semantic_search::SemanticSearch>>,
    pub knowledge_nexus: Option<Arc<pharmakon_memory::weaver::KnowledgeNexus>>,
    pub health_monitor: crate::orchestration::health_monitor::HealthMonitor,
    pub policy_engine: Arc<crate::security::policy::PolicyEngine>,
    pub session_store: Option<Arc<crate::persistence::DbSessionStore>>,
    pub planner_model: Option<Arc<Mutex<Arc<dyn AgentModel>>>>,
    pub vision_stream: Option<Arc<Mutex<pharmakon_tools::media::vision_stream::VisionRingBuffer>>>,
    pub graph_store: Option<Arc<crate::memory::graph::GraphStore>>,
    pub interaction_count: Arc<std::sync::atomic::AtomicU32>,
    pub fallback_models: Arc<StdMutex<Vec<String>>>,
    pub total_tokens: Arc<std::sync::atomic::AtomicU64>,
    pub total_cost: Arc<Mutex<f64>>,
    pub start_time: std::time::Instant,
    pub tool_call_counts: Arc<Mutex<std::collections::HashMap<String, u32>>>,
    pub territory_manager: Arc<crate::orchestration::territory::TerritoryManager>,
    pub research_notebook: Arc<Mutex<crate::orchestration::research::ResearchNotebook>>,
    pub usage_history: Arc<Mutex<Vec<(chrono::DateTime<chrono::Utc>, u64, f64)>>>,
}

impl Clone for Agent {
    fn clone(&self) -> Self {
        Self {
            model: self.model.clone(),
            session_id: self.session_id.clone(),
            session_states: self.session_states.clone(),
            prompt_manager: self.prompt_manager.clone(),
            event_tx: self.event_tx.clone(),
            approval_tx: self.approval_tx.clone(),
            trajectory: self.trajectory.clone(),
            compactor: self.compactor.clone(),
            tools: self.tools.clone(),
            hooks: self.hooks.clone(),
            fact_memory: self.fact_memory.clone(),
            semantic_search: self.semantic_search.clone(),
            knowledge_nexus: self.knowledge_nexus.clone(),
            health_monitor: self.health_monitor.clone(),
            policy_engine: self.policy_engine.clone(),
            session_store: self.session_store.clone(),
            planner_model: self.planner_model.clone(),
            vision_stream: self.vision_stream.clone(),
            graph_store: self.graph_store.clone(),
            interaction_count: self.interaction_count.clone(),
            fallback_models: self.fallback_models.clone(),
            total_tokens: self.total_tokens.clone(),
            total_cost: self.total_cost.clone(),
            start_time: self.start_time,
            tool_call_counts: self.tool_call_counts.clone(),
            territory_manager: self.territory_manager.clone(),
            research_notebook: self.research_notebook.clone(),
            usage_history: self.usage_history.clone(),
        }
    }
}

impl Agent {
    pub fn new(model: Arc<dyn AgentModel>, session_id: String) -> Self {
        let (event_tx, _) = broadcast::channel(100);
        let (approval_tx, _) = broadcast::channel(100);
        let trajectory = Arc::new(Mutex::new(crate::trajectory::Trajectory::new(
            session_id.clone(),
            model.name().to_string(),
        )));
        let compactor = Arc::new(Mutex::new(crate::memory::compactor::ContextCompactor::new(
            model.clone(),
        )));
        let mut pm = SystemPromptManager::new(crate::soul::Soul::default_soul());
        pm.add_contribution(Box::new(
            crate::system_prompt::autonomy::AutonomyContribution,
        ));

        let playbook_names = pharmakon_tools::playbook::PlaybookTool::list_names();
        pm.add_contribution(Box::new(crate::system_prompt::PlaybookContribution {
            names: playbook_names,
        }));

        let mut hooks = crate::hooks::HookRegistry::new();
        hooks.register(Box::new(
            crate::hooks::token_economy::TokenEconomyHook::new(0.8, 100_000),
        )); // 100k token default budget

        Self {
            model: Arc::new(Mutex::new(model.clone())),
            session_id: Arc::new(Mutex::new(session_id)),
            session_states: Arc::new(Mutex::new(std::collections::HashMap::new())),
            prompt_manager: Arc::new(Mutex::new(pm)),
            event_tx,
            approval_tx,
            trajectory,
            compactor,
            tools: Arc::new(Mutex::new(Vec::new())),
            hooks: Arc::new(hooks),
            fact_memory: None,
            semantic_search: None,
            knowledge_nexus: None,
            health_monitor: crate::orchestration::health_monitor::HealthMonitor::new(0.3),
            policy_engine: Arc::new(crate::security::policy::PolicyEngine::new()),
            session_store: None,
            planner_model: None,
            vision_stream: None,
            graph_store: None,
            interaction_count: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            fallback_models: Arc::new(StdMutex::new(Vec::new())),
            total_tokens: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            total_cost: Arc::new(Mutex::new(0.0)),
            start_time: std::time::Instant::now(),
            tool_call_counts: Arc::new(Mutex::new(std::collections::HashMap::new())),
            territory_manager: Arc::new(crate::orchestration::territory::TerritoryManager::new()),
            research_notebook: Arc::new(Mutex::new(
                crate::orchestration::research::ResearchNotebook::new("Uninitialized"),
            )),
            usage_history: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn with_fallback_models(self, models: Vec<String>) -> Self {
        {
            let mut fm = self.fallback_models.lock().unwrap();
            *fm = models;
        }
        self
    }

    pub async fn get_current_session_state(&self) -> Arc<Mutex<SessionState>> {
        let session_id = {
            let sid = self.session_id.lock().await;
            sid.clone()
        };
        // Check if task-local session ID is available, override if so
        let session_id = CURRENT_SESSION_ID.with(|id| {
            if id.is_empty() {
                session_id
            } else {
                id.clone()
            }
        });
        self.get_session_state(&session_id).await
    }

    pub async fn get_session_state(&self, session_id: &str) -> Arc<Mutex<SessionState>> {
        let mut states = self.session_states.lock().await;
        if let Some(state) = states.get(session_id) {
            return state.clone();
        }

        let mut history = Vec::new();
        if let Some(store) = &self.session_store
            && let Ok(h) = store.load_history(session_id).await {
                history = h;
            }

        let state = Arc::new(Mutex::new(SessionState {
            history,
            working_memory: Vec::new(),
            active_playbooks: Vec::new(),
            context_engine: Arc::new(Mutex::new(
                crate::memory::context_engine::ContextEngine::new(8192),
            )),
        }));
        states.insert(session_id.to_string(), state.clone());
        state
    }

    pub fn with_store(mut self, store: Arc<crate::persistence::DbSessionStore>) -> Self {
        self.session_store = Some(store);
        self
    }

    pub fn with_fact_memory(mut self, fact_memory: Arc<Mutex<BeliefSystem>>) -> Self {
        self.fact_memory = Some(fact_memory);
        self
    }

    pub fn with_semantic_search(
        mut self,
        search: Arc<pharmakon_memory::semantic_search::SemanticSearch>,
    ) -> Self {
        self.semantic_search = Some(search);
        self
    }

    pub fn with_knowledge_nexus(
        mut self,
        nexus: Arc<pharmakon_memory::weaver::KnowledgeNexus>,
    ) -> Self {
        self.knowledge_nexus = Some(nexus);
        self
    }

    pub async fn add_tool(&self, tool: Arc<dyn pharmakon_common::Tool>) {
        let mut tools = self.tools.lock().await;
        let name = tool.name().to_string();
        if tools.iter().any(|existing| existing.name() == name) {
            log::debug!("Skipping duplicate tool registration: {}", name);
            return;
        }
        tools.push(tool);
    }

    pub async fn init_standard_tools(&self) {
        let background_processes = Arc::new(Mutex::new(std::collections::HashMap::new()));
        self.add_tool(Arc::new(pharmakon_tools::terminal::ShellTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::terminal::TerminalTool::new()))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::terminal::BackgroundRunTool {
            active_processes: background_processes.clone(),
        }))
        .await;
        self.add_tool(Arc::new(pharmakon_tools::terminal::ProcessStatusTool {
            active_processes: background_processes,
            retry_counts: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }))
        .await;
        self.add_tool(Arc::new(pharmakon_tools::files::FileReadTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::files::FileWriteTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::code::ViewFileTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::code::ListDirTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::code::CodeEditTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::code::MultiCodeEditTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::code::GrepSearchTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::code::FindDefinitionTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::code::PythonInterpreterTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::repomap::RepoMapTool::new()))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::git::GitStatusTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::git::GitDiffTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::git::GitCommitTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::browser::BrowserTool::new(None)))
            .await;

        if let Ok(key) = std::env::var("BRAVE_SEARCH_API_KEY") {
            self.add_tool(Arc::new(pharmakon_tools::search::BraveSearchTool::new(key)))
                .await;
        }

        self.add_tool(Arc::new(pharmakon_tools::web_fetch::WebFetchTool::new()))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::web_search::BraveSearchTool::new(
            "".to_string(),
        )))
        .await;
        self.add_tool(Arc::new(pharmakon_tools::web_search::GoogleSearchTool))
            .await;
        self.add_tool(Arc::new(
            pharmakon_tools::search::custom_scout::CustomScoutTool,
        ))
        .await;
        self.add_tool(Arc::new(
            pharmakon_tools::memory_hydration::HydrateContextTool::new(),
        ))
        .await;
        self.add_tool(Arc::new(pharmakon_tools::playbook::PlaybookTool::new()))
            .await;
        self.add_tool(Arc::new(
            pharmakon_tools::project_management::TaskTrackerTool::new(),
        ))
        .await;
        self.add_tool(Arc::new(
            pharmakon_tools::workspace::WorkspacePerceptionTool::new(),
        ))
        .await;
        self.add_tool(Arc::new(pharmakon_tools::probe::EnvironmentProbeTool::new()))
            .await;
        self.add_tool(Arc::new(
            pharmakon_tools::link_understanding::LinkUnderstandingTool::new(),
        ))
        .await;
        self.add_tool(Arc::new(pharmakon_tools::quality::CargoQualityTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::tool_market::ToolMarketTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::ExecutionTraceTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::DeterministicReplayTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::ToolReliabilityScoringTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::ContextBudgetOptimizerTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::DryRunTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::WorkspaceSnapshotTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::SemanticGrepTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::WebTaskTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::LocalModelRouterTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::SkillCompositionTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::FailureMemoryTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::ProactiveInterventionTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::CognitiveMirrorTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::IntentCompilerTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::RegretMinimizationTool))
            .await;
        self.add_tool(Arc::new(
            pharmakon_tools::codex::CounterfactualSimulatorTool,
        ))
        .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::AttentionRouterTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::TemporalAwarenessTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::SoftDependencyGraphTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::AutonomyDialTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::FailurePredictionTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::AstLspBridgeTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::SpecFirstTestTool))
            .await;
        self.add_tool(Arc::new(
            pharmakon_tools::codex::SemanticConflictResolutionTool,
        ))
        .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::TimeTravelDebuggerTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::NexusVisualizerTool))
            .await;
        self.add_tool(Arc::new(
            pharmakon_tools::codex::ProactiveSelfOptimizationTool,
        ))
        .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::DiffSecurityAuditorTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::AstNativeMutationTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::MctsSimulatorTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::MemoryActorStatusTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::GraphPrefetchTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::RlfcTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::EphemeralRedTeamTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::FractalSwarmTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::NodeReplTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::CodexAutomationTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::CurrentTimeTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::WeatherLookupTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::FinanceLookupTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::SportsLookupTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::codex::CodexCatalogTool))
            .await;
    }

    pub async fn setup_autonomous_tools(&self) {
        self.init_standard_tools().await;

        // Add Phase 3 Tools
        self.add_tool(Arc::new(pharmakon_tools::checkpoint::CheckpointTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::reflection::ReflectionTool))
            .await;
        self.add_tool(Arc::new(pharmakon_tools::orchestration::ToolRouterTool))
            .await;

        if let Some(nexus) = &self.knowledge_nexus {
            self.add_tool(Arc::new(
                pharmakon_tools::memory_mgmt::MemoryManagementTool::new(Some(nexus.clone())),
            ))
            .await;
        }

        // Add apply_patch (SAFER replacement for write_file)
        self.add_tool(Arc::new(pharmakon_tools::code::ApplyPatchTool))
            .await;
    }

    pub async fn chat(&self, message: &str) -> Result<String> {
        let session_id = {
            let sid = self.session_id.lock().await;
            sid.clone()
        };
        self.chat_on_session(message, &session_id).await
    }

    pub async fn set_session_id(&self, id: String) {
        let mut sid = self.session_id.lock().await;
        *sid = id;
    }

    pub async fn replace_history(&self, history: Vec<Message>) -> Result<()> {
        let state_arc = self.get_current_session_state().await;
        let mut state = state_arc.lock().await;
        state.history = history;
        Ok(())
    }

    pub async fn update_model(&self, model: Arc<dyn AgentModel>) -> Result<()> {
        let mut m = self.model.lock().await;
        *m = model;
        Ok(())
    }

    pub async fn clear_history(&self) -> Result<()> {
        // 1. Reset context engine
        {
            let state_arc = self.get_current_session_state().await;
            let state = state_arc.lock().await;
            let mut context_engine = state.context_engine.lock().await;
            context_engine.clear_history();
        }

        // 2. Clear history
        {
            let state_arc = self.get_current_session_state().await;
            let mut state = state_arc.lock().await;
            state.history.clear();
        }

        // 3. Reset trajectory
        {
            let session_id = {
                let sid = self.session_id.lock().await;
                sid.clone()
            };
            let mut trajectory = self.trajectory.lock().await;
            let model_name = {
                let m = self.model.lock().await;
                m.name().to_string()
            };
            *trajectory = crate::trajectory::Trajectory::new(session_id, model_name);
        }

        Ok(())
    }

    pub async fn chat_on_session(&self, user_message: &str, session_id: &str) -> Result<String> {
        CURRENT_SESSION_ID.scope(session_id.to_string(), async {
            if user_message.starts_with("/model") {
                return self.handle_model_command(user_message).await;
            }

            let state_arc = self.get_session_state(session_id).await;

            let user_msg = Message {
                role: "user".to_string(),
                content: Some(MessageContent::Text(user_message.to_string())),
                ..Default::default()
            };

            let _ = self.hooks.trigger_message_received(&user_msg).await;

            if let Some(store) = &self.session_store {
                store.save_message(session_id, &user_msg).await?;
            }

            {
                let mut state = state_arc.lock().await;
                state.history.push(user_msg);

                let mut history = state.history.clone();
                {
                    let context_engine = state.context_engine.lock().await;
                    let _ = context_engine.prune_history(&mut history).await;
                }
                state.history = history;

                if state.history.len() > 20 {
                    let compactor = self.compactor.lock().await;
                    if let Ok(compacted) = compactor.compact(state.history.clone()).await {
                        state.history = compacted;
                    }
                }
            }

        let _ = self.event_tx.send(Event::AgentThought {
            content: MessageContent::Text("Thinking...".to_string()),
        });

        // Parallel context gathering
        let semantic_search = self.semantic_search.clone();
        let knowledge_nexus = self.knowledge_nexus.clone();
        let user_msg_text = user_message.to_string();

        // Parallel context gathering for performance
        let (semantic_res, nexus_res) = tokio::join!(
            async {
                if let Some(search) = semantic_search {
                    search.search_with_limit(&user_msg_text, 3).await.ok()
                } else {
                    None
                }
            },
            async {
                if let Some(nexus) = knowledge_nexus {
                    nexus.smart_search(&user_msg_text, 8).await.ok()
                } else {
                    None
                }
            }
        );

        if let Some(memories) = semantic_res
            && !memories.is_empty() {
                let memory_context = memories.join("\\n---\\n");
                self.add_to_working_memory(
                    format!("Long-term Memories:\\n{}", memory_context),
                    0.7,
                    "SemanticSearch".to_string(),
                )
                .await;
            }

        if let Some(memories) = nexus_res
            && !memories.is_empty() {
                let memory_context = memories.join("\\n---\\n");
                let _ = self.hooks.trigger_context_recovered(&memory_context).await;
                self.add_to_working_memory(
                    format!(
                        "Knowledge Nexus Insights (Hybrid + Graph):\\n{}",
                        memory_context
                    ),
                    0.9,
                    "KnowledgeNexus".to_string(),
                )
                .await;
            }



        let tools_count = self.tools.lock().await.len();
        log::info!(
            "Agent entering decision loop with {} tools and session: {}",
            tools_count,
            session_id
        );

        let mut iteration_count = 0;
        let max_iterations = 15;
        let start_time = std::time::Instant::now();
        let max_duration = std::time::Duration::from_secs(300); // 5 minutes

        loop {
            iteration_count += 1;
            if iteration_count > max_iterations {
                let reason = format!(
                    "Loop limit exceeded ({} iterations). Potential infinite loop detected.",
                    iteration_count
                );
                log::error!("CRITICAL: {}", reason);
                let _ = self.event_tx.send(Event::AgentHangDetected {
                    reason: reason.clone(),
                });
                return Err(anyhow::anyhow!(AgentError::new(
                    AgentErrorCode::HangDetected,
                    reason
                )));
            }

            if start_time.elapsed() > max_duration {
                let reason = format!(
                    "Time limit exceeded ({:?}). Agent is taking too long to respond.",
                    max_duration
                );
                log::error!("CRITICAL: {}", reason);
                let _ = self.event_tx.send(Event::AgentHangDetected {
                    reason: reason.clone(),
                });
                return Err(anyhow::anyhow!(AgentError::new(
                    AgentErrorCode::HangDetected,
                    reason
                )));
            }

            log::info!("[SESSION: {}] Agent iteration start ({})...", session_id, iteration_count);

            // 1. HIERARCHICAL REASONING: Step 1 - Define Strategy (Only on first iteration)
            if iteration_count == 1 {
                 let _ = self.event_tx.send(Event::AgentThought {
                    content: MessageContent::Text("Analyzing task complexity and planning resource retrieval...".to_string()),
                 });
                 // We could use a lighter model here to decide which tools to hydrate first.
            }

            let mut messages_to_send = Vec::new();
            {
                let prompt_manager = self.prompt_manager.lock().await;

                // 1. VIRTUAL CONTEXT SCALING:
                // Instead of loading all context, we load a sparse index.
                let state = state_arc.lock().await;
                let virtual_index = {
                    let mut entries = Vec::new();
                    for (i, unit) in state.working_memory.iter().enumerate() {
                        entries.push(pharmakon_memory::context_engine::ContextEntry {
                            id: format!("wm-{}", i),
                            summary: unit.summary.clone().unwrap_or_else(|| {
                                if unit.content.len() > 100 {
                                    format!("{}...", &unit.content[..100])
                                } else {
                                    unit.content.clone()
                                }
                            }),
                            relevance: unit.importance,
                            category: unit.source.clone(),
                        });
                    }
                    let engine = state.context_engine.lock().await;
                    engine.generate_virtual_index(&entries)
                };

                let layout = crate::system_prompt::PromptLayout {
                    system_rules: prompt_manager.soul().system_prompt.clone(),
                    playbooks: {
                        if state.active_playbooks.is_empty() {
                            "No specialized playbooks active.".to_string()
                        } else {
                            state.active_playbooks
                                .iter()
                                .map(|(name, content)| format!("#### ACTIVE PLAYBOOK: {}\n{}", name, content))
                                .collect::<Vec<_>>()
                                .join("\n\n---\n\n")
                        }
                    },
                    repo_map: None, // Will be populated by repomap tool if needed
                    knowledge_graph: None,
                    working_memory: virtual_index,
                    current_task: user_message.to_string(),
                };

                messages_to_send.push(Message {
                    role: "system".to_string(),
                    content: Some(MessageContent::Text(layout.render())),
                    ..Default::default()
                });

                messages_to_send.extend(state.history.clone());
            }

            let tool_definitions = {
                let tools_lock = self.tools.lock().await;
                if tools_lock.is_empty() {
                    None
                } else {
                    Some(
                        tools_lock
                            .iter()
                            .map(|t| ToolDefinition {
                                r#type: "function".to_string(),
                                function: crate::model::FunctionDefinition {
                                    name: t.name().to_string(),
                                    description: t.description().to_string(),
                                    parameters: t.parameters(),
                                },
                            })
                            .collect(),
                    )
                }
            };

            // Tiered Reasoning: Use planner model for tool selection if available
            let mut target_model = {
                let m = self.model.lock().await;
                (*m).clone()
            };

            log::info!("[SESSION: {}] Sending completion request to model...", session_id);

            let request = CompletionRequest {
                messages: messages_to_send,
                temperature: Some(0.2),
                max_tokens: Some(4096),
                tools: tool_definitions,
            };

            let mut response_result = None;
            let mut current_fallback_index = 0;
            let fallback_models = self.fallback_models.clone();

            while response_result.is_none() {
                let model_lock = target_model.clone();
                let completion_task = async {
                    model_lock.complete(request.clone()).await
                };

                response_result = Some(completion_task.await);

                match response_result {
                    Some(Ok(_)) => break, // Success, exit retry loop
                    Some(Err(ref e)) => {
                        let is_rate_limit = e.to_string().to_lowercase().contains("429")
                            || e.to_string().to_lowercase().contains("too many requests")
                            || e.to_string().to_lowercase().contains("quota");

                        let fallback_list = fallback_models.lock().unwrap();
                        if is_rate_limit && current_fallback_index < fallback_list.len() {
                            let fallback_id = &fallback_list[current_fallback_index];
                            log::warn!(
                                "Rate limit encountered for {}. Falling back to: {}",
                                target_model.name(),
                                fallback_id
                            );
                            let _ = self.event_tx.send(Event::Error {
                                message: format!(
                                    "API Rate limit reached for {}. Switching to fallback: {}",
                                    target_model.name(),
                                    fallback_id
                                ),
                            });

                            if let Some(new_model) =
                                crate::providers::registry::ModelRegistry::get_model(fallback_id)
                            {
                                target_model = new_model;
                                current_fallback_index += 1;
                                response_result = None;
                                continue;
                            } else {
                                log::error!(
                                    "Fallback model {} not found or configured.",
                                    fallback_id
                                );
                                current_fallback_index += 1;
                                response_result = None;
                                continue;
                            }
                        }

                        let _ = self.event_tx.send(Event::Error {
                            message: format!("Model error: {}", e),
                        });
                        return Err(response_result.unwrap().err().unwrap().into());
                    }
                    None => unreachable!(),
                }
            }

            let response: pharmakon_common::agent_types::CompletionResponse =
                response_result.unwrap().unwrap();

            log::debug!(
                "[SESSION: {}] Model response received. Content: {}, Tool calls: {}",
                session_id,
                response.content.is_some(),
                response.tool_calls.as_ref().map(|t| t.len()).unwrap_or(0)
            );

            if response.content.is_none() && response.tool_calls.is_none() {
                log::warn!(
                    "Model returned an empty response (no text, no tools). This might indicate a safety filter or internal model issue."
                );
            }

            let _ = self.event_tx.send(Event::InteractionFinished {
                response: response.clone(),
            });

            // Handle tool calls in parallel if multiple
            if let Some(tool_calls) = &response.tool_calls {
                log::info!("[SESSION: {}] Handling {} tool a-call(s)...", session_id, tool_calls.len());
                let mut tool_tasks = Vec::new();
                for tool_call in tool_calls {
                    let tool_call = tool_call.clone();
                    let _ = self.record_step(crate::trajectory::TrajectoryStep::Action {
                        tool: tool_call.function.name.clone(),
                        args: serde_json::from_str(&tool_call.function.arguments).unwrap_or_default(),
                        intent_id: None,
                        timestamp: chrono::Utc::now(),
                    }).await;
                    let tool = self
                        .tools
                        .lock()
                        .await
                        .iter()
                        .find(|t| t.name() == tool_call.function.name)
                        .cloned();
                    let event_tx = self.event_tx.clone();
                    let mut approval_rx = self.approval_tx.subscribe();
                    let hooks = self.hooks.clone();
                    let soul = {
                        let pm = self.prompt_manager.lock().await;
                        pm.soul().clone()
                    };
                    let policy_engine = self.policy_engine.clone();
                    let tool_call_counts = self.tool_call_counts.clone();
                    let forensic_id = uuid::Uuid::new_v4().to_string();

                    tool_tasks.push(tokio::spawn(async move {
                        let tool_name_from_call = tool_call.function.name.clone();
                        let tool = match tool {
                            Some(t) => t,
                            None => {
                                return (
                                    tool_call.id.clone(),
                                    Err(anyhow!("Tool not found: {}", tool_call.function.name)),
                                    tool_name_from_call,
                                );
                            }
                        };

                        let args: serde_json::Value =
                            serde_json::from_str(&tool_call.function.arguments).unwrap_or_default();

                        let _ = event_tx.send(Event::ToolCall {
                            name: tool.name().to_string(),
                            args: args.clone(),
                        });

                        if let Some(allowlist) = &soul.tool_allowlist
                            && !allowlist.contains(&tool.name().to_string()) {
                                let tool_name = tool.name().to_string();
                                return (
                                    tool_call.id.clone(),
                                    Ok(format!("Tool '{}' is not allowed.", tool.name())),
                                    tool_name,
                                );
                            }

                        let policy_result = policy_engine.evaluate_tool_call(tool.name(), &args);
                        let (needs_approval, _) = match policy_result {
                            crate::security::policy::PolicyAction::Deny(reason) => {
                                let tool_name = tool.name().to_string();
                                return (
                                    tool_call.id.clone(),
                                    Ok(format!("Denied by policy: {}", reason)),
                                    tool_name,
                                );
                            }
                            crate::security::policy::PolicyAction::RequireApproval(reason) => {
                                (true, reason)
                            }
                            crate::security::policy::PolicyAction::Allow => (
                                tool.requires_approval(&args),
                                tool.approval_description(&args),
                            ),
                        };

                        if needs_approval {
                            let approval_id = uuid::Uuid::new_v4().to_string();
                            log::info!(
                                "TOOL APPROVAL REQUIRED: tool='{}', id='{}', args={}",
                                tool.name(),
                                approval_id,
                                args
                            );
                            let _ = event_tx.send(Event::ApprovalRequest {
                                id: approval_id.clone(),
                                tool: tool.name().to_string(),
                                args: args.clone(),
                            });
                            let mut approved = false;
                            while let Ok((id, result)) = approval_rx.recv().await {
                                if id == approval_id {
                                    approved = result;
                                    break;
                                }
                            }
                            if !approved {
                                let tool_name = tool.name().to_string();
                                return (
                                    tool_call.id.clone(),
                                    Ok("Denied by user.".to_string()),
                                    tool_name,
                                );
                            }
                        }

                        let _ = hooks.trigger_before_tool_call(tool.name(), &args).await;
                        log::info!("Executing tool: {} with args: {}", tool.name(), args);

                        {
                            let mut counts = tool_call_counts.lock().await;
                            *counts.entry(tool.name().to_string()).or_insert(0) += 1;
                        }

                        let _ = event_tx.send(Event::ForensicLog {
                            id: forensic_id.clone(),
                            action: format!("Executing {}", tool.name()),
                            hypothesis: format!("Using {} with args {}", tool.name(), args),
                            observation: None,
                        });

                        let result = tool.call(args).await;
                        let mut result_str = match &result {
                            Ok(s) => s.clone(),
                            Err(e) => e.to_string(),
                        };

                        // TOOL RESULT COMPRESSION: Prevent token explosion from long tool outputs
                        if result_str.len() > 2000 {
                            if tool.name() == "web_fetch" || tool.name() == "browser" {
                                log::info!("Agent: Compressing large result from '{}' ({} chars)", tool.name(), result_str.len());
                                let preview = result_str.chars().take(800).collect::<String>();
                                result_str = format!("{}... [TRUNCATED due to size. The full content was omitted to save tokens. Use more specific search queries if needed.]", preview);
                            } else if result_str.len() > 10000 {
                                // Generic compression for other tools if extremely long
                                log::warn!("Agent: Generic compression for extremely long output from '{}'", tool.name());
                                let preview = result_str.chars().take(2000).collect::<String>();
                                result_str = format!("{}... [EXTREMELY LARGE OUTPUT TRUNCATED]", preview);
                            }
                        }

                        let _ = event_tx.send(Event::ForensicLog {
                            id: forensic_id,
                            action: format!("Completed {}", tool.name()),
                            hypothesis: "".to_string(),
                            observation: Some(if result_str.len() > 100 {
                                format!("{}...", &result_str[..100])
                            } else {
                                result_str.clone()
                            }),
                        });

                        let _ = hooks
                            .trigger_after_tool_call(tool.name(), &result_str)
                            .await;
                        let tool_name = tool.name().to_string();
                        (tool_call.id.clone(), result.map_err(|e| anyhow!(e.0)), tool_name)
                    }));
                }

                let task_results = futures::future::join_all(tool_tasks).await;
                let mut tool_errors = Vec::new();

                for task_res in task_results {
                    if let Ok((tool_call_id, result_res, tool_name)) = task_res {
                        let result = match result_res {
                            Ok(r) => r,
                            Err(e) => {
                                let error_string = format!("Tool '{}' failed with error: {}", tool_name, e);
                                log::error!("{}", error_string);
                                tool_errors.push(error_string.clone());
                                e.to_string()
                            }
                        };

                        if result.contains("### INJECTED PLAYBOOK")
                            && let Some(line) = result.lines().next() {
                                let name = line.replace("### INJECTED PLAYBOOK: ", "").trim().to_string();
                                let _ = self.register_playbook(session_id, name, result.clone()).await;
                            }
                        let _ = self.event_tx.send(Event::ToolResult {
                            result: result.clone(),
                        });
                        let _ = self.record_step(crate::trajectory::TrajectoryStep::Observation {
                            result: result.clone(),
                            action_id: None,
                            timestamp: chrono::Utc::now(),
                        }).await;
                        let tool_result_msg = Message {
                            role: "tool".to_string(),
                            name: Some(tool_name),
                            content: Some(MessageContent::Text(result.clone())),
                            tool_call_id: Some(tool_call_id),
                            ..Default::default()
                        };

                        // VOLATILE TOOL OUTPUT OPTIMIZATION:
                        let is_volatile = tool_result_msg
                            .content
                            .as_ref()
                            .map(|c| c.to_string().len() > 1024)
                            .unwrap_or(false);

                        if let Some(store) = &self.session_store
                            && !is_volatile {
                                let _ = store.save_message(session_id, &tool_result_msg).await;
                            }
                        let mut state = state_arc.lock().await;
                        state.history.push(tool_result_msg);
                    }
                }

                if !tool_errors.is_empty() {
                    let error_summary = tool_errors.join("\n");
                    let rescue_message = Message {
                        role: "system".to_string(),
                        content: Some(MessageContent::Text(format!(
                            "Some tools failed to execute. Please review the errors and try a different approach. Errors:\n{}",
                            error_summary
                        ))),
                        ..Default::default()
                    };
                    let mut state = state_arc.lock().await;
                    state.history.push(rescue_message);
                }

                continue; // Next iteration to let model process tool results
            }

            if response.content.is_none() && response.tool_calls.is_none() {
                log::warn!(
                    "Model returned empty response. Breaking loop to avoid hang."
                );
                break Ok(String::new());
            }

            let raw_content = response
                .content
                .as_ref()
                .map(|c| c.to_string())
                .unwrap_or_default();

            // THOUGHT EXTRACTION: Extract <think>...</think> and strip it from final user content
            log::info!("[SESSION: {}] Processing final content response...", session_id);
            let mut final_content = raw_content.clone();
            let mut thoughts = Vec::new();

            while let Some(start) = final_content.find("<think>") {
                if let Some(end) = final_content[start..].find("</think>") {
                    let absolute_end = start + end + 8; // 8 is length of </think>
                    let thought_content = final_content[start + 7..start + end].trim().to_string();
                    thoughts.push(thought_content.clone());

                    let _ = self.record_step(crate::trajectory::TrajectoryStep::Thought {
                        content: thought_content.clone(),
                        timestamp: chrono::Utc::now(),
                    }).await;

                    // Send thought event
                    let _ = self.event_tx.send(Event::AgentThought {
                        content: MessageContent::Text(thought_content),
                    });

                    final_content.replace_range(start..absolute_end, "");
                } else {
                    break;
                }
            }
            let final_content = final_content.trim().to_string();

            let _ = self.record_step(crate::trajectory::TrajectoryStep::Response {
                content: final_content.clone(),
                timestamp: chrono::Utc::now(),
            }).await;

            let final_msg = Message {
                role: "assistant".to_string(),
                content: if final_content.is_empty() {
                    None
                } else {
                    Some(MessageContent::Text(final_content.clone()))
                },
                ..Default::default()
            };
            let _ = self.hooks.trigger_message_sent(&final_msg).await;

            // BATCHED REFLECTION OPTIMIZATION (Backgrounded):
            let current_interaction_count = self
                .interaction_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                + 1;
            if current_interaction_count.is_multiple_of(5) {
                let agent_clone = self.clone();
                tokio::spawn(async move {
                    if let Err(e) = agent_clone.reflect().await {
                        log::error!("Error during background reflection: {}", e);
                    }
                });
            }

            // Auto-index assistant response (Backgrounded)
            if let Some(nexus) = &self.knowledge_nexus {
                let nexus = nexus.clone();
                let content_to_index = final_content.clone();
                tokio::spawn(async move {
                    let id = uuid::Uuid::new_v4().to_string();
                    let _ = nexus
                        .remember_batch(vec![(id, content_to_index)])
                        .await;
                });
            }

            let _ = self.event_tx.send(Event::AgentResponse {
                content: response
                    .content
                    .unwrap_or(MessageContent::Text("".to_string())),
            });
            return Ok(final_content);
        } }).await
    }

    pub async fn plan_retrieval(&self, query: &str) -> pharmakon_memory::RagStrategy {
        if query.to_lowercase().contains("deep research") || query.len() > 200 {
            pharmakon_memory::RagStrategy::DeepResearch {
                max_depth: 3,
                beam_width: 2,
            }
        } else {
            pharmakon_memory::RagStrategy::Hybrid { initial_top_k: 5 }
        }
    }

    pub async fn add_to_working_memory(&self, content: String, importance: f32, source: String) {
        // SEMANTIC NOISE GATE: Filter out low-importance signals
        if importance < 0.3 {
            log::debug!(
                "Agent: Noise Gate blocked low-importance context ({:.2}) from {}",
                importance,
                source
            );
            return;
        }

        let state_arc = self.get_current_session_state().await;
        let mut state = state_arc.lock().await;
        let wm = &mut state.working_memory;

        // Micro-summary generation
        let summary = if content.len() > 150 {
            let compactor = self.compactor.lock().await;
            compactor.compact_block(&content, 0.5).await.ok()
        } else {
            Some(content.clone())
        };

        wm.push(WorkingMemoryUnit {
            content,
            summary,
            importance,
            timestamp: chrono::Utc::now(),
            tokens: 0, // Should be estimated
            source,
        });

        // Keep working memory focused
        if wm.len() > 10 {
            wm.sort_by(|a, b| b.importance.partial_cmp(&a.importance).unwrap());
            wm.truncate(8);
        }
    }

    pub async fn scout_workspace(&self, query: &str) -> Option<String> {
        let nexus = self.knowledge_nexus.clone()?;
        let results = nexus.smart_search(query, 5).await.ok()?;
        if results.is_empty() {
            return None;
        }
        Some(results.join("\\n---\\n"))
    }

    pub async fn reflect(&self) -> Result<()> {
        log::info!("Agent: Performing periodic self-reflection...");
        let state_arc = self.get_current_session_state().await;
        let state = state_arc.lock().await;

        if state.history.len() < 4 {
            return Ok(());
        }

        let context = state
            .history
            .iter()
            .rev()
            .take(10)
            .map(|m| {
                format!(
                    "{}: {}",
                    m.role,
                    m.content
                        .as_ref()
                        .map(|c| c.to_string())
                        .unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\\n");

        drop(state);

        let system_prompt = "Analyze the recent conversation and extract ONE key learned fact, architectural decision, or verified constraint. Output only the distilled insight.";
        let request = CompletionRequest {
            messages: vec![
                Message {
                    role: "system".to_string(),
                    content: Some(MessageContent::Text(system_prompt.to_string())),
                    ..Default::default()
                },
                Message {
                    role: "user".to_string(),
                    content: Some(MessageContent::Text(format!("Context:\\n{}", context))),
                    ..Default::default()
                },
            ],
            temperature: Some(0.3),
            max_tokens: Some(200),
            tools: None,
        };

        let model = self.model.lock().await;
        if let Ok(response) = model.complete(request).await
            && let Some(insight) = response.content.as_ref().and_then(|c| c.as_text())
                && !insight.trim().is_empty() {
                    log::info!("Agent Reflection Insight: {}", insight);

                    // Save to fact memory for long-term recall
                    if let Some(fact_mem) = &self.fact_memory {
                        let mut fm = fact_mem.lock().await;
                        fm.add_belief(insight, 0.9, "learned_context")?;
                    }

                    // Also index into semantic search for recovery across sessions
                    if let Some(search) = &self.semantic_search {
                        let _ = search.remember(insight).await;
                    }
                }
        Ok(())
    }

    pub async fn add_fact(&self, fact: &str) -> Result<()> {
        if let Some(fact_memory) = &self.fact_memory {
            let mut fm = fact_memory.lock().await;
            fm.add_belief(fact, 0.8, "learned_fact")?;
        }
        Ok(())
    }

    async fn handle_model_command(&self, cmd: &str) -> Result<String> {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.len() < 2 {
            return Ok("Usage: /model <model_id>".to_string());
        }
        let model_id = parts[1];
        if let Some(new_model) = crate::providers::registry::ModelRegistry::get_model(model_id) {
            let mut model = self.model.lock().await;
            *model = new_model;
            let _ = self.event_tx.send(Event::ModelSwitched {
                model_id: model_id.to_string(),
            });
            Ok(format!("Switched to model: {}", model_id))
        } else {
            Ok(format!("Model not found: {}", model_id))
        }
    }

    pub async fn soul(&self) -> crate::soul::Soul {
        let pm = self.prompt_manager.lock().await;
        pm.soul().clone()
    }

    pub async fn set_soul(&self, soul: crate::soul::Soul) {
        let mut pm = self.prompt_manager.lock().await;
        pm.set_soul(soul);
    }

    pub async fn add_contribution(
        &self,
        contribution: Box<dyn crate::system_prompt::SystemPromptContribution>,
    ) {
        let mut pm = self.prompt_manager.lock().await;
        pm.add_contribution(contribution);
    }

    pub fn with_isolated_knowledge(self) -> Self {
        // In a real impl, this would isolate the fact memory/nexus
        self
    }

    pub async fn commit_knowledge(&self) -> Result<()> {
        Ok(())
    }

    pub async fn trajectory_steps(&self) -> Vec<crate::trajectory::TrajectoryStep> {
        let t = self.trajectory.lock().await;
        t.steps.clone()
    }

    pub async fn get_history(&self) -> Vec<Message> {
        let state_arc = self.get_current_session_state().await;
        let state = state_arc.lock().await;
        state.history.clone()
    }

    pub async fn record_step(&self, step: crate::trajectory::TrajectoryStep) -> Result<()> {
        let session_id = CURRENT_SESSION_ID
            .try_with(|id| id.clone())
            .unwrap_or_else(|_| "default".to_string());

        {
            let mut trajectory = self.trajectory.lock().await;
            trajectory.add_step(step.clone());
        }

        if let Some(store) = &self.session_store {
            let event_type = match &step {
                crate::trajectory::TrajectoryStep::Intent { .. } => "intent",
                crate::trajectory::TrajectoryStep::Thought { .. } => "thought",
                crate::trajectory::TrajectoryStep::Action { .. } => "action",
                crate::trajectory::TrajectoryStep::Observation { .. } => "observation",
                crate::trajectory::TrajectoryStep::Response { .. } => "response",
            };
            let payload = serde_json::to_value(&step)?;
            let _ = store
                .save_trajectory_event(&session_id, event_type, &payload)
                .await;
        }
        Ok(())
    }

    pub async fn register_playbook(
        &self,
        session_id: &str,
        name: String,
        content: String,
    ) -> Result<()> {
        let state_arc = self.get_session_state(session_id).await;
        let mut state = state_arc.lock().await;
        if !state.active_playbooks.iter().any(|(n, _)| n == &name) {
            state.active_playbooks.push((name, content));
        }
        Ok(())
    }

    pub async fn reset_history(&self) -> Result<()> {
        self.clear_history().await
    }

    pub async fn reset_session_history(&self, session_id: &str) -> Result<()> {
        let mut states = self.session_states.lock().await;
        states.remove(session_id);
        Ok(())
    }

    pub fn approve(&self, id: String, approved: bool) {
        let _ = self.approval_tx.send((id, approved));
    }

    pub async fn get_token_usage(&self) -> (u64, f64) {
        (
            self.total_tokens.load(std::sync::atomic::Ordering::SeqCst),
            *self.total_cost.lock().await,
        )
    }

    pub async fn model_name(&self) -> String {
        let m = self.model.lock().await;
        m.name().to_string()
    }

    pub async fn heartbeat(&self) -> Result<String> {
        Ok("HEARTBEAT_OK".to_string())
    }

    pub async fn perform_maintenance(&self) -> Result<()> {
        log::info!("Agent: Performing autonomous maintenance...");
        Ok(())
    }
}
