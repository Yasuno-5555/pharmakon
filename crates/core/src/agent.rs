use crate::model::{
    AgentError, AgentModel, CompletionRequest, CompletionResponse, Message, MessageContent,
    ToolDefinition,
};
use crate::system_prompt::SystemPromptManager;
use anyhow::{Result, anyhow};
use pharmakon_common::{Event, ToolRegistry};
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast};

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
    pub memory_weaver: Option<Arc<pharmakon_memory::weaver::MemoryWeaver>>,
    pub health_monitor: crate::orchestration::health_monitor::HealthMonitor,
    pub policy_engine: Arc<crate::security::policy::PolicyEngine>,
    pub session_store: Option<Arc<crate::persistence::DbSessionStore>>,
    pub planner_model: Option<Arc<Mutex<Arc<dyn AgentModel>>>>,
    pub vision_stream: Option<Arc<Mutex<pharmakon_tools::media::vision_stream::VisionRingBuffer>>>,
    pub graph_store: Option<Arc<crate::memory::graph::GraphStore>>,
    pub interaction_count: std::sync::atomic::AtomicU32,
    pub fallback_models: Vec<String>,
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
            memory_weaver: None,
            health_monitor: crate::orchestration::health_monitor::HealthMonitor::new(0.5),
            policy_engine: Arc::new(crate::security::policy::PolicyEngine::new()),
            session_store: None,
            planner_model: None,
            vision_stream: None,
            graph_store: None,
            interaction_count: std::sync::atomic::AtomicU32::new(0),
            fallback_models: Vec::new(),
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

    pub fn with_memory_weaver(
        mut self,
        weaver: Arc<pharmakon_memory::weaver::MemoryWeaver>,
    ) -> Self {
        self.memory_weaver = Some(weaver);
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

    pub fn with_soul(self, soul: crate::soul::Soul) -> Self {
        self.set_soul(soul);
        self
    }
}

impl pharmakon_common::ToolRegistry for Agent {
    fn add_tool(&self, tool: Arc<dyn pharmakon_common::Tool>) {
        let mut tools = self.tools.blocking_lock();
        if !tools.iter().any(|t| t.name() == tool.name()) {
            tools.push(tool);
        }
    }
}

impl Agent {
    pub fn setup_autonomous_tools(self: &Arc<Self>) {
        use pharmakon_tools::{
            CameraTool, CargoQualityTool, CodeEditTool, DiagnosticTool, FileReadTool,
            FileWriteTool, FindDefinitionTool, GitCommitTool, GitDiffTool, GitStatusTool,
            GrepSearchTool, ListDirTool, MultiCodeEditTool, PythonInterpreterTool, ScreenshotTool,
            ShellTool, SkillFactoryTool, TokenEconomyControlTool, ViewFileTool,
            WorkspacePerceptionTool,
        };

        let agent_arc: Arc<Agent> = self.clone();
        let tool_registry_arc: Arc<dyn pharmakon_common::ToolRegistry> = agent_arc;
        self.add_tool(Arc::new(pharmakon_tools::SkillFactoryTool::new(
            Arc::downgrade(&tool_registry_arc),
        )));
        self.add_tool(Arc::new(WorkspacePerceptionTool));
        self.add_tool(Arc::new(GrepSearchTool));
        self.add_tool(Arc::new(CodeEditTool));
        self.add_tool(Arc::new(MultiCodeEditTool));
        self.add_tool(Arc::new(ListDirTool));
        self.add_tool(Arc::new(ViewFileTool));
        self.add_tool(Arc::new(GitStatusTool));
        self.add_tool(Arc::new(GitDiffTool));
        self.add_tool(Arc::new(GitCommitTool));
        self.add_tool(Arc::new(CargoQualityTool));
        self.add_tool(Arc::new(FindDefinitionTool));
        self.add_tool(Arc::new(ShellTool));
        self.add_tool(Arc::new(FileReadTool));
        self.add_tool(Arc::new(FileWriteTool));
        self.add_tool(Arc::new(ScreenshotTool));
        self.add_tool(Arc::new(CameraTool));
        self.add_tool(Arc::new(PythonInterpreterTool));
        self.add_tool(Arc::new(TokenEconomyControlTool {}));

        self.add_tool(Arc::new(DiagnosticTool {
            vision_stream: self.vision_stream.clone(),
            telemetry: None,
            mcp_stats_source: "agent_internal".to_string(),
        }));

        // Inject Full-Spectrum Autonomous Engineering Guidelines
        let mut pm = self.prompt_manager.blocking_lock();
        pm.add_contribution(Box::new(crate::system_prompt::StaticContribution::new(
            "Token Economy & Frugality (Energy Saving Mode)",
            "Tokens are precious. To minimize API costs and stay within limits: \
            1. **Targeted Reading**: Use `view_file` with narrow line ranges (e.g. 50-100 lines) instead of reading whole files. \
            2. **Filtered Search**: Use `grep_search` with specific queries and `include` filters to avoid massive outputs. \
            3. **Incremental Edits**: Prefer `edit_file` or `multi_edit_file` over rewriting entire files with `write_file`. \
            4. **Summarization**: When reporting results, be concise. Only include the essential output. \
            A frugal agent is a sustainable agent."
        )));

        pm.add_contribution(Box::new(crate::system_prompt::StaticContribution::new(
            "Full-Spectrum Autonomous Engineering (Aider/SWE-agent/Devin/Antigravity lineage)",
            "You are a master engineer equipped with an elite tool suite. \
            - **Version Control**: Use `git_status`, `git_diff`, and `git_commit`. Committing often is encouraged. \
            - **Quality Control**: Use `cargo_check` (check, test, fmt) for Rust-specific verification. \
            - **Navigation**: Use `find_definition`, `grep_search`, `list_dir`, and `view_file`. \
            - **Execution**: Use `shell` for one-offs and `terminal` for stateful sessions. \
            - **Scripting**: Use `python_interpreter` for data analysis or complex logic blocks. \
            - **Perception**: Use `screenshot` and `camera_capture` to see the physical/digital world. \
            - **Scalability (MCP)**: If a required capability is missing, you can autonomously search for and connect to Model Context Protocol (MCP) servers (e.g. `npx @modelcontextprotocol/server-github`). \
            Your goal is to manage the entire lifecycle: Discover, Plan, Execute, Verify, and Commit."
        )));

        pm.add_contribution(Box::new(crate::system_prompt::StaticContribution::new(
            "Self-Perception & Situational Awareness",
            "You have tools to perceive your own situation and environment. \
            If you feel lost or need to understand your current context (system resources, directory structure, active capabilities), \
            autonomously call `self_diagnostic` or `perceive_workspace`."
        )));
    }

    pub fn update_model(&self, model: Arc<dyn AgentModel>) {
        {
            let mut m = self.model.blocking_lock();
            *m = model.clone();
        }
        {
            let mut traj = self.trajectory.blocking_lock();
            traj.metadata.model = model.name().to_string();
        }
        {
            let mut compactor = self.compactor.blocking_lock();
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

    pub fn reset_history(&self) {
        let mut history = self.history.blocking_lock();
        history.clear();
    }

    pub fn add_message(&self, msg: Message) {
        let mut history = self.history.blocking_lock();
        history.push(msg);
    }

    pub fn model_name(&self) -> String {
        let m = self.model.blocking_lock();
        m.name().to_string()
    }

    pub fn add_contribution(
        &self,
        contribution: Box<dyn crate::system_prompt::SystemPromptContribution>,
    ) {
        let mut pm = self.prompt_manager.blocking_lock();
        pm.add_contribution(contribution);
    }

    pub fn set_session_id(&self, id: String) {
        let mut sid = self.session_id.blocking_lock();
        *sid = id;
    }

    pub fn replace_history(&self, new_history: Vec<Message>) {
        let mut history = self.history.blocking_lock();
        *history = new_history;
    }

    pub fn trajectory_steps(&self) -> Vec<crate::trajectory::TrajectoryStep> {
        let traj = self.trajectory.blocking_lock();
        traj.steps.clone()
    }

    pub fn soul(&self) -> crate::soul::Soul {
        let pm = self.prompt_manager.blocking_lock();
        pm.soul().clone()
    }

    pub fn set_soul(&self, soul: crate::soul::Soul) {
        let mut pm = self.prompt_manager.blocking_lock();
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
            if let Some(weaver) = &self.memory_weaver {
                let _ = weaver.remember(user_message).await;
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
        let memory_weaver = self.memory_weaver.clone();
        let user_msg_text = user_message.to_string();

        let (semantic_res, weaver_res, scout_res) = if user_message.len() < 5 {
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
                    if let Some(weaver) = memory_weaver {
                        weaver.search(&user_msg_text, 5).await.ok()
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
                let mut prompt_manager = self.prompt_manager.lock().await;
                prompt_manager.add_contribution(Box::new(
                    crate::system_prompt::StaticContribution::new(
                        "Long-term Memories",
                        &format!(
                            "Relevant snippets from past conversations:\n{}",
                            memory_context
                        ),
                    ),
                ));
            }
        }

        if let Some(memories) = weaver_res {
            if !memories.is_empty() {
                let memory_context = memories.join("\n---\n");
                // Trigger context recovered hook
                let _ = self.hooks.trigger_context_recovered(&memory_context).await;

                let mut prompt_manager = self.prompt_manager.lock().await;
                prompt_manager.add_contribution(Box::new(
                    crate::system_prompt::StaticContribution::new(
                        "Recovered Context (Past Sessions)",
                        &format!("Insights recovered from all sessions:\n{}", memory_context),
                    ),
                ));
            }
        }

        if let Some(scout_context) = scout_res {
            let mut prompt_manager = self.prompt_manager.lock().await;
            prompt_manager.add_contribution(Box::new(crate::system_prompt::StaticContribution::new(
                "Proactive Scout Insights",
                &format!("Based on your request, I proactively found and peeked into these files:\n{}", scout_context)
            )));
        }

        let tools_count = self.tools.blocking_lock().len();
        log::info!(
            "Agent entering decision loop with {} tools and session: {}",
            tools_count,
            session_id
        );

        loop {
            log::info!("Agent iteration start...");
            let mut messages_to_send = Vec::new();
            {
                let prompt_manager = self.prompt_manager.lock().await;
                messages_to_send.push(Message {
                    role: "system".to_string(),
                    content: Some(MessageContent::Text(prompt_manager.build())),
                    ..Default::default()
                });
            }
            {
                let history = self.history.lock().await;
                messages_to_send.extend(history.clone());
            }

            let tools_lock = self.tools.blocking_lock();
            let tool_definitions = if tools_lock.is_empty() {
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

            let response: pharmakon_common::agent_types::CompletionResponse = response_result.unwrap();

            let _ = self.event_tx.send(Event::InteractionFinished {
                response: response.clone(),
            });

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
                        .blocking_lock()
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
                        let result = tool.call(args).await;
                        let result_str = match &result {
                            Ok(s) => s.clone(),
                            Err(e) => e.to_string(),
                        };
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
                    self.add_message(rescue_msg);
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
            if let Some(weaver) = &self.memory_weaver {
                let _ = weaver.remember(&final_content).await;
            }

            let _ = self.event_tx.send(Event::AgentResponse {
                content: response
                    .content
                    .unwrap_or(MessageContent::Text("".to_string())),
            });
            return Ok(final_content);
        }
    }

    pub async fn verify_code(&self, code: &str, language: &str) -> Result<bool> {
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
            .map_err(|e| AgentError(format!("Docker failure: {}. Is Docker running?", e)))?;

        let status = child.wait().await.map_err(|e| AgentError(e.to_string()))?;
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
                    if let Some(weaver) = &self.memory_weaver {
                        let _ = weaver
                            .remember(&format!("Insight from session {}: {}", session_id, insight))
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
            self.update_model(model);
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
        let mut soul = self.0.soul();
        if let Some(t) = traits {
            soul.traits = t;
        }
        if let Some(p) = prompt {
            soul.system_prompt = p;
        }
        if let Some(s) = style {
            soul.response_style = Some(s);
        }
        self.0.set_soul(soul);
        Ok(())
    }
}
