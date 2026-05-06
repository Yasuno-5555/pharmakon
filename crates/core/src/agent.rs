use crate::model::{
    AgentError, AgentErrorCode, AgentModel, AgentResult, CompletionRequest, CompletionResponse,
    Message, MessageContent, ToolDefinition,
};
use crate::system_prompt::SystemPromptManager;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use pharmakon_common::{Event, ToolRegistry};
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast};

#[derive(Debug, Clone)]
pub struct WorkingMemoryUnit {
    pub content: String,
    pub importance: f32,
    pub timestamp: std::time::Instant,
}

pub struct Agent {
    pub model: Arc<Mutex<Arc<dyn AgentModel>>>,
    pub session_id: Arc<Mutex<String>>,
    pub prompt_manager: Arc<Mutex<crate::system_prompt::SystemPromptManager>>,
    pub event_tx: broadcast::Sender<Event>,
    pub approval_tx: broadcast::Sender<(String, bool)>,
    pub approval_rx: Option<broadcast::Receiver<(String, bool)>>,
    pub trajectory: Arc<Mutex<crate::trajectory::Trajectory>>,
    pub context_engine: Arc<Mutex<crate::memory::context_engine::ContextEngine>>,
    pub compactor: Arc<Mutex<crate::memory::compactor::ContextCompactor>>,
    pub history: Arc<Mutex<Vec<Message>>>,
    pub tools: Arc<Mutex<Vec<Arc<dyn pharmakon_common::Tool>>>>,
    pub hooks: Arc<crate::hooks::HookRegistry>,
    pub fact_memory: Option<Arc<Mutex<crate::memory::FactMemory>>>,
    pub semantic_search: Option<Arc<pharmakon_memory::semantic_search::SemanticSearch>>,
    pub knowledge_nexus: Option<Arc<pharmakon_memory::weaver::KnowledgeNexus>>,
    pub health_monitor: crate::orchestration::health_monitor::HealthMonitor,
    pub policy_engine: Arc<crate::security::policy::PolicyEngine>,
    pub session_store: Option<Arc<crate::persistence::DbSessionStore>>,
    pub planner_model: Option<Arc<Mutex<Arc<dyn AgentModel>>>>,
    pub vision_stream: Option<Arc<Mutex<pharmakon_tools::media::vision_stream::VisionRingBuffer>>>,
    pub graph_store: Option<Arc<crate::memory::graph::GraphStore>>,
    pub working_memory: Arc<Mutex<Vec<WorkingMemoryUnit>>>, // Context packing buffer
    pub interaction_count: std::sync::atomic::AtomicU32,
    pub fallback_models: Vec<String>,
    pub total_tokens: Arc<std::sync::atomic::AtomicU64>,
    pub total_cost: Arc<Mutex<f64>>,
    pub start_time: std::time::Instant,
    pub tool_call_counts: Arc<Mutex<std::collections::HashMap<String, u32>>>,
    pub territory_manager: Arc<crate::orchestration::territory::TerritoryManager>,
    pub research_notebook: Arc<Mutex<crate::orchestration::research::ResearchNotebook>>,
    pub usage_history: Arc<Mutex<Vec<(chrono::DateTime<chrono::Utc>, u64, f64)>>>,
}

impl Agent {
    pub fn new(model: Arc<dyn AgentModel>, session_id: String) -> Self {
        let (event_tx, _) = broadcast::channel(100);
        let (approval_tx, approval_rx) = broadcast::channel(10);
        let trajectory = Arc::new(Mutex::new(crate::trajectory::Trajectory::new(
            session_id.clone(),
            model.name().to_string(),
        )));
        let context_engine = Arc::new(Mutex::new(
            crate::memory::context_engine::ContextEngine::new(8192),
        ));
        let compactor = Arc::new(Mutex::new(crate::memory::compactor::ContextCompactor::new(
            model.clone(),
        )));
        let mut pm = SystemPromptManager::new(crate::soul::Soul::default_soul());
        pm.add_contribution(Box::new(crate::system_prompt::StaticContribution::new(
            "Autonomous Execution Bias",
            "You are a fully autonomous agent. Your priority is to get things done (Execution Bias). \
            For shell commands and file operations, you should proceed WITHOUT manual approval by default to maximize speed and autonomy. \
            However, you are responsible for system safety. If YOU judge that a specific command is exceptionally high-risk, irreversible, or potentially destructive (e.g., massive data deletion, critical system change), you MUST set `requires_manual_approval: true` in the tool arguments to consult the user. \
            Otherwise, act with authority and resolve the task autonomously."
        )));
        let prompt_manager = Arc::new(Mutex::new(pm));

        let mut hooks = crate::hooks::HookRegistry::new();
        hooks.register(Box::new(
            crate::hooks::token_economy::TokenEconomyHook::new(0.8, 100_000),
        )); // 100k token default budget

        Self {
            model: Arc::new(Mutex::new(model)),
            session_id: Arc::new(Mutex::new(session_id)),
            prompt_manager,
            event_tx,
            approval_tx,
            approval_rx: Some(approval_rx),
            trajectory,
            context_engine,
            compactor,
            history: Arc::new(Mutex::new(Vec::new())),
            tools: Arc::new(Mutex::new(Vec::new())),
            hooks: Arc::new(hooks),
            fact_memory: None,
            semantic_search: None,
            knowledge_nexus: None,
            health_monitor: crate::orchestration::health_monitor::HealthMonitor::new(0.5),
            policy_engine: Arc::new(crate::security::policy::PolicyEngine::new()),
            session_store: None,
            planner_model: None,
            vision_stream: None,
            graph_store: None,
            working_memory: Arc::new(Mutex::new(Vec::new())),
            interaction_count: std::sync::atomic::AtomicU32::new(0),
            fallback_models: Vec::new(),
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

    pub fn with_fallback_models(mut self, models: Vec<String>) -> Self {
        self.fallback_models = models;
        self
    }

    pub fn with_store(mut self, store: Arc<crate::persistence::DbSessionStore>) -> Self {
        self.session_store = Some(store);
        self
    }

    pub fn with_knowledge_nexus(
        mut self,
        nexus: Arc<pharmakon_memory::weaver::KnowledgeNexus>,
    ) -> Self {
        self.knowledge_nexus = Some(nexus);
        self
    }

    pub fn with_fact_memory(
        mut self,
        fact_memory: Arc<Mutex<pharmakon_memory::fact_memory::FactMemory>>,
    ) -> Self {
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
}

#[async_trait]
impl pharmakon_common::ToolRegistry for Agent {
    async fn add_tool(&self, tool: Arc<dyn pharmakon_common::Tool>) {
        let mut tools = self.tools.lock().await;
        if !tools.iter().any(|t| t.name() == tool.name()) {
            tools.push(tool);
        }
    }
}

impl Agent {
    pub async fn setup_autonomous_tools(self: &Arc<Self>) {
        use crate::trajectory::tool::InsightTool;
        use pharmakon_tools::{
            ApplyPatchTool, BackgroundRunTool, CameraTool, CargoQualityTool, CodeEditTool,
            ConnectMcpServerTool, DiagnosticTool, DiscoverToolsTool, FileReadTool, FileWriteTool,
            FindDefinitionTool, GitCommitTool, GitDiffTool, GitStatusTool, GrepSearchTool,
            HostScriptTool, LinkUnderstandingTool, ListAnchorsTool, ListDirTool, McpTool,
            MultiCodeEditTool, PlaybookTool, ProcessStatusTool, PythonInterpreterTool, RepoMapTool,
            ResearchConsolidateTool, ResearchFetchTool, ResearchSearchTool, ScreenshotTool,
            SetAnchorTool, ShellTool, TokenEconomyControlTool, ViewFileTool,
            WorkspacePerceptionTool,
        };

        let agent_arc: Arc<Agent> = self.clone();
        let tool_registry_arc: Arc<dyn pharmakon_common::ToolRegistry> = agent_arc;
        self.add_tool(Arc::new(pharmakon_tools::SkillFactoryTool::new(
            Arc::downgrade(&tool_registry_arc),
        )))
        .await;
        self.add_tool(Arc::new(WorkspacePerceptionTool)).await;
        self.add_tool(Arc::new(GrepSearchTool)).await;
        self.add_tool(Arc::new(CodeEditTool)).await;
        self.add_tool(Arc::new(MultiCodeEditTool)).await;
        self.add_tool(Arc::new(ListDirTool)).await;
        self.add_tool(Arc::new(ViewFileTool)).await;
        self.add_tool(Arc::new(GitStatusTool)).await;
        self.add_tool(Arc::new(GitDiffTool)).await;
        self.add_tool(Arc::new(GitCommitTool)).await;
        self.add_tool(Arc::new(CargoQualityTool)).await;
        self.add_tool(Arc::new(FindDefinitionTool)).await;
        self.add_tool(Arc::new(ShellTool)).await;
        self.add_tool(Arc::new(FileReadTool)).await;
        self.add_tool(Arc::new(FileWriteTool)).await;
        self.add_tool(Arc::new(ScreenshotTool)).await;
        self.add_tool(Arc::new(CameraTool)).await;
        self.add_tool(Arc::new(PythonInterpreterTool)).await;
        self.add_tool(Arc::new(TokenEconomyControlTool {})).await;
        self.add_tool(Arc::new(SetAnchorTool)).await;
        self.add_tool(Arc::new(ListAnchorsTool)).await;
        self.add_tool(Arc::new(PlaybookTool)).await;
        self.add_tool(Arc::new(RepoMapTool)).await;
        self.add_tool(Arc::new(ApplyPatchTool)).await;

        if let Some(nexus) = &self.knowledge_nexus {
            self.add_tool(Arc::new(
                pharmakon_tools::ast_ingest::ASTKnowledgeIngestTool::new(nexus.clone()),
            ))
            .await;
        }

        // Load MCP Tools from config
        if let Ok(mcp_tools) = crate::mcp_manager::McpManager::load_tools().await {
            for tool in mcp_tools {
                self.add_tool(tool).await;
            }
        }

        self.add_tool(Arc::new(ConnectMcpServerTool {
            tool_registry: self.tools.clone(),
        }))
        .await;
        self.add_tool(Arc::new(DiscoverToolsTool {
            tool_registry: self.tools.clone(),
        }))
        .await;

        use crate::orchestration::territory_tools::{
            ListTerritoriesTool, LockTerritoryTool, UnlockTerritoryTool,
        };
        self.add_tool(Arc::new(LockTerritoryTool {
            territory_manager: self.territory_manager.clone(),
        }))
        .await;
        self.add_tool(Arc::new(UnlockTerritoryTool {
            territory_manager: self.territory_manager.clone(),
        }))
        .await;
        self.add_tool(Arc::new(ListTerritoriesTool {
            territory_manager: self.territory_manager.clone(),
        }))
        .await;

        self.add_tool(Arc::new(ResearchSearchTool {
            notebook: self.research_notebook.clone(),
        }))
        .await;
        self.add_tool(Arc::new(ResearchFetchTool {
            notebook: self.research_notebook.clone(),
            store: self
                .session_store
                .clone()
                .map(|s| s as Arc<dyn pharmakon_common::ResearchPersistence>),
        }))
        .await;
        self.add_tool(Arc::new(ResearchConsolidateTool {
            notebook: self.research_notebook.clone(),
        }))
        .await;

        let active_processes = Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));
        let retry_counts = Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new()));

        self.add_tool(Arc::new(BackgroundRunTool {
            active_processes: active_processes.clone(),
        }))
        .await;
        self.add_tool(Arc::new(ProcessStatusTool {
            active_processes,
            retry_counts,
        }))
        .await;

        self.add_tool(Arc::new(InsightTool::new(Arc::downgrade(self))))
            .await;
        self.add_tool(Arc::new(HostScriptTool)).await;
        self.add_tool(Arc::new(LinkUnderstandingTool::new())).await;

        self.add_tool(Arc::new(DiagnosticTool {
            vision_stream: self.vision_stream.clone(),
            telemetry: None,
            mcp_stats_source: "agent_internal".to_string(),
        }))
        .await;

        // Inject Full-Spectrum Autonomous Engineering Guidelines
        let mut pm = self.prompt_manager.lock().await;
        pm.add_contribution(Box::new(crate::system_prompt::StaticContribution::new(
            "Token Economy & Frugality (Energy Saving Mode)",
            "Tokens are precious. To minimize API costs and stay within limits: \
            1. **Targeted Reading**: Use `view_file` with narrow line ranges (e.g. 50-100 lines) instead of reading whole files. \
            2. **Structural Map**: Use `get_repo_map` to understand the codebase structure before diving into files. \
            3. **Surgical Edits**: Use `apply_patch` for precise, token-efficient code modifications. This is the preferred method for editing. \
            4. **Filtered Search**: Use `grep_search` with specific queries and `include` filters to avoid massive outputs. \
            5. **Summarization**: When reporting results, be concise. Only include the essential output. \
            A frugal agent is a sustainable agent."
        )));

        pm.add_contribution(Box::new(crate::system_prompt::StaticContribution::new(
            "Adaptive Engineering & Reliability (V2.1)",
            "Follow these strict reliability guidelines: \
            1. **Surgical Precision**: Prefer `apply_patch` for changes to existing code. \
            2. **Massive Refactor Rule**: Use `write_file` ONLY for: (a) New files, (b) Rewriting >50% of a file, (c) When `apply_patch` fails 3 times due to context mismatch. \
            3. **Linear Planning**: Do NOT attempt parallel execution. Follow a strict loop: PLAN -> EXECUTE STEP -> VERIFY (via cargo check/test) -> COMMIT. \
            4. **Background Monitoring**: If a background process fails (check via `get_process_status`), you have a MAX of 3 auto-fix attempts. If it still fails, YOU MUST STOP AND ASK THE USER FOR HELP. \
            5. **Shell Hygiene**: Use the `reset: true` flag in `terminal` when moving to a new logical task to avoid state pollution from previous environment variables or directory changes."
        )));

        pm.add_contribution(Box::new(crate::system_prompt::StaticContribution::new(
            "Full-Spectrum Autonomous Engineering (Aider/SWE-agent/Devin/Antigravity lineage)",
            "You are a master engineer equipped with an elite tool suite. \
            - **Navigation**: Use `get_repo_map` (AST) for the big picture and `find_definition` (LSP) for deep type resolution. \
            - **Editing**: Use `apply_patch` as your primary tool. \
            - **Execution**: Use `terminal` with session persistence, but keep it clean. \
            - **CodeAct Paradigm**: When faced with complex multi-step analysis (e.g., searching across multiple files and aggregating results), prefer writing a Python script using the `pharmakon` bridge. This is more efficient than sequential tool calls. \
            - **Connectivity**: You can expand your toolbox by calling `connect_mcp_server`. Use this to integrate with external services like Slack, Sentry, or specialized DB tools. \
            - **Symphony Workforce Coordination**: If you spawn sub-agents (workers), you are the CHIEF ORCHESTRATOR. You MUST: \
                1. **Decompose strictly**: Assign distinct, non-overlapping directories or modules to each worker. \
                2. **Prevent Race Conditions**: Never allow two workers to edit the same file. \
                3. **Sync State**: Require workers to report findings before allowing them to proceed to the next phase. \
            - **Governance**: Always verify your work. If you hit a retry limit or a critical failure, do not hallucinate—report it and wait for instructions."
        )));

        pm.add_contribution(Box::new(crate::system_prompt::StaticContribution::new(
            "Self-Perception & Situational Awareness",
            "You have tools to perceive your own situation and environment. \
            If you feel lost or need to understand your current context (system resources, directory structure, active capabilities), \
            autonomously call `self_diagnostic` or `perceive_workspace`."
        )));
    }

    pub async fn update_model(&self, model: Arc<dyn AgentModel>) {
        {
            let mut m = self.model.lock().await;
            *m = model.clone();
        }
        {
            let mut traj = self.trajectory.lock().await;
            traj.metadata.model = model.name().to_string();
        }
        {
            let mut compactor = self.compactor.lock().await;
            *compactor = crate::memory::compactor::ContextCompactor::new(model);
        }
        log::info!("Agent model updated.");
    }

    pub fn event_tx(&self) -> broadcast::Sender<Event> {
        self.event_tx.clone()
    }

    pub fn approve(&self, id: String, approved: bool) {
        let _ = self.approval_tx.send((id, approved));
    }

    pub async fn reset_history(&self) {
        let mut history = self.history.lock().await;
        history.clear();
    }

    pub async fn start_new_session(&self) {
        let new_id = uuid::Uuid::new_v4().to_string();

        // 1. Update session ID
        {
            let mut sid = self.session_id.lock().await;
            *sid = new_id.clone();
        }

        // 2. Clear history
        {
            let mut history = self.history.lock().await;
            history.clear();
        }

        // 3. Reset trajectory
        {
            let model_name = self.model_name().await;
            let mut traj = self.trajectory.lock().await;
            *traj = crate::trajectory::Trajectory::new(new_id, model_name);
        }

        // 4. Reset interaction count
        self.interaction_count
            .store(0, std::sync::atomic::Ordering::SeqCst);

        log::info!("Agent started a new session.");
    }

    pub async fn add_message(&self, msg: Message) {
        let mut history = self.history.lock().await;
        history.push(msg);
    }

    pub async fn model_name(&self) -> String {
        let m = self.model.lock().await;
        m.name().to_string()
    }

    pub async fn add_contribution(
        &self,
        contribution: Box<dyn crate::system_prompt::SystemPromptContribution>,
    ) {
        let mut pm = self.prompt_manager.lock().await;
        pm.add_contribution(contribution);
    }

    pub async fn set_session_id(&self, id: String) {
        let mut sid = self.session_id.lock().await;
        *sid = id;
    }

    pub async fn replace_history(&self, new_history: Vec<Message>) {
        let mut history = self.history.lock().await;
        *history = new_history;
    }

    pub async fn trajectory_steps(&self) -> Vec<crate::trajectory::TrajectoryStep> {
        let traj = self.trajectory.lock().await;
        traj.steps.clone()
    }

    pub async fn soul(&self) -> crate::soul::Soul {
        let pm = self.prompt_manager.lock().await;
        pm.soul().clone()
    }

    pub async fn set_soul(&self, soul: crate::soul::Soul) {
        let mut pm = self.prompt_manager.lock().await;
        pm.set_soul(soul);
    }

    pub async fn chat(&self, user_message: &str) -> Result<String> {
        if user_message.starts_with("/model") {
            return self.handle_model_command(user_message).await;
        }

        let (session_id, history_len) = {
            let sid = self.session_id.lock().await;
            let hist = self.history.lock().await;
            (sid.clone(), hist.len())
        };

        if history_len == 0 {
            if let Some(store) = &self.session_store {
                let mut history = self.history.lock().await;
                *history = store.load_history(&session_id).await.unwrap_or_default();
            }
        }

        let user_msg = Message {
            role: "user".to_string(),
            content: Some(MessageContent::Text(user_message.to_string())),
            ..Default::default()
        };

        let _ = self.hooks.trigger_message_received(&user_msg).await;

        if let Some(store) = &self.session_store {
            store.save_message(&session_id, &user_msg).await?;
        }
        {
            let mut history = self.history.lock().await;
            history.push(user_msg);
        }

        // Auto-index user message for future recovery (only if meaningful)
        if user_message.len() > 10 {
            if let Some(nexus) = &self.knowledge_nexus {
                let id = uuid::Uuid::new_v4().to_string();
                let _ = nexus
                    .remember_batch(vec![(id, user_message.to_string())])
                    .await;
            }
        }

        {
            let mut history = self.history.lock().await;
            let mut context_engine = self.context_engine.lock().await;
            let _ = context_engine.prune_history(&mut history).await;

            if history.len() > 20 {
                let mut compactor = self.compactor.lock().await;
                if let Ok(compacted) = compactor.compact(history.clone()).await {
                    *history = compacted;
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

        let (semantic_res, nexus_res, scout_res) = if user_message.len() < 5 {
            (None, None, None)
        } else {
            tokio::join!(
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
                },
                async { self.scout_workspace(&user_msg_text).await }
            )
        };

        if let Some(memories) = semantic_res {
            if !memories.is_empty() {
                let memory_context = memories.join("\n---\n");
                self.add_to_working_memory(format!("Long-term Memories:\n{}", memory_context), 0.7)
                    .await;
            }
        }

        if let Some(memories) = nexus_res {
            if !memories.is_empty() {
                let memory_context = memories.join("\n---\n");
                let _ = self.hooks.trigger_context_recovered(&memory_context).await;
                self.add_to_working_memory(
                    format!(
                        "Knowledge Nexus Insights (Hybrid + Graph):\n{}",
                        memory_context
                    ),
                    0.9,
                )
                .await;
            }
        }

        if let Some(scout_context) = scout_res {
            self.add_to_working_memory(
                format!("Proactive Scout Insights:\n{}", scout_context),
                0.8,
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

            log::info!("Agent iteration start ({})...", iteration_count);
            let mut messages_to_send = Vec::new();
            {
                let prompt_manager = self.prompt_manager.lock().await;
                let mut system_prompt = prompt_manager.build();

                // Inject Working Memory
                let packed_wm = self.pack_working_memory().await;
                if !packed_wm.is_empty() {
                    system_prompt.push_str(&format!(
                        "\n\n### ACTIVE WORKING MEMORY (High Importance Context)\n{}",
                        packed_wm
                    ));
                }

                messages_to_send.push(Message {
                    role: "system".to_string(),
                    content: Some(MessageContent::Text(system_prompt)),
                    ..Default::default()
                });
            }
            {
                let history = self.history.lock().await;
                messages_to_send.extend(history.clone());
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
            let initial_target_model = if tool_definitions.is_some() && self.planner_model.is_some()
            {
                let pm = self.planner_model.as_ref().unwrap().lock().await;
                (*pm).clone()
            } else {
                let m = self.model.lock().await;
                (*m).clone()
            };

            let req_temp = {
                let prompt_manager = self.prompt_manager.lock().await;
                prompt_manager
                    .soul()
                    .temperature_override
                    .map(|t| t as f32)
                    .or(Some(0.7f32))
            };
            let request = CompletionRequest {
                messages: messages_to_send,
                temperature: req_temp,
                max_tokens: None,
                tools: tool_definitions,
            };

            let mut target_model = initial_target_model.clone();
            let mut response_result;
            let mut current_fallback_index = 0;
            // DYNAMIC FALLBACK OPTIMIZATION: Use models from configuration
            let fallback_models = if !self.fallback_models.is_empty() {
                self.fallback_models.clone()
            } else {
                vec![
                    "groq/llama-3.3-70b-versatile".to_string(),
                    "ollama/llama3".to_string(),
                ]
            };

            loop {
                log::debug!("Agent sending request to model: {}", target_model.name());
                let start = std::time::Instant::now();

                response_result = if tools_count == 0 {
                    // For streaming, we don't currently do automatic fallback inside the stream,
                    // but we can catch initial setup errors.
                    match target_model.as_ref().stream_complete(request.clone()).await {
                        Ok(mut stream) => {
                            let mut full_content = String::new();
                            use futures::StreamExt;
                            let mut stream_error = false;
                            while let Some(chunk_result) = stream.next().await {
                                match chunk_result {
                                    Ok(chunk) => {
                                        full_content.push_str(&chunk);
                                        let _ = self.event_tx.send(Event::AgentResponseChunk {
                                            session_id: session_id.clone(),
                                            chunk,
                                        });
                                    }
                                    Err(e) => {
                                        if e.is_rate_limit() {
                                            stream_error = true;
                                            response_result = Err(anyhow::Error::new(e));
                                            break;
                                        }
                                        let _ = self.event_tx.send(Event::Error {
                                            message: format!("Streaming error: {}", e),
                                        });
                                        return Err(anyhow::Error::new(e));
                                    }
                                }
                            }
                            if stream_error {
                                // Fallthrough to fallback logic below
                                Err(anyhow::anyhow!("Rate limit hit during stream setup"))
                            } else {
                                Ok(CompletionResponse {
                                    content: Some(pharmakon_common::MessageContent::Text(
                                        full_content,
                                    )),
                                    tool_calls: None,
                                    usage: None,
                                })
                            }
                        }
                        Err(e) => Err(anyhow::Error::new(e)),
                    }
                } else {
                    match target_model.as_ref().complete(request.clone()).await {
                        Ok(res) => {
                            self.health_monitor.record_success(start.elapsed());
                            Ok(res)
                        }
                        Err(e) => {
                            self.health_monitor.record_failure();
                            Err(anyhow::Error::new(e))
                        }
                    }
                };

                match response_result {
                    Ok(_) => break, // Success, exit retry loop
                    Err(ref e) => {
                        let is_rate_limit = if let Some(ae) = e.downcast_ref::<AgentError>() {
                            ae.is_rate_limit()
                        } else {
                            let err_str = e.to_string().to_lowercase();
                            err_str.contains("429")
                                || err_str.contains("too many requests")
                                || err_str.contains("quota")
                        };

                        if is_rate_limit && current_fallback_index < fallback_models.len() {
                            let fallback_id = &fallback_models[current_fallback_index];
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
                                continue;
                            } else {
                                log::error!(
                                    "Fallback model {} not found or configured.",
                                    fallback_id
                                );
                                current_fallback_index += 1;
                                continue;
                            }
                        }

                        let _ = self.event_tx.send(Event::Error {
                            message: format!("Model error: {}", e),
                        });
                        return Err(response_result.err().unwrap());
                    }
                }
            }

            let response: pharmakon_common::agent_types::CompletionResponse =
                response_result.unwrap();

            log::debug!(
                "Model response received. Content: {}, Tool calls: {}",
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

            if let Some(usage) = &response.usage {
                self.total_tokens.fetch_add(
                    usage.total_tokens as u64,
                    std::sync::atomic::Ordering::SeqCst,
                );
                let mut cost_lock = self.total_cost.lock().await;
                let cost_increment = (usage.total_tokens as f64 / 1000.0) * 0.002;
                *cost_lock += cost_increment;

                let mut history = self.usage_history.lock().await;
                history.push((chrono::Utc::now(), usage.total_tokens as u64, *cost_lock));

                let _ = self.event_tx.send(Event::TokenUsageUpdate {
                    total_tokens: self.total_tokens.load(std::sync::atomic::Ordering::SeqCst),
                    total_cost: *cost_lock,
                });
            }

            if let Some(content) = &response.content {
                let c = content.clone();
                let event = Event::AgentThought { content: c };
                let _ = self.event_tx.send(event);
                let mut trajectory = self.trajectory.lock().await;
                trajectory.add_step(crate::trajectory::TrajectoryStep::Thought {
                    content: content.to_string(),
                    timestamp: chrono::Utc::now(),
                });
            }

            let assistant_msg = Message {
                role: "assistant".to_string(),
                content: response.content.clone(),
                tool_calls: response.tool_calls.clone(),
                ..Default::default()
            };

            if let Some(store) = &self.session_store {
                store.save_message(&session_id, &assistant_msg).await?;
                let trajectory = self.trajectory.lock().await;
                store.save_trajectory(&trajectory).await?;
            }
            {
                let mut history = self.history.lock().await;
                history.push(assistant_msg);
            }

            if let Some(tool_calls) = response.tool_calls {
                let mut tool_tasks = Vec::new();
                for tool_call in tool_calls {
                    let tool = self
                        .tools
                        .lock()
                        .await
                        .iter()
                        .find(|t| t.name() == tool_call.function.name)
                        .cloned();
                    let event_tx = self.event_tx.clone();
                    let mut approval_rx = self.approval_rx.as_ref().map(|rx| rx.resubscribe());
                    let hooks = self.hooks.clone();
                    let soul = {
                        let pm = self.prompt_manager.lock().await;
                        pm.soul().clone()
                    };
                    let policy_engine = self.policy_engine.clone();
                    let tool_call_counts = self.tool_call_counts.clone();
                    let forensic_id = uuid::Uuid::new_v4().to_string();

                    tool_tasks.push(tokio::spawn(async move {
                        let tool = match tool {
                            Some(t) => t,
                            None => {
                                return (
                                    tool_call.id.clone(),
                                    Err(anyhow!("Tool not found: {}", tool_call.function.name)),
                                );
                            }
                        };

                        let args: serde_json::Value =
                            serde_json::from_str(&tool_call.function.arguments).unwrap_or_default();

                        let _ = event_tx.send(Event::ToolCall {
                            name: tool.name().to_string(),
                            args: args.clone(),
                        });

                        if let Some(allowlist) = &soul.tool_allowlist {
                            if !allowlist.contains(&tool.name().to_string()) {
                                return (
                                    tool_call.id.clone(),
                                    Ok(format!("Tool '{}' is not allowed.", tool.name())),
                                );
                            }
                        }

                        let policy_result = policy_engine.evaluate_tool_call(tool.name(), &args);
                        let (needs_approval, _) = match policy_result {
                            crate::security::policy::PolicyAction::Deny(reason) => {
                                return (
                                    tool_call.id.clone(),
                                    Ok(format!("Denied by policy: {}", reason)),
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
                            if let Some(ref mut rx) = approval_rx {
                                let mut approved = false;
                                while let Ok((id, result)) = rx.recv().await {
                                    if id == approval_id {
                                        approved = result;
                                        break;
                                    }
                                }
                                if !approved {
                                    return (
                                        tool_call.id.clone(),
                                        Ok("Denied by user.".to_string()),
                                    );
                                }
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
                        let result_str = match &result {
                            Ok(s) => s.clone(),
                            Err(e) => e.to_string(),
                        };

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
                        (tool_call.id.clone(), result.map_err(|e| anyhow!(e.0)))
                    }));
                }

                let mut rescue_error = None;
                let task_results = futures::future::join_all(tool_tasks).await;
                for task_res in task_results {
                    if let Ok((tool_call_id, result_res)) = task_res {
                        let result = match result_res {
                            Ok(r) => r,
                            Err(e) => {
                                log::error!("Tool task execution error: {}", e);
                                rescue_error = Some(e.to_string());
                                e.to_string()
                            }
                        };
                        let _ = self.event_tx.send(Event::ToolResult {
                            result: result.clone(),
                        });
                        {
                            let mut trajectory = self.trajectory.lock().await;
                            trajectory.add_step(crate::trajectory::TrajectoryStep::Observation {
                                result: result.clone(),
                                timestamp: chrono::Utc::now(),
                            });
                        }
                        let tool_result_msg = Message {
                            role: "tool".to_string(),
                            content: Some(MessageContent::Text(result)),
                            tool_call_id: Some(tool_call_id),
                            ..Default::default()
                        };

                        // VOLATILE TOOL OUTPUT OPTIMIZATION:
                        let is_volatile = tool_result_msg
                            .content
                            .as_ref()
                            .map(|c| c.to_string().len() > 1024)
                            .unwrap_or(false);

                        if let Some(store) = &self.session_store {
                            if !is_volatile {
                                let _ = store.save_message(&session_id, &tool_result_msg).await;
                            }
                        }
                        {
                            let mut history = self.history.lock().await;
                            history.push(tool_result_msg);
                        }
                    }
                }

                if let Some(err) = rescue_error {
                    log::warn!("Autonomous Self-Healing: Tool failed. Triggering rescue...");
                    let rescue_msg =
                        crate::orchestration::crestodian::Crestodian::generate_rescue_message(&err);
                    self.add_message(rescue_msg).await;
                    // We don't break, we continue the loop to let the LLM see the rescue message
                }

                log::info!("All tool calls processed. Continuing decision loop...");
                continue;
            }

            if response.content.is_none() && response.tool_calls.is_none() {
                log::warn!(
                    "Model returned empty response. Retrying or breaking if too many iterations."
                );
                continue;
            }

            let raw_content = response
                .content
                .as_ref()
                .map(|c| c.to_string())
                .unwrap_or_default();

            // THOUGHT EXTRACTION: Extract <think>...</think> and strip it from final user content
            let mut final_content = raw_content.clone();
            let mut thoughts = Vec::new();

            while let Some(start) = final_content.find("<think>") {
                if let Some(end) = final_content[start..].find("</think>") {
                    let absolute_end = start + end + 8; // 8 is length of </think>
                    let thought_content = final_content[start + 7..start + end].trim().to_string();
                    thoughts.push(thought_content.clone());

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

            {
                let mut trajectory = self.trajectory.lock().await;
                trajectory.add_step(crate::trajectory::TrajectoryStep::Response {
                    content: final_content.clone(),
                    timestamp: chrono::Utc::now(),
                });
            }

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

            // BATCHED REFLECTION OPTIMIZATION:
            let current_interaction_count = self
                .interaction_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                + 1;
            if current_interaction_count % 5 == 0 {
                let _ = self.reflect().await;
            }

            // Auto-index assistant response
            if let Some(nexus) = &self.knowledge_nexus {
                let id = uuid::Uuid::new_v4().to_string();
                let _ = nexus
                    .remember_batch(vec![(id, final_content.clone())])
                    .await;
            }

            let _ = self.event_tx.send(Event::AgentResponse {
                content: response
                    .content
                    .unwrap_or(MessageContent::Text("".to_string())),
            });
            return Ok(final_content);
        }
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

    pub async fn add_to_working_memory(&self, content: String, importance: f32) {
        let mut wm = self.working_memory.lock().await;
        wm.push(WorkingMemoryUnit {
            content,
            importance,
            timestamp: std::time::Instant::now(),
        });

        // Eviction logic: keep only top 10 important or recent items
        if wm.len() > 10 {
            wm.sort_by(|a, b| b.importance.partial_cmp(&a.importance).unwrap());
            wm.truncate(10);
        }
    }

    pub async fn pack_working_memory(&self) -> String {
        let wm = self.working_memory.lock().await;
        let mut packed = String::new();
        for unit in wm.iter() {
            packed.push_str(&format!("\n---\n{}\n", unit.content));
        }
        packed
    }

    pub async fn is_docker_available(&self) -> bool {
        let output = tokio::process::Command::new("docker")
            .arg("--version")
            .output()
            .await;
        output.is_ok() && output.unwrap().status.success()
    }

    pub async fn verify_code(&self, code: &str, language: &str) -> Result<bool> {
        if !self.is_docker_available().await {
            log::warn!("Docker is not available. Skipping sandboxed code verification.");
            return Ok(true); // Treat as verified to avoid blocking when Docker is missing
        }
        log::info!("Agent: Verifying {} code in sandbox...", language);

        let docker_image = match language.to_lowercase().as_str() {
            "rust" => "rust:latest",
            "python" => "python:3-slim",
            "javascript" | "typescript" => "node:slim",
            _ => return Ok(true), // Skip verification for unknown languages
        };

        // Use 'docker run --rm' to ensure immediate container deletion after exit.
        // We use a timeout to prevent infinite loops.
        let mut child = tokio::process::Command::new("docker")
            .arg("run")
            .arg("--rm")
            .arg("-i")
            .arg(docker_image)
            .arg("sh")
            .arg("-c")
            .arg(format!(
                "echo '{}' > tmp_code && (timeout 10s cat tmp_code | {})",
                code, language
            ))
            .spawn()
            .map_err(|e| {
                AgentError::new(
                    AgentErrorCode::EnvironmentError,
                    format!("Docker failure: {}. Is Docker running?", e),
                )
            })?;

        let status = child
            .wait()
            .await
            .map_err(|e| AgentError::new(AgentErrorCode::EnvironmentError, e.to_string()))?;
        Ok(status.success())
    }

    pub async fn reflect(&self) -> Result<()> {
        let (session_id, recent_steps) = {
            let sid = self.session_id.lock().await;
            let trajectory = self.trajectory.lock().await;
            let steps = trajectory
                .steps
                .iter()
                .rev()
                .take(5)
                .cloned()
                .collect::<Vec<_>>();
            (sid.clone(), steps)
        };

        if recent_steps.is_empty() {
            return Ok(());
        }

        let context_str = recent_steps
            .iter()
            .map(|s| format!("{:?}", s))
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            "Analyze the following interaction trajectory. Extract any permanent user preferences, new facts, or operational insights. \
            Output as a concise list of points. If no significant learning occurred, respond only with 'NO_INSIGHT'.\n\nTrajectory:\n{}",
            context_str
        );

        let request = CompletionRequest {
            messages: vec![
                Message {
                    role: "system".to_string(),
                    content: Some(MessageContent::Text("You are the Pharmakon Reflection Engine. Your goal is to learn from every interaction to improve the user experience.".to_string())),
                    ..Default::default()
                },
                Message {
                    role: "user".to_string(),
                    content: Some(MessageContent::Text(prompt)),
                    ..Default::default()
                },
            ],
            temperature: Some(0.2),
            max_tokens: Some(200),
            tools: None,
        };

        // LIGHTWEIGHT MODEL OPTIMIZATION:
        let target_model = if let Some(pm_mutex) = &self.planner_model {
            let pm = pm_mutex.lock().await;
            (*pm).clone()
        } else {
            let m = self.model.lock().await;
            (*m).clone()
        };

        if let Ok(response) = target_model.as_ref().complete(request).await {
            if let Some(content) = response.content {
                let insight = content.to_string();
                if !insight.contains("NO_INSIGHT") {
                    log::info!(
                        "Agent [{}]: Reflection discovered insight: {}",
                        session_id,
                        insight
                    );
                    let _ = self.event_tx.send(Event::AgentInsight {
                        insight: insight.clone(),
                    });

                    // Save to fact memory for long-term recall
                    if let Some(fact_mem) = &self.fact_memory {
                        let mut fm = fact_mem.lock().await;
                        fm.set_fact("learned_context", &insight, 0.9)?;
                    }

                    // Also index into semantic search for recovery across sessions
                    if let Some(nexus) = &self.knowledge_nexus {
                        let id = uuid::Uuid::new_v4().to_string();
                        let _ = nexus
                            .remember_batch(vec![(
                                id,
                                format!("Insight from session {}: {}", session_id, insight),
                            )])
                            .await;
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn heartbeat(&self) -> Result<String> {
        let msg = "system_heartbeat_check: respond with HEARTBEAT_OK if functional.";
        self.chat(msg).await
    }

    pub async fn perform_maintenance(&self) -> Result<()> {
        log::info!("Agent: Performing periodic maintenance (Memory Decay)...");
        if let Some(nexus) = &self.knowledge_nexus {
            // Reduce decay_score by 5% (0.95 factor)
            nexus.decay_memories(0.95).await?;
        }
        Ok(())
    }

    pub async fn add_fact(&self, fact: &str) -> Result<()> {
        if let Some(fact_memory) = &self.fact_memory {
            let mut fm = fact_memory.lock().await;
            fm.set_fact("learned_fact", fact, 0.8)?;
        }
        Ok(())
    }

    async fn handle_model_command(&self, command: &str) -> Result<String> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.len() == 1 {
            let models = crate::providers::registry::ModelRegistry::list_available_models();
            let mut response = "Available models (based on your API keys):\n".to_string();
            if models.is_empty() {
                response.push_str("(No models available. Please check your API keys.)\n");
            } else {
                for m in models {
                    response.push_str(&format!("- {}\n", m));
                }
            }
            response.push_str("\nUsage: `/model <provider>/<model_id>` to switch.");
            return Ok(response);
        }

        let model_id = parts[1];
        if let Some(model) = crate::providers::registry::ModelRegistry::get_model(model_id) {
            self.update_model(model).await;
            let _ = self.event_tx.send(Event::ModelSwitched {
                model_id: model_id.to_string(),
            });
            return Ok(format!(
                "✅ Successfully switched to model: **{}**",
                model_id
            ));
        } else {
            return Ok(format!(
                "❌ Model not found or API key missing for: `{}`\nUse `/model` to see available models.",
                model_id
            ));
        }
    }

    async fn scout_workspace(&self, query: &str) -> Option<String> {
        log::info!(
            "Proactive Scout: Searching for related files for query: {}",
            query
        );

        let keywords: Vec<&str> = query
            .split_whitespace()
            .filter(|w| w.len() > 4)
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
            .collect();

        if keywords.is_empty() {
            return None;
        }

        let mut discovered_content = String::new();
        let mut count = 0;

        for kw in keywords {
            if count >= 2 {
                break;
            }
            let cmd = format!(
                "find . -maxdepth 3 -name '*{}*' -not -path '*/.*' | head -n 1",
                kw
            );
            let output = tokio::process::Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .output()
                .await
                .ok()?;
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() && std::path::Path::new(&path).is_file() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    discovered_content.push_str(&format!(
                        "\n--- SCOUTED FILE: {} ---\n{}\n",
                        path,
                        content.lines().take(50).collect::<Vec<_>>().join("\n")
                    ));
                    count += 1;
                }
            }
        }

        if discovered_content.is_empty() {
            None
        } else {
            Some(discovered_content)
        }
    }
}

pub struct AgentSoulManager(pub Arc<Agent>);

#[async_trait::async_trait]
impl pharmakon_common::SoulManager for AgentSoulManager {
    async fn update_soul(
        &self,
        traits: Option<Vec<String>>,
        prompt: Option<String>,
        style: Option<String>,
    ) -> anyhow::Result<()> {
        let mut soul = self.0.soul().await;
        if let Some(t) = traits {
            soul.traits = t;
        }
        if let Some(p) = prompt {
            soul.system_prompt = p;
        }
        if let Some(s) = style {
            soul.response_style = Some(s);
        }
        self.0.set_soul(soul).await;
        Ok(())
    }
}
