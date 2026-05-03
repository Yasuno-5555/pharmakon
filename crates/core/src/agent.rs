use crate::model::{Message, AgentModel, CompletionRequest, ToolDefinition, FunctionDefinition, MessageContent, CompletionResponse};
use crate::persistence::DbSessionStore;
use crate::soul::Soul;
use pharmakon_common::Event;
use pharmakon_tools::Tool;
use anyhow::{Result, anyhow};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

pub struct Agent {
    pub model: Arc<dyn AgentModel>,
    pub history: Vec<Message>,
    pub tools: Vec<Arc<dyn Tool>>,
    pub session_store: Option<Arc<DbSessionStore>>,
    pub session_id: String,
    pub prompt_manager: crate::system_prompt::SystemPromptManager,
    pub event_tx: broadcast::Sender<Event>,
    pub approval_tx: tokio::sync::mpsc::Sender<(String, bool)>,
    pub approval_rx: Option<Arc<Mutex<tokio::sync::mpsc::Receiver<(String, bool)>>>>,
    pub trajectory: crate::trajectory::Trajectory,
    pub context_engine: crate::memory::context_engine::ContextEngine,
    pub compactor: crate::memory::compactor::ContextCompactor,
    pub hooks: Arc<crate::hooks::HookRegistry>,
    pub policy_engine: crate::security::policy::PolicyEngine,
    pub semantic_search: Option<Arc<pharmakon_memory::semantic_search::SemanticSearch>>,
    pub memory_weaver: Option<Arc<pharmakon_memory::weaver::MemoryWeaver>>,
    pub fact_memory: Arc<Mutex<pharmakon_memory::FactMemory>>,
    pub health_monitor: Arc<crate::orchestration::health_monitor::HealthMonitor>,
}

impl Agent {
    pub fn new(model: Arc<dyn AgentModel>, session_id: String) -> Self {
        let (event_tx, _) = broadcast::channel(100);
        let (approval_tx, approval_rx) = tokio::sync::mpsc::channel(10);
        let trajectory = crate::trajectory::Trajectory::new(session_id.clone(), model.name().to_string());
        let context_engine = crate::memory::context_engine::ContextEngine::new(8192);
        let compactor = crate::memory::compactor::ContextCompactor::new(model.clone());
        let hooks = Arc::new(crate::hooks::HookRegistry::new());
        let prompt_manager = crate::system_prompt::SystemPromptManager::new(Soul::default_soul());
        let policy_engine = crate::security::policy::PolicyEngine::new();
        Self {
            model,
            history: Vec::new(),
            tools: Vec::new(),
            session_store: None,
            session_id,
            prompt_manager,
            event_tx,
            approval_tx,
            approval_rx: Some(Arc::new(Mutex::new(approval_rx))),
            trajectory,
            context_engine,
            compactor,
            hooks,
            policy_engine,
            semantic_search: None,
            memory_weaver: None,
            fact_memory: Arc::new(Mutex::new(pharmakon_memory::FactMemory::new().unwrap())),
            health_monitor: Arc::new(crate::orchestration::health_monitor::HealthMonitor::new(0.5)),
        }
    }

    pub fn with_memory_weaver(mut self, weaver: Arc<pharmakon_memory::weaver::MemoryWeaver>) -> Self {
        self.memory_weaver = Some(weaver);
        self
    }

    pub fn with_semantic_search(mut self, search: Arc<pharmakon_memory::semantic_search::SemanticSearch>) -> Self {
        self.semantic_search = Some(search);
        self
    }

    pub fn with_store(mut self, store: Arc<DbSessionStore>) -> Self {
        self.session_store = Some(store);
        self
    }

    pub fn with_soul(&mut self, soul: Soul) -> &mut Self {
        self.prompt_manager = crate::system_prompt::SystemPromptManager::new(soul);
        self
    }

    pub fn add_tool(&mut self, tool: Arc<dyn Tool>) {
        self.remove_tool_by_name(tool.name());
        self.tools.push(tool);
    }

    pub fn remove_tool_by_name(&mut self, name: &str) {
        self.tools.retain(|t| t.name() != name);
    }

    pub async fn heartbeat(&mut self) -> Result<String> {
        // Build a sophisticated autonomous prompt inspired by OpenClaw's HEARTBEAT.md
        let mut heartbeat_prompt = format!(
            "## HEARTBEAT MODE (Autonomous Patrol)\n\
             Current Time: {}\n\n\
             ### Instructions:\n\
             1. Review your current state, including active commitments and latest facts.\n\
             2. Check if there are any urgent tasks or notifications needed.\n\
             3. If action is required, use the necessary tools to perform it.\n\
             4. If no action is needed, reply ONLY with 'HEARTBEAT_OK'.\n\n\
             ### Context Status:\n",
            chrono::Utc::now().to_rfc3339()
        );

        if let Some(store) = &self.session_store {
            let commitments = store.load_commitments().await?;
            if !commitments.is_empty() {
                heartbeat_prompt.push_str("#### Active Commitments:\n");
                for c in commitments {
                    if c["status"] == "pending" || c["status"] == "in_progress" {
                        heartbeat_prompt.push_str(&format!("- {}: {}\n", c["id"], c["description"]));
                    }
                }
            }
        }

        log::info!("Agent initiated autonomous heartbeat check...");
        let response = self.chat(&heartbeat_prompt).await?;
        
        if response.contains("HEARTBEAT_OK") {
            log::info!("Heartbeat completed: No action required.");
        } else {
            log::info!("Heartbeat completed: Agent performed autonomous actions.");
        }
        
        Ok(response)
    }

    pub async fn chat(&mut self, user_message: &str) -> Result<String> {
        // If history is empty and we have a store, try to load it
        if self.history.is_empty() {
            if let Some(store) = &self.session_store {
                self.history = store.load_history(&self.session_id).await.unwrap_or_default();
            }
        }

        let user_msg = Message {
            role: "user".to_string(),
            content: Some(MessageContent::Text(user_message.to_string())),
            tool_calls: None,
            tool_call_id: None,
        };

        // Trigger hooks
        let _ = self.hooks.trigger_message_received(&user_msg).await;

        // Save user message
        if let Some(store) = &self.session_store {
            store.save_message(&self.session_id, &user_msg).await?;
        }
        self.history.push(user_msg);

        // Use ContextEngine for pruning
        let _ = self.context_engine.prune_history(&mut self.history).await;

        // Perform semantic compaction if history is long
        if self.history.len() > 20 {
            if let Ok(compacted) = self.compactor.compact(self.history.clone()).await {
                self.history = compacted;
            }
        }

        // Perform RAG if semantic search is available
        if let Some(search) = &self.semantic_search {
            self.prompt_manager.clear_contributions();

            let strategy = self.prompt_manager.soul().rag_strategy.clone()
                .unwrap_or(pharmakon_memory::RagStrategy::Hybrid { initial_top_k: 3 });

            match strategy {
                pharmakon_memory::RagStrategy::InitialContext { top_k } |
                pharmakon_memory::RagStrategy::Hybrid { initial_top_k: top_k } => {
                    if let Ok(memories) = search.search_with_limit(user_message, top_k as u64).await {
                        if !memories.is_empty() {
                            let memory_context = memories.join("\n---\n");
                            self.prompt_manager.add_contribution(Box::new(crate::system_prompt::StaticContribution::new(
                                "Long-term Memories",
                                &format!("The following are relevant snippets from past conversations:\n{}", memory_context)
                            )));
                        }
                    }
                }
                _ => {}
            }
        }

        // Perform advanced RAG if Memory Weaver is available
        if let Some(weaver) = &self.memory_weaver {
            if let Ok(memories) = weaver.search(user_message, 3).await {
                if !memories.is_empty() {
                    let memory_context = memories.join("\n---\n");
                    self.prompt_manager.add_contribution(Box::new(crate::system_prompt::StaticContribution::new(
                        "Deep Semantic Context",
                        &format!("The following are highly relevant snippets retrieved via deep local indexing:\n{}", memory_context)
                    )));
                }
            }
        }

        loop {
            // Trigger hooks
            let _ = self.hooks.trigger_agent_thinking(&self.session_id).await;

            let mut messages_to_send = Vec::new();

            // Add system prompt first
            messages_to_send.push(Message {
                role: "system".to_string(),
                content: Some(MessageContent::Text(self.prompt_manager.build())),
                tool_calls: None,
                tool_call_id: None,
            });

            // Add history
            messages_to_send.extend(self.history.clone());

            let tool_definitions = if self.tools.is_empty() {
                None
            } else {
                Some(self.tools.iter().map(|t| ToolDefinition {
                    r#type: "function".to_string(),
                    function: FunctionDefinition {
                        name: t.name().to_string(),
                        description: t.description().to_string(),
                        parameters: t.parameters(),
                    }
                }).collect())
            };

            let req_temp = self.prompt_manager.soul().temperature_override.map(|t| t as f32).or(Some(0.7f32));
            let request = CompletionRequest {
                messages: messages_to_send,
                temperature: req_temp,
                max_tokens: None,
                tools: tool_definitions,
            };

            log::debug!("Agent sending request to model: {}", self.model.name());
            let start = std::time::Instant::now();
            
            // Try streaming if we are just generating text (no tools or first step)
            let response = if request.tools.is_none() {
                let mut stream = match self.model.stream_complete(request).await {
                    Ok(s) => s,
                    Err(e) => {
                        self.health_monitor.record_failure();
                        return Err(anyhow::Error::new(e));
                    }
                };

                let mut full_content = String::new();
                use futures::StreamExt;
                while let Some(chunk_res) = stream.next().await {
                    match chunk_res {
                        Ok(chunk) => {
                            full_content.push_str(&chunk);
                            let _ = self.event_tx.send(Event::AgentResponseChunk { 
                                session_id: self.session_id.clone(),
                                chunk 
                            });
                        }
                        Err(e) => {
                            self.health_monitor.record_failure();
                            return Err(anyhow::Error::new(e));
                        }
                    }
                }
                
                self.health_monitor.record_success(start.elapsed());
                CompletionResponse {
                    content: Some(MessageContent::Text(full_content)),
                    tool_calls: None,
                    usage: None,
                }
            } else {
                match self.model.complete(request).await {
                    Ok(res) => {
                        self.health_monitor.record_success(start.elapsed());
                        res
                    }
                    Err(e) => {
                        self.health_monitor.record_failure();
                        return Err(anyhow::Error::new(e));
                    }
                }
            };

            match &response.content {
                Some(content) => {
                    let c = <pharmakon_common::MessageContent as Clone>::clone(content);
                    let event = pharmakon_common::Event::AgentThought { content: c };
                    let _ = self.event_tx.send(event);
                    self.trajectory.add_step(crate::trajectory::TrajectoryStep::Thought {
                        content: content.to_string(),
                        timestamp: chrono::Utc::now()
                    });
                }
                None => {}
            }

            let assistant_msg = Message {
                role: "assistant".to_string(),
                content: response.content.clone(),
                tool_calls: response.tool_calls.clone(),
                tool_call_id: None,
            };

            // Save assistant message
            if let Some(store) = &self.session_store {
                let s: &Arc<crate::persistence::DbSessionStore> = store;
                let _ = s.save_message(&self.session_id, &assistant_msg).await?;
                let _ = s.save_trajectory(&self.trajectory).await?;
            }

            // Store interaction in semantic search for long-term memory
            if let Some(search) = &self.semantic_search {
                if let Some(assistant_content) = &assistant_msg.content {
                    let _ = search.store_interaction(user_message, &assistant_content.to_string()).await;
                }
            }

            // Store interaction in Memory Weaver
            if let Some(weaver) = &self.memory_weaver {
                if let Some(assistant_content) = &assistant_msg.content {
                    let combined = format!("User: {}\nAssistant: {}", user_message, assistant_content);
                    let _ = weaver.remember(&combined).await;
                }
            }

            // Log usage if available
            if let (Some(store), Some(usage)) = (&self.session_store, &response.usage) {
                let s: &Arc<crate::persistence::DbSessionStore> = store;
                let u: &pharmakon_common::Usage = usage;
                let provider_name = self.model.name().to_string();
                let _ = s.log_usage(
                    &self.session_id,
                    &provider_name,
                    &provider_name,
                    u.prompt_tokens,
                    u.completion_tokens
                ).await;
            }

            self.history.push(assistant_msg);

            if let Some(tool_calls) = response.tool_calls {
                for tool_call in tool_calls {
                    let _ = self.event_tx.send(Event::ToolCall {
                        name: tool_call.function.name.clone(),
                        args: serde_json::from_str(&tool_call.function.arguments).unwrap_or_default()
                    });
                    self.trajectory.add_step(crate::trajectory::TrajectoryStep::Action {
                        tool: tool_call.function.name.clone(),
                        args: serde_json::from_str(&tool_call.function.arguments).unwrap_or_default(),
                        timestamp: chrono::Utc::now()
                    });

                    let tool = self.tools.iter().find(|t| t.name() == tool_call.function.name)
                        .ok_or_else(|| anyhow!("Tool not found: {}", tool_call.function.name))?;

                    let args: serde_json::Value = serde_json::from_str(&tool_call.function.arguments)?;

                    // Check tool allowlist
                    if let Some(allowlist) = &self.prompt_manager.soul().tool_allowlist {
                        if !allowlist.contains(&tool.name().to_string()) {
                            let result = format!("Tool '{}' is not allowed for this soul.", tool.name());
                            let tool_result_msg = Message {
                                role: "tool".to_string(),
                                content: Some(MessageContent::Text(result)),
                                tool_calls: None,
                                tool_call_id: Some(tool_call.id.clone()),
                            };
                            self.history.push(tool_result_msg);
                            continue;
                        }
                    }

                    // Evaluate security policy
                    let policy_result = self.policy_engine.evaluate_tool_call(tool.name(), &args);
                    
                    // Determine if approval is needed (either from policy or tool itself)
                    let (needs_approval, reason) = match policy_result {
                        crate::security::policy::PolicyAction::Deny(reason) => {
                            log::warn!("Tool execution denied by policy: {}. Reason: {}", tool.name(), reason);
                            let result = format!("Execution denied by security policy: {}", reason);
                            let _ = self.event_tx.send(Event::ToolResult { result: result.clone() });
                            let tool_result_msg = Message {
                                role: "tool".to_string(),
                                content: Some(MessageContent::Text(result)),
                                tool_calls: None,
                                tool_call_id: Some(tool_call.id.clone()),
                            };
                            if let Some(store) = &self.session_store {
                                store.save_message(&self.session_id, &tool_result_msg).await?;
                            }
                            self.history.push(tool_result_msg);
                            continue;
                        }
                        crate::security::policy::PolicyAction::RequireApproval(reason) => (true, reason),
                        crate::security::policy::PolicyAction::Allow => {
                            if tool.requires_approval(&args) {
                                (true, tool.approval_description(&args))
                            } else {
                                (false, String::new())
                            }
                        }
                    };

                    if needs_approval {
                        let approval_id = uuid::Uuid::new_v4().to_string();
                        let _ = self.event_tx.send(Event::ApprovalRequest {
                            id: approval_id.clone(),
                            tool: tool.name().to_string(),
                            args: args.clone()
                        });

                        if let Some(rx) = &self.approval_rx {
                            let mut rx_lock = rx.lock().await;
                            let mut approved = false;
                            while let Some((id, result)) = rx_lock.recv().await {
                                if id == approval_id {
                                    approved = result;
                                    break;
                                }
                            }
                            if !approved {
                                log::warn!("Tool execution denied by user: {}. Reason: {}", tool.name(), reason);
                                let result = "Execution denied by user.".to_string();
                                let _ = self.event_tx.send(Event::ToolResult { result: result.clone() });
                                let tool_result_msg = Message {
                                    role: "tool".to_string(),
                                    content: Some(MessageContent::Text(result)),
                                    tool_calls: None,
                                    tool_call_id: Some(tool_call.id.clone()),
                                };
                                if let Some(store) = &self.session_store {
                                    store.save_message(&self.session_id, &tool_result_msg).await?;
                                }
                                self.history.push(tool_result_msg);
                                continue;
                            }
                        }
                    }

                    log::info!("Agent executing tool: {} with args: {}", tool.name(), args);

                    let _ = self.hooks.trigger_before_tool_call(tool.name(), &args).await;
                    let result = tool.call(args).await?;
                    let _ = self.hooks.trigger_after_tool_call(tool.name(), &result).await;

                    let _ = self.event_tx.send(Event::ToolResult { result: result.clone() });
                    self.trajectory.add_step(crate::trajectory::TrajectoryStep::Observation {
                        result: result.clone(),
                        timestamp: chrono::Utc::now()
                    });

                    let tool_result_msg = Message {
                        role: "tool".to_string(),
                        content: Some(MessageContent::Text(result)),
                        tool_calls: None,
                        tool_call_id: Some(tool_call.id),
                    };

                    if let Some(store) = &self.session_store {
                        store.save_message(&self.session_id, &tool_result_msg).await?;
                    }
                    self.history.push(tool_result_msg);
                }
                continue;
            }

            let final_content_msg = response.content.clone().ok_or_else(|| anyhow!("Model returned empty response"))?;
            let final_content = final_content_msg.to_string();
            let _ = self.event_tx.send(Event::AgentResponse { content: final_content_msg.clone() });
            self.trajectory.add_step(crate::trajectory::TrajectoryStep::Response {
                content: final_content.clone(),
                timestamp: chrono::Utc::now()
            });

            let final_msg = Message {
                role: "assistant".to_string(),
                content: Some(final_content_msg),
                tool_calls: None,
                tool_call_id: None,
            };
            let _ = self.hooks.trigger_message_sent(&final_msg).await;

            // Store interaction in long-term memory (detached spawn)
            if let Some(search) = &self.semantic_search {
                let search = search.clone();
                let user_msg = user_message.to_string();
                let assistant_msg = final_content.clone();
                tokio::spawn(async move {
                    let _ = search.store_interaction(&user_msg, &assistant_msg).await;
                });
            }

            return Ok(final_content);
        }
    }

    pub async fn add_fact(&self, fact: String) -> Result<()> {
        let mut fact_memory = self.fact_memory.lock().await;
        let key = format!("fact_{}", chrono::Utc::now().timestamp_millis());
        fact_memory.set_fact(&key, &fact, 1.0)?;
        Ok(())
    }

    pub fn reset_history(&mut self) {
        self.history.clear();
    }

    pub async fn chat_stream(&mut self, user_message: &str) -> Result<tokio::sync::mpsc::Receiver<String>> {
        let (tx, rx) = tokio::sync::mpsc::channel(100);

        if self.history.is_empty() {
            if let Some(store) = &self.session_store {
                self.history = store.load_history(&self.session_id).await.unwrap_or_default();
            }
        }

        let user_msg = Message {
            role: "user".to_string(),
            content: Some(MessageContent::Text(user_message.to_string())),
            tool_calls: None,
            tool_call_id: None,
        };

        let _ = self.hooks.trigger_message_received(&user_msg).await;

        if let Some(store) = &self.session_store {
            store.save_message(&self.session_id, &user_msg).await?;
        }
        self.history.push(user_msg);

        let _ = self.context_engine.prune_history(&mut self.history).await;

        // Perform RAG Warmup
        if let Some(search) = &self.semantic_search {
            self.prompt_manager.clear_contributions();

            let strategy = self.prompt_manager.soul().rag_strategy.clone()
                .unwrap_or(pharmakon_memory::RagStrategy::Hybrid { initial_top_k: 3 });

            match strategy {
                pharmakon_memory::RagStrategy::InitialContext { top_k } |
                pharmakon_memory::RagStrategy::Hybrid { initial_top_k: top_k } => {
                    if let Ok(memories) = search.search_with_limit(user_message, top_k as u64).await {
                        if !memories.is_empty() {
                            let memory_context = memories.join("\n---\n");
                            self.prompt_manager.add_contribution(Box::new(crate::system_prompt::StaticContribution::new(
                                "Long-term Memories",
                                &format!("The following are relevant snippets from past conversations:\n{}", memory_context)
                            )));
                        }
                    }
                }
                _ => {}
            }
        }

        let model = self.model.clone();
        let system_prompt = self.prompt_manager.build();
        let history = self.history.clone();
        let tools = self.tools.clone();
        let event_tx = self.event_tx.clone();
        let hooks = self.hooks.clone();
        let session_id = self.session_id.clone();
        let semantic_search = self.semantic_search.clone();
        let user_message_owned = user_message.to_string();
        let temperature = self.prompt_manager.soul().temperature_override.map(|t| t as f32).unwrap_or(0.7f32);

        tokio::spawn(async move {
            let current_history = history;
            loop {
                let _ = hooks.trigger_agent_thinking(&session_id).await;

                let mut messages = vec![Message {
                    role: "system".to_string(),
                    content: Some(MessageContent::Text(system_prompt.clone())),
                    tool_calls: None,
                    tool_call_id: None,
                }];
                messages.extend(current_history.clone());

                let tool_defs = if tools.is_empty() { None } else {
                    Some(tools.iter().map(|t| ToolDefinition {
                        r#type: "function".to_string(),
                        function: FunctionDefinition {
                            name: t.name().to_string(),
                            description: t.description().to_string(),
                            parameters: t.parameters(),
                        }
                    }).collect::<Vec<_>>())
                };

                let stream_res = model.stream_complete(CompletionRequest {
                    messages,
                    temperature: Some(temperature),
                    max_tokens: None,
                    tools: tool_defs,
                }).await;

                match stream_res {
                    Ok(mut stream) => {
                        let mut full_content = String::new();
                        while let Some(chunk_res) = futures::StreamExt::next(&mut stream).await {
                            if let Ok(chunk) = chunk_res {
                                full_content.push_str(&chunk);
                                let _ = tx.send(chunk).await;
                                let _ = event_tx.send(Event::AgentResponse { content: MessageContent::Text(full_content.clone()) });
                            }
                        }

                        let final_msg = Message {
                            role: "assistant".to_string(),
                            content: Some(MessageContent::Text(full_content.clone())),
                            tool_calls: None,
                            tool_call_id: None,
                        };
                        let _ = hooks.trigger_message_sent(&final_msg).await;

                        // Store interaction in long-term memory (detached spawn)
                        if let Some(search) = &semantic_search {
                            let search = search.clone();
                            let user_msg = user_message_owned.clone();
                            let assistant_msg = full_content.clone();
                            tokio::spawn(async move {
                                let _ = search.store_interaction(&user_msg, &assistant_msg).await;
                            });
                        }

                        break;
                    }
                    Err(e) => {
                        let _ = tx.send(format!("Error: {}", e)).await;
                        break;
                    }
                }
            }
        });

        Ok(rx)
    }
}

pub struct AgentSoulManager(pub Arc<Mutex<Agent>>);

#[async_trait::async_trait]
impl pharmakon_common::SoulManager for AgentSoulManager {
    async fn update_soul(&self, traits: Option<Vec<String>>, prompt: Option<String>, style: Option<String>) -> anyhow::Result<()> {
        let mut agent = self.0.lock().await;
        let mut soul = agent.prompt_manager.soul().clone();
        if let Some(t) = traits { soul.traits = t; }
        if let Some(p) = prompt { soul.system_prompt = p; }
        if let Some(s) = style { soul.response_style = Some(s); }
        agent.with_soul(soul);
        Ok(())
    }
}
