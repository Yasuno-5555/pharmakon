use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use crate::model::{AgentModel, CompletionRequest, CompletionResponse, Message, MessageContent, ToolDefinition, AgentError};
use crate::system_prompt::SystemPromptManager;
use anyhow::{Result, anyhow};
use pharmakon_common::Event;

pub struct Agent {
    pub model: Arc<dyn AgentModel>,
    pub session_id: String,
    pub prompt_manager: crate::system_prompt::SystemPromptManager,
    pub event_tx: broadcast::Sender<Event>,
    pub approval_tx: broadcast::Sender<(String, bool)>,
    pub approval_rx: Option<broadcast::Receiver<(String, bool)>>,
    pub trajectory: crate::trajectory::Trajectory,
    pub context_engine: crate::memory::context_engine::ContextEngine,
    pub compactor: crate::memory::compactor::ContextCompactor,
    pub history: Vec<Message>,
    pub tools: Vec<Arc<dyn pharmakon_common::Tool>>,
    pub hooks: Arc<crate::hooks::HookRegistry>,
    pub fact_memory: Option<Arc<Mutex<crate::memory::FactMemory>>>,
    pub semantic_search: Option<Arc<pharmakon_memory::semantic_search::SemanticSearch>>,
    pub memory_weaver: Option<Arc<pharmakon_memory::weaver::MemoryWeaver>>,
    pub health_monitor: crate::orchestration::health_monitor::HealthMonitor,
    pub policy_engine: Arc<crate::security::policy::PolicyEngine>,
    pub session_store: Option<Arc<crate::persistence::DbSessionStore>>,
    pub planner_model: Option<Arc<dyn AgentModel>>,
    pub vision_stream: Option<Arc<Mutex<pharmakon_tools::media::vision_stream::VisionRingBuffer>>>,
    pub graph_store: Option<Arc<crate::memory::graph::GraphStore>>,
}

impl Agent {
    pub fn new(model: Arc<dyn AgentModel>, session_id: String) -> Self {
        let (event_tx, _) = broadcast::channel(100);
        let (approval_tx, approval_rx) = broadcast::channel(10);
        let trajectory = crate::trajectory::Trajectory::new(session_id.clone(), model.name().to_string());
        let context_engine = crate::memory::context_engine::ContextEngine::new(8192);
        let compactor = crate::memory::compactor::ContextCompactor::new(model.clone());
        let prompt_manager = SystemPromptManager::new(crate::soul::Soul::default_soul());

        Self {
            model,
            session_id,
            prompt_manager,
            event_tx,
            approval_tx,
            approval_rx: Some(approval_rx),
            trajectory,
            context_engine,
            compactor,
            history: Vec::new(),
            tools: Vec::new(),
            hooks: Arc::new(crate::hooks::HookRegistry::new()),
            fact_memory: None,
            semantic_search: None,
            memory_weaver: None,
            health_monitor: crate::orchestration::health_monitor::HealthMonitor::new(0.5),
            policy_engine: Arc::new(crate::security::policy::PolicyEngine::new()),
            session_store: None,
            planner_model: None,
            vision_stream: None,
            graph_store: None,
        }
    }

    pub fn with_store(mut self, store: Arc<crate::persistence::DbSessionStore>) -> Self {
        self.session_store = Some(store);
        self
    }

    pub fn with_memory_weaver(mut self, weaver: Arc<pharmakon_memory::weaver::MemoryWeaver>) -> Self {
        self.memory_weaver = Some(weaver);
        self
    }

    pub fn with_semantic_search(mut self, search: Arc<pharmakon_memory::semantic_search::SemanticSearch>) -> Self {
        self.semantic_search = Some(search);
        self
    }

    pub fn with_soul(mut self, soul: crate::soul::Soul) -> Self {
        self.prompt_manager = SystemPromptManager::new(soul);
        self
    }

    pub fn set_soul(&mut self, soul: crate::soul::Soul) {
        self.prompt_manager = SystemPromptManager::new(soul);
    }

    pub fn add_tool(&mut self, tool: Arc<dyn pharmakon_common::Tool>) {
        self.tools.push(tool);
    }

    pub fn reset_history(&mut self) {
        self.history.clear();
    }

    pub async fn chat(&mut self, user_message: &str) -> Result<String> {
        if self.history.is_empty() {
            if let Some(store) = &self.session_store {
                self.history = store.load_history(&self.session_id).await.unwrap_or_default();
            }
        }

        let user_msg = Message {
            role: "user".to_string(),
            content: Some(MessageContent::Text(user_message.to_string())),
            ..Default::default()
        };

        let _ = self.hooks.trigger_message_received(&user_msg).await;

        if let Some(store) = &self.session_store {
            store.save_message(&self.session_id, &user_msg).await?;
        }
        self.history.push(user_msg);

        // Auto-index user message for future recovery (only if meaningful)
        if user_message.len() > 10 {
            if let Some(weaver) = &self.memory_weaver {
                let _ = weaver.remember(user_message).await;
            }
        }

        let _ = self.context_engine.prune_history(&mut self.history).await;

        if self.history.len() > 20 {
            if let Ok(compacted) = self.compactor.compact(self.history.clone()).await {
                self.history = compacted;
            }
        }

        // Parallel context gathering
        let semantic_search = self.semantic_search.clone();
        let memory_weaver = self.memory_weaver.clone();
        let user_msg_text = user_message.to_string();
        
        let (semantic_res, weaver_res) = tokio::join!(
            async {
                if let Some(search) = semantic_search {
                    search.search_with_limit(&user_msg_text, 3).await.ok()
                } else { None }
            },
            async {
                if let Some(weaver) = memory_weaver {
                    weaver.search(&user_msg_text, 5).await.ok()
                } else { None }
            }
        );

        if let Some(memories) = semantic_res {
            if !memories.is_empty() {
                let memory_context = memories.join("\n---\n");
                self.prompt_manager.add_contribution(Box::new(crate::system_prompt::StaticContribution::new(
                    "Long-term Memories",
                    &format!("Relevant snippets from past conversations:\n{}", memory_context)
                )));
            }
        }

        if let Some(memories) = weaver_res {
            if !memories.is_empty() {
                let memory_context = memories.join("\n---\n");
                // Trigger context recovered hook
                let _ = self.hooks.trigger_context_recovered(&memory_context).await;
                
                self.prompt_manager.add_contribution(Box::new(crate::system_prompt::StaticContribution::new(
                    "Recovered Context (Past Sessions)",
                    &format!("Insights recovered from all sessions:\n{}", memory_context)
                )));
            }
        }

        loop {
            let mut messages_to_send = Vec::new();
            messages_to_send.push(Message {
                role: "system".to_string(),
                content: Some(MessageContent::Text(self.prompt_manager.build())),
                ..Default::default()
            });
            messages_to_send.extend(self.history.clone());

            let tool_definitions = if self.tools.is_empty() {
                None
            } else {
                Some(self.tools.iter().map(|t| {
                    ToolDefinition {
                        r#type: "function".to_string(),
                        function: crate::model::FunctionDefinition {
                            name: t.name().to_string(),
                            description: t.description().to_string(),
                            parameters: t.parameters(),
                        }
                    }
                }).collect())
            };

            // Tiered Reasoning: Use planner model for tool selection if available
            let target_model = if tool_definitions.is_some() && self.planner_model.is_some() {
                self.planner_model.as_ref().unwrap()
            } else {
                &self.model
            };

            let req_temp = self.prompt_manager.soul().temperature_override.map(|t| t as f32).or(Some(0.7f32));
            let request = CompletionRequest {
                messages: messages_to_send,
                temperature: req_temp,
                max_tokens: None,
                tools: tool_definitions,
            };

            log::debug!("Agent sending request to model: {}", target_model.name());
            let start = std::time::Instant::now();
            let response = if self.tools.is_empty() {
                let mut stream = target_model.stream_complete(request).await?;
                let mut full_content = String::new();
                use futures::StreamExt;
                while let Some(chunk_result) = stream.next().await {
                    let chunk = chunk_result?;
                    full_content.push_str(&chunk);
                    let _ = self.event_tx.send(pharmakon_common::Event::AgentThoughtChunk { 
                        session_id: self.session_id.clone(), 
                        chunk 
                    });
                }
                CompletionResponse {
                    content: Some(pharmakon_common::MessageContent::Text(full_content)),
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

            if let Some(content) = &response.content {
                let c = content.clone();
                let event = Event::AgentThought { content: c };
                let _ = self.event_tx.send(event);
                self.trajectory.add_step(crate::trajectory::TrajectoryStep::Thought {
                    content: content.to_string(),
                    timestamp: chrono::Utc::now()
                });
            }

            let assistant_msg = Message {
                role: "assistant".to_string(),
                content: response.content.clone(),
                tool_calls: response.tool_calls.clone(),
                ..Default::default()
            };

            if let Some(store) = &self.session_store {
                store.save_message(&self.session_id, &assistant_msg).await?;
                store.save_trajectory(&self.trajectory).await?;
            }
            self.history.push(assistant_msg);

            if let Some(tool_calls) = response.tool_calls {
                let mut tool_tasks = Vec::new();
                for tool_call in tool_calls {
                    let tool = self.tools.iter().find(|t| t.name() == tool_call.function.name).cloned();
                    let event_tx = self.event_tx.clone();
                    let mut approval_rx = self.approval_rx.as_ref().map(|rx| rx.resubscribe());
                    let hooks = self.hooks.clone();
                    let soul = self.prompt_manager.soul().clone();
                    let policy_engine = self.policy_engine.clone();

                    tool_tasks.push(tokio::spawn(async move {
                        let tool = match tool {
                            Some(t) => t,
                            None => return (tool_call.id.clone(), Err(anyhow!("Tool not found: {}", tool_call.function.name))),
                        };

                        let args: serde_json::Value = serde_json::from_str(&tool_call.function.arguments).unwrap_or_default();

                        let _ = event_tx.send(Event::ToolCall {
                            name: tool.name().to_string(),
                            args: args.clone()
                        });

                        if let Some(allowlist) = &soul.tool_allowlist {
                            if !allowlist.contains(&tool.name().to_string()) {
                                return (tool_call.id.clone(), Ok(format!("Tool '{}' is not allowed.", tool.name())));
                            }
                        }

                        let policy_result = policy_engine.evaluate_tool_call(tool.name(), &args);
                        let (needs_approval, _) = match policy_result {
                            crate::security::policy::PolicyAction::Deny(reason) => return (tool_call.id.clone(), Ok(format!("Denied by policy: {}", reason))),
                            crate::security::policy::PolicyAction::RequireApproval(reason) => (true, reason),
                            crate::security::policy::PolicyAction::Allow => (tool.requires_approval(&args), tool.approval_description(&args)),
                        };

                        if needs_approval {
                            let approval_id = uuid::Uuid::new_v4().to_string();
                            let _ = event_tx.send(Event::ApprovalRequest {
                                id: approval_id.clone(),
                                tool: tool.name().to_string(),
                                args: args.clone()
                            });
                            if let Some(ref mut rx) = approval_rx {
                                let mut approved = false;
                                while let Ok((id, result)) = rx.recv().await {
                                    if id == approval_id { approved = result; break; }
                                }
                                if !approved { return (tool_call.id.clone(), Ok("Denied by user.".to_string())); }
                            }
                        }

                        let _ = hooks.trigger_before_tool_call(tool.name(), &args).await;
                        let result = tool.call(args).await;
                        let result_str = match &result { Ok(s) => s.clone(), Err(e) => e.to_string() };
                        let _ = hooks.trigger_after_tool_call(tool.name(), &result_str).await;
                        (tool_call.id.clone(), result.map_err(|e| anyhow!(e.0)))
                    }));
                }

                let task_results = futures::future::join_all(tool_tasks).await;
                for task_res in task_results {
                    if let Ok((tool_call_id, result_res)) = task_res {
                        let result = match result_res { Ok(r) => r, Err(e) => e.to_string() };
                        let _ = self.event_tx.send(Event::ToolResult { result: result.clone() });
                        self.trajectory.add_step(crate::trajectory::TrajectoryStep::Observation {
                            result: result.clone(),
                            timestamp: chrono::Utc::now()
                        });
                        let tool_result_msg = Message {
                            role: "tool".to_string(),
                            content: Some(MessageContent::Text(result)),
                            tool_call_id: Some(tool_call_id),
                            ..Default::default()
                        };
                        if let Some(store) = &self.session_store {
                            let _ = store.save_message(&self.session_id, &tool_result_msg).await;
                        }
                        self.history.push(tool_result_msg);
                    }
                }
                continue;
            }

            let final_content = response.content.as_ref().map(|c| c.to_string()).unwrap_or_default();
            self.trajectory.add_step(crate::trajectory::TrajectoryStep::Response {
                content: final_content.clone(),
                timestamp: chrono::Utc::now()
            });

            let final_msg = Message {
                role: "assistant".to_string(),
                content: response.content.clone(),
                ..Default::default()
            };
            let _ = self.hooks.trigger_message_sent(&final_msg).await;

            // Trigger reflection asynchronously
            let _ = self.reflect().await;

            // Auto-index assistant response
            if let Some(weaver) = &self.memory_weaver {
                let _ = weaver.remember(&final_content).await;
            }

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
            .arg(format!("echo '{}' > tmp_code && (timeout 10s cat tmp_code | {})", code, language))
            .spawn()
            .map_err(|e| AgentError(format!("Docker failure: {}. Is Docker running?", e)))?;

        let status = child.wait().await.map_err(|e| AgentError(e.to_string()))?;
        Ok(status.success())
    }

    pub async fn reflect(&mut self) -> Result<()> {
        log::info!("Agent [{}]: Initiating post-interaction reflection...", self.session_id);
        
        // Extract recent trajectory context
        let recent_steps: Vec<_> = self.trajectory.steps.iter()
            .rev()
            .take(5)
            .cloned()
            .collect();
            
        if recent_steps.is_empty() {
            return Ok(());
        }

        let context_str = recent_steps.iter()
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

        if let Ok(response) = self.model.complete(request).await {
            if let Some(content) = response.content {
                let insight = content.to_string();
                if !insight.contains("NO_INSIGHT") {
                    log::info!("Agent [{}]: Reflection discovered insight: {}", self.session_id, insight);
                    let _ = self.event_tx.send(Event::AgentInsight { insight: insight.clone() });
                    
                    // Save to fact memory for long-term recall
                    if let Some(fact_mem) = &self.fact_memory {
                        let mut fm = fact_mem.lock().await;
                        fm.set_fact("learned_context", &insight, 0.9)?;
                    }

                    // Also index into semantic search for recovery across sessions
                    if let Some(weaver) = &self.memory_weaver {
                        let _ = weaver.remember(&format!("Insight from session {}: {}", self.session_id, insight)).await;
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn heartbeat(&mut self) -> Result<String> {
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
        agent.set_soul(soul);
        Ok(())
    }
}
