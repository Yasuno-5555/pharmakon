use crate::orchestration::budget::{self, IterationSnapshot, ProgressTracker, TerminationSignal, TerminationPolicy};
use crate::orchestration::dsge_integration::AgentEconomy;
use crate::model::{
    AgentError, AgentErrorCode, AgentModel, CompletionRequest,
    Message, MessageContent, ToolDefinition, ToolCategory,
};
use crate::system_prompt::SystemPromptManager;
use anyhow::{Result, anyhow};
use pharmakon_common::Event;
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
    pub context_manager: Arc<Mutex<crate::context::ContextManager>>,
    pub active_categories: Arc<Mutex<std::collections::HashSet<crate::model::ToolCategory>>>,
    pub event_tx: broadcast::Sender<Event>,
    pub approval_tx: broadcast::Sender<(String, bool)>,
    pub trajectory: Arc<Mutex<crate::trajectory::Trajectory>>,
    pub compactor: Arc<Mutex<crate::memory::compactor::ContextCompactor>>,
    pub hooks: Arc<crate::hooks::HookRegistry>,
    pub fact_memory: Option<Arc<Mutex<crate::memory::BeliefSystem>>>,
    pub semantic_search: Option<Arc<pharmakon_memory::semantic_search::SemanticSearch>>,
    pub knowledge_nexus: Option<Arc<pharmakon_memory::weaver::KnowledgeNexus>>,
    pub health_monitor: crate::orchestration::health_monitor::HealthMonitor,
    pub policy_engine: Arc<crate::security::policy::PolicyEngine>,
    pub session_store: Option<Arc<crate::persistence::DbSessionStore>>,
    pub planner_model: Option<Arc<Mutex<Arc<dyn AgentModel>>>>,
    pub graph_store: Option<Arc<crate::memory::graph::GraphStore>>,
    pub interaction_count: Arc<std::sync::atomic::AtomicU32>,
    pub fallback_models: Arc<StdMutex<Vec<String>>>,
    pub total_tokens: Arc<std::sync::atomic::AtomicU64>,
    pub total_cost: Arc<Mutex<f64>>,
    pub start_time: std::time::Instant,
    pub dry_run: Arc<std::sync::atomic::AtomicBool>,
    pub tool_call_counts: Arc<Mutex<std::collections::HashMap<String, u32>>>,
    pub territory_manager: Arc<crate::orchestration::territory::TerritoryManager>,
    pub research_notebook: Arc<Mutex<crate::orchestration::research::ResearchNotebook>>,
    pub usage_history: Arc<Mutex<Vec<(chrono::DateTime<chrono::Utc>, u64, f64)>>>,
    pub event_log: Arc<crate::event_log::EventLog>,
    pub snapshot_store: Arc<crate::snapshot_store::SnapshotStore>,
    pub registry: Arc<Mutex<pharmakon_tools::registry::ToolMetaRegistry>>,
    pub governor: Arc<crate::orchestration::governor::ToolGovernor>,
    pub economy: Arc<std::sync::Mutex<AgentEconomy>>,
    pub bank_of_pharmakon: Arc<Mutex<crate::orchestration::economy_v2::BankOfPharmakon>>,
    pub dream_started: Arc<std::sync::atomic::AtomicBool>,
    pub cron_manager: Arc<StdMutex<Option<Arc<crate::automation::cron::CronManager>>>>,
    pub skill_library: Arc<std::sync::Mutex<crate::orchestration::skill_library::RhaiSkillLibrary>>,
    pub vision_stream: Option<Arc<tokio::sync::Mutex<pharmakon_tools::media::vision_stream::VisionRingBuffer>>>,
}

impl Clone for Agent {
    fn clone(&self) -> Self {
        Self {
            model: self.model.clone(),
            session_id: self.session_id.clone(),
            session_states: self.session_states.clone(),
            prompt_manager: self.prompt_manager.clone(),
            context_manager: self.context_manager.clone(),
            active_categories: self.active_categories.clone(),
            event_tx: self.event_tx.clone(),
            approval_tx: self.approval_tx.clone(),
            trajectory: self.trajectory.clone(),
            compactor: self.compactor.clone(),
            hooks: self.hooks.clone(),
            fact_memory: self.fact_memory.clone(),
            semantic_search: self.semantic_search.clone(),
            knowledge_nexus: self.knowledge_nexus.clone(),
            health_monitor: self.health_monitor.clone(),
            policy_engine: self.policy_engine.clone(),
            session_store: self.session_store.clone(),
            planner_model: self.planner_model.clone(),
            graph_store: self.graph_store.clone(),
            interaction_count: self.interaction_count.clone(),
            fallback_models: self.fallback_models.clone(),
            total_tokens: self.total_tokens.clone(),
            total_cost: self.total_cost.clone(),
            start_time: self.start_time,
            dry_run: self.dry_run.clone(),
            tool_call_counts: self.tool_call_counts.clone(),
            territory_manager: self.territory_manager.clone(),
            research_notebook: self.research_notebook.clone(),
            usage_history: self.usage_history.clone(),
            event_log: self.event_log.clone(),
            snapshot_store: self.snapshot_store.clone(),
            registry: self.registry.clone(),
            governor: self.governor.clone(),
            dream_started: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            cron_manager: Arc::new(StdMutex::new(None)),
            economy: Arc::new(std::sync::Mutex::new(AgentEconomy::new(0.5))),
            bank_of_pharmakon: self.bank_of_pharmakon.clone(),
            skill_library: Arc::new(std::sync::Mutex::new(crate::orchestration::skill_library::RhaiSkillLibrary::new())),
            vision_stream: self.vision_stream.clone(),
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

        let home = dirs::home_dir().unwrap_or_default();
        let context_dir = home.join(".pharmakon").join("context");
        let context_manager = Arc::new(Mutex::new(crate::context::ContextManager::new(&context_dir).unwrap_or_else(|_| crate::context::ContextManager::new(".").unwrap())));

        let mut active_categories = std::collections::HashSet::new();
        active_categories.insert(ToolCategory::Core);
        let active_categories = Arc::new(Mutex::new(active_categories));

        let mut hooks = crate::hooks::HookRegistry::new();
        hooks.register(Box::new(
            crate::hooks::token_economy::TokenEconomyHook::new(0.8, 100_000),
        )); // 100k token default budget

        let total_tokens = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let total_cost = Arc::new(Mutex::new(0.0));

        let registry = Arc::new(Mutex::new(pharmakon_tools::registry::ToolMetaRegistry::new(
            pharmakon_tools::registry::ToolDependencies {
                model: Some(model.clone()),
                store: None,
                soul_manager: None,
                event_tx: Some(event_tx.clone()),
                nexus: None,
                vision_stream: None,
                total_tokens: Some(total_tokens.clone()),
                total_cost: Some(total_cost.clone()),
            }
        )));

        Self {
            model: Arc::new(Mutex::new(model.clone())),
            session_id: Arc::new(Mutex::new(session_id)),
            session_states: Arc::new(Mutex::new(std::collections::HashMap::new())),
            prompt_manager: Arc::new(Mutex::new(pm)),
            context_manager,
            active_categories,
            event_tx,
            approval_tx,

            trajectory,
            compactor,
            hooks: Arc::new(hooks),
            fact_memory: None,
            semantic_search: None,
            knowledge_nexus: None,
            health_monitor: crate::orchestration::health_monitor::HealthMonitor::new(0.3),
            policy_engine: Arc::new(crate::security::policy::PolicyEngine::new()),
            session_store: None,
            planner_model: None,
            graph_store: None,
            interaction_count: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            fallback_models: Arc::new(StdMutex::new(Vec::new())),
            total_tokens,
            total_cost,
            start_time: std::time::Instant::now(),
            dry_run: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            tool_call_counts: Arc::new(Mutex::new(std::collections::HashMap::new())),
            territory_manager: Arc::new(crate::orchestration::territory::TerritoryManager::new()),
            research_notebook: Arc::new(Mutex::new(
                crate::orchestration::research::ResearchNotebook::new("Uninitialized"),
            )),
            usage_history: Arc::new(Mutex::new(Vec::new())),
            event_log: Arc::new(crate::event_log::EventLog::new(
                Some(home.join(".pharmakon").join("event_log").join("events.jsonl")),
            )),
            snapshot_store: Arc::new(crate::snapshot_store::SnapshotStore::new(
                home.join(".pharmakon").join("snapshots"),
            )),
            dream_started: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            cron_manager: Arc::new(StdMutex::new(None)),
            registry,
            skill_library: Arc::new(std::sync::Mutex::new(crate::orchestration::skill_library::RhaiSkillLibrary::new())),
            economy: Arc::new(std::sync::Mutex::new(AgentEconomy::new(0.5))),
            bank_of_pharmakon: Arc::new(Mutex::new(crate::orchestration::economy_v2::BankOfPharmakon::new(100_000))),
            governor: Arc::new(crate::orchestration::governor::ToolGovernor::new(Default::default())),
            vision_stream: None,
        }
    }

    pub fn with_fallback_models(self, models: Vec<String>) -> Self {
        {
            let mut fm = self.fallback_models.lock().unwrap();
            *fm = models;
        }
        self
    }

    pub fn clone_for_speculative(&self, dry_run: bool, speculative_session_id: String) -> Self {
        let mut cloned = self.clone();
        cloned.dry_run = Arc::new(std::sync::atomic::AtomicBool::new(dry_run));
        cloned.session_id = Arc::new(Mutex::new(speculative_session_id));
        cloned
    }

    pub async fn get_current_session_state(&self) -> Arc<Mutex<SessionState>> {
        let session_id = {
            let sid = self.session_id.lock().await;
            sid.clone()
        };
        // Check if task-local session ID is available, override if so
        let session_id = CURRENT_SESSION_ID.try_with(|id| {
            if id.is_empty() {
                session_id.clone()
            } else {
                id.clone()
            }
        }).unwrap_or(session_id);
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
        self.session_store = Some(store.clone());
        // Update registry deps
        {
            let mut reg = self.registry.try_lock().expect("Failed to lock registry during init");
            reg.update_deps(|deps| {
                deps.store = Some(store);
            });
        }
        self
    }

    pub fn with_fact_memory(mut self, fact_memory: Arc<Mutex<BeliefSystem>>) -> Self {
        self.fact_memory = Some(fact_memory);
        // Note: ToolDependencies doesn't currently have fact_memory, but it has nexus
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
        self.knowledge_nexus = Some(nexus.clone());
        {
            let mut reg = self.registry.try_lock().expect("Failed to lock registry during init");
            reg.update_deps(|deps| {
                deps.nexus = Some(nexus);
            });
        }
        self
    }

    pub async fn add_tool(&self, tool: Arc<dyn pharmakon_common::Tool>) {
        let mut reg = self.registry.lock().await;
        reg.add_tool(tool);
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

    pub fn set_dry_run(&self, enabled: bool) {
        self.dry_run.store(enabled, std::sync::atomic::Ordering::SeqCst);
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
                if !self.dream_started.swap(true, std::sync::atomic::Ordering::SeqCst) { let skill_lib = self.skill_library.clone(); tokio::spawn(async move { log::info!("Dream Mode started"); loop { tokio::time::sleep(std::time::Duration::from_secs(300)).await; let mut lib = skill_lib.lock().unwrap(); lib.decay(); log::debug!("Dream Mode: decay cycle complete, {} entries", lib.entries.len()); } }); }
                return self.handle_model_command(user_message).await;
            }
            if user_message.starts_with("/plan") {
                return self.handle_plan_command(user_message).await;
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
                let memory_context = memories.join("
---
");
                self.add_to_working_memory(
                    format!("Long-term Memories:
{}", memory_context),
                    0.7,
                    "SemanticSearch".to_string(),
                )
                .await;
            }

        if let Some(memories) = nexus_res
            && !memories.is_empty() {
                let memory_context = memories.join("
---
");
                let _ = self.hooks.trigger_context_recovered(&memory_context).await;
                self.add_to_working_memory(
                    format!(
                        "Knowledge Nexus Insights (Hybrid + Graph):
{}",
                        memory_context
                    ),
                    0.9,
                    "KnowledgeNexus".to_string(),
                )
                .await;
            }



        let tools_count = self.registry.lock().await.all_metadata().len();
        log::info!(
            "Agent entering decision loop with {} tools and session: {}",
            tools_count,
            session_id
        );

        //--- Budgeted Execution Model Start ---
        let model = {
            let m = self.model.lock().await;
            Some((*m).clone())
        };
        let complexity = crate::orchestration::scheduler::classify_task_complexity(
            user_message,
            model.as_ref(),
        ).await;

        let budget = budget::estimate_budget(complexity);

        let mut progress_tracker = ProgressTracker::new(&budget.policy);
        let mut current_iteration = 0;

        let start_time = std::time::Instant::now();
        //--- Budgeted Execution Model End ---

        loop {
            // --- Start of Loop: Budget and Progress Checks ---
            current_iteration += 1;
            
            // --- Entropy Check (Loop Detection) ---
            let entropy = self.event_log.recent_tool_entropy(10).await;
            self.economy.lock().unwrap().update_inflation(current_iteration as u64 * 400, 4000);
            if entropy > 0.8 {
                log::warn!("[SESSION: {}] High entropy detected ({:.2}). Possible loop.", session_id, entropy);
                self.event_log.append(session_id, crate::event_log::EventKind::EntropyAlert {
                    score: entropy,
                    pattern: "high_repetition".to_string(),
                }).await;
                
                let mut state = state_arc.lock().await;
                state.history.push(Message {
                    role: "system".to_string(),
                    content: Some(MessageContent::Text(format!(
                        "WARNING: High tool call repetition detected ({:.2}). You might be in a loop. Please reconsider your strategy and try a different approach.",
                        entropy
                    ))),
                    ..Default::default()
                });
                
                // Hard termination if entropy exceeds critical threshold
                // (integrated via ProgressTracker for unified signal handling)
                let entropy_signal = progress_tracker.check_entropy(entropy, 0.95);
                if entropy_signal != TerminationSignal::Continue {
                    let reason = format!(
                        "Entropy overflow detected ({:.2}). Agent is in a pathological loop with no progress. Task aborted.",
                        entropy
                    );
                    log::error!("CRITICAL: {}", reason);
                    self.event_log.append(session_id, crate::event_log::EventKind::SessionEvent {
                        action: "failed".to_string(),
                        detail: reason.clone(),
                    }).await;
                    return Err(anyhow!(AgentError::new(AgentErrorCode::HangDetected, reason)));
                }
            }

            if start_time.elapsed() > budget.hard_max_wall_time {
                let reason = format!(
                    "Wall time limit exceeded ({:?}). Task aborted.",
                    budget.hard_max_wall_time
                );
                log::error!("CRITICAL: {}", reason);
                return Err(anyhow!(AgentError::new(AgentErrorCode::HangDetected, reason)));
            }

            if let TerminationPolicy::FixedIterations(max_iters) = budget.policy {
                if current_iteration > max_iters {
                    log::info!(
                        "Fixed iteration limit reached ({}). Task finished.",
                        max_iters
                    );
                    let final_response = state_arc
                        .lock()
                        .await
                        .history
                        .last()
                        .and_then(|msg| {
                            if msg.role == "assistant" {
                                msg.content.as_ref().map(|c| c.to_string())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_default();
                    break Ok(final_response);
                }
            }
            
            let mut snapshot = IterationSnapshot::new();
            // --- End of Loop: Budget and Progress Checks ---

            log::info!("[SESSION: {}] Agent iteration start ({})...", session_id, current_iteration);
            
            // ... (rest of the message preparation logic remains the same)
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
                                    format!("{}...", unit.content.chars().take(100).collect::<String>())
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

                let dynamic_context = {
                    let ctx_mgr = self.context_manager.lock().await;
                    ctx_mgr.render_prompt_context()
                };

                let cap_summary = {
                    let reg = self.registry.lock().await;
                    reg.catalog.capability_summary()
                };
                // Inject skill library guidance (few-shot examples + anti-patterns)
                let skill_guidance = {
                    let lib = self.skill_library.lock().unwrap();
                    crate::orchestration::skill_library::build_codeact_system_prompt(&lib, user_message)
                };

                let layout = crate::system_prompt::PromptLayout {
                    capability_summary: cap_summary,
                    dynamic_context,
                    system_rules: format!("{}\n{}", prompt_manager.soul().system_prompt, skill_guidance),
                    playbooks: {
                        if state.active_playbooks.is_empty() {
                            "No specialized playbooks active.".to_string()
                        } else {
                            state.active_playbooks
                                .iter()
                                .map(|(name, content)| format!("#### ACTIVE PLAYBOOK: {}
{}", name, content))
                                .collect::<Vec<_>>()
                                .join("

---

")
                        }
                    },
                    repo_map: None, // Will be populated by repomap tool if needed
                    knowledge_graph: None,
                    working_memory: virtual_index,
                    current_task: user_message.to_string(),
                };

                // Use PromptLayers for cache-optimal prompt topology:
                // Layer 0 (cacheable) = system prompt + capability summary (never changes)
                // Layer 1 (semi-static) = playbook + goal
                // Layer 2 (dynamic) = conversation history
                // Layer 3 (actionable) = current task (already in history as last user msg)
                let layers = crate::context::topology::PromptLayers {
                    cacheable_prefix: layout.render(),
                    semi_static: String::new(),
                    dynamic: state.history.clone(),
                    actionable: String::new(),
                };
                messages_to_send = layers.assemble();
            { let shadow = self.economy.lock().unwrap().shadow_directive(); if !shadow.is_empty() && !messages_to_send.is_empty() { if let Some(ref mut c) = messages_to_send[0].content { let text = c.as_text().unwrap_or(""); *c = pharmakon_common::agent_types::MessageContent::Text(format!("{}\n{}", text, shadow)); } } }
            }

            let tool_definitions = {
                let mut reg = self.registry.lock().await;
                let active_cats = self.active_categories.lock().await;

                // 1. Start with core tools and any already loaded tools
                let mut tools_to_inject = reg.all_metadata()
                    .iter()
                    .filter(|m| m.category == ToolCategory::Core)
                    .cloned()
                    .collect::<Vec<_>>();

                // 2. Search for relevant tools based on the current task/query
                let search_results = reg.search(user_message, 15);
                for meta in search_results {
                    if !tools_to_inject.iter().any(|t| t.name == meta.name) {
                        tools_to_inject.push(meta);
                    }
                }

                // 3. Serendipity injection: randomly sample 3 non-core tools
                // to increase codex tool discovery (66 tools otherwise invisible to BM25)
                let non_core: Vec<_> = reg.all_metadata().iter()
                    .filter(|m| m.category != ToolCategory::Core)
                    .cloned()
                    .collect();
                if !non_core.is_empty() {
                    let n_sample = 3.min(non_core.len());
                    let sampled: Vec<_> = non_core.iter()
                        .filter(|m| !tools_to_inject.iter().any(|t| t.name == m.name))
                        .collect();
                    // Rotating sample: use interaction count as seed for variety
                    let seed = self.interaction_count.load(std::sync::atomic::Ordering::SeqCst) as usize;
                    let start = seed % sampled.len().max(1);
                    for i in 0..n_sample {
                        let idx = (start + i * 7 + i * i) % sampled.len().max(1);
                        tools_to_inject.push(sampled[idx].clone());
                    }
                }

                let active_tools: Vec<_> = tools_to_inject
                    .into_iter()
                    .filter(|m| m.category == ToolCategory::Core || active_cats.contains(&m.category))
                    .filter_map(|m| {
                        // On-demand hydration to get parameters
                        reg.hydrate(&m.name).map(|t| ToolDefinition {
                            r#type: "function".to_string(),
                            function: crate::model::FunctionDefinition {
                                name: t.name().to_string(),
                                description: t.description().to_string(),
                                parameters: t.parameters(),
                            },
                        })
                    })
                    .collect();

                if active_tools.is_empty() {
                    None
                } else {
                    Some(active_tools)
                }
            };

            let mut target_model = {
                let m = self.model.lock().await;
                let default_model = (*m).clone();
                if default_model.name() == "mock-model" || default_model.name() == "test" {
                    default_model
                } else {
                    self.economy.lock().unwrap().select_model(user_message, match complexity { budget::TaskComplexity::Simple => 0.2, budget::TaskComplexity::Standard => 0.5, budget::TaskComplexity::Deep => 0.8 }).unwrap_or(default_model)
                }
            };

            log::info!("[SESSION: {}] Sending completion request to model...", session_id);

            let mut request = CompletionRequest {
                messages: messages_to_send,
                temperature: Some(0.2),
                max_tokens: Some(self.economy.lock().unwrap().recommend_max_tokens(target_model.name())),
                tools: tool_definitions,
            };

             let mut response_result = None;
             let mut current_fallback_index = 0;
             let mut consecutive_empty_responses = 0;
             let fallback_models = self.fallback_models.clone();

             while response_result.is_none() {
                 let model_lock = target_model.clone();
                 let completion_task = async {
                     model_lock.complete(request.clone()).await
                 };

                 response_result = Some(completion_task.await);

                 match response_result {
                     Some(Ok(ref res)) => {
                         let is_max_tokens = res.finish_reason.as_ref().map(|fr| fr.to_uppercase() == "MAX_TOKENS").unwrap_or(false)
                             || res.content.as_ref().map(|c| c.to_string().contains("[Model stopped: Max tokens reached]")).unwrap_or(false);

                         if is_max_tokens {
                             let fallback_list = fallback_models.lock().unwrap();
                             if current_fallback_index < fallback_list.len() {
                                 let fallback_id = &fallback_list[current_fallback_index];
                                 log::warn!(
                                     "Output token limit reached (MAX_TOKENS) for {}. Escalating to fallback model: {}",
                                     target_model.name(),
                                     fallback_id
                                 );
                                 let _ = self.event_tx.send(Event::Error {
                                     message: format!(
                                         "Output token limit reached (MAX_TOKENS) for {}. Escalating to fallback model: {}",
                                         target_model.name(),
                                         fallback_id
                                     ),
                                 });

                                 if let Some(new_model) =
                                     crate::providers::registry::ModelRegistry::get_model(fallback_id)
                                 {
                                     target_model = new_model;
                                     current_fallback_index += 1;
                                     consecutive_empty_responses = 0;
                                     request.max_tokens = Some(self.economy.lock().unwrap().recommend_max_tokens(target_model.name()));
                                     response_result = None;
                                     continue;
                                 }
                             }
                         }
                         let is_empty = res.content.as_ref().map(|c| c.to_string().trim().is_empty()).unwrap_or(true)
                             && res.tool_calls.is_none();
                         if is_empty {
                             consecutive_empty_responses += 1;
                             log::warn!(
                                 "Empty response from model {} (consecutive empty count: {})",
                                 target_model.name(),
                                 consecutive_empty_responses
                             );
                             if consecutive_empty_responses >= 2 {
                                 let fallback_list = fallback_models.lock().unwrap();
                                 if current_fallback_index < fallback_list.len() {
                                     let fallback_id = &fallback_list[current_fallback_index];
                                     log::warn!(
                                         "Two consecutive empty responses from {}. Switching to fallback: {}",
                                         target_model.name(),
                                         fallback_id
                                     );
                                     let _ = self.event_tx.send(Event::Error {
                                         message: format!(
                                             "Two consecutive empty responses from {}. Switching to fallback: {}",
                                             target_model.name(),
                                             fallback_id
                                         ),
                                     });

                                     if let Some(new_model) =
                                         crate::providers::registry::ModelRegistry::get_model(fallback_id)
                                     {
                                         target_model = new_model;
                                         current_fallback_index += 1;
                                         consecutive_empty_responses = 0;
                                         response_result = None;
                                         continue;
                                     }
                                 }
                             } else {
                                 log::info!("Retrying same model once for empty response...");
                                 response_result = None;
                                 continue;
                             }
                         } else {
                             consecutive_empty_responses = 0;
                         }
                     }
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

                        { let mn = target_model.name().to_string(); self.economy.lock().unwrap().record_model_result(&mn, 0, false, is_rate_limit); }
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
            // Feed actual API token consumption into the DSGE economy layer
            if let Some(ref usage) = response.usage {
                let mut economy = self.economy.lock().unwrap();
                economy.record_token_usage(usage.prompt_tokens as u64, usage.completion_tokens as u64);
                self.total_tokens.fetch_add(usage.total_tokens as u64, std::sync::atomic::Ordering::SeqCst);
                // Record per-call observation for online production function fitting
                let quality_proxy = if response.content.is_some() { 0.8 } else { 0.3 };
                economy.record_observation(crate::orchestration::dsge_integration::CallObservation {
                    tokens_spent: usage.total_tokens as u64,
                    latency_ms: start_time.elapsed().as_millis() as u64,
                    success: response.content.is_some() || response.tool_calls.is_some(),
                    model_id: target_model.name().to_string(),
                    quality_proxy,
                });
                // Periodic online fit: every 8 observations
                if economy.observations.len() % 8 == 0 {
                    economy.update_production_from_observations();
                }
            }
            { let mn = target_model.name().to_string(); let lat = start_time.elapsed().as_millis() as u64; self.economy.lock().unwrap().record_model_result(&mn, lat, true, false); }
            self.economy.lock().unwrap().observe_latency(start_time.elapsed().as_millis() as u64);

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

            if let Some(tool_calls) = &response.tool_calls {
                log::info!("[SESSION: {}] Handling {} tool a-call(s)...", session_id, tool_calls.len());
                
                snapshot.tool_calls = tool_calls.len();
                if let Some(last_tool) = tool_calls.last() {
                    snapshot.last_tool_call_args = Some(last_tool.function.arguments.clone());
                }

                let mut tool_tasks = Vec::new();
                for tool_call in tool_calls {
                    // ... (tool task spawning logic remains the same)
                    let tool_call = tool_call.clone();
                    let _ = self.record_step(crate::trajectory::TrajectoryStep::Action {
                        tool: tool_call.function.name.clone(),
                        args: serde_json::from_str(&tool_call.function.arguments).unwrap_or_default(),
                        intent_id: None,
                        timestamp: chrono::Utc::now(),
                    }).await;
                    let tool = {
                        let mut reg = self.registry.lock().await;
                        reg.hydrate(&tool_call.function.name)
                    };
                    let event_tx = self.event_tx.clone();
                    let mut approval_rx = self.approval_tx.subscribe();
                    let hooks = self.hooks.clone();
                    let soul = {
                        let pm = self.prompt_manager.lock().await;
                        pm.soul().clone()
                    };
                    let policy_engine = self.policy_engine.clone();
                    let tool_call_counts = self.tool_call_counts.clone();
                    let dry_run = self.dry_run.load(std::sync::atomic::Ordering::SeqCst);
                    let forensic_id = uuid::Uuid::new_v4().to_string();
                    let el = self.event_log.clone();
                    let ss = self.snapshot_store.clone();
                    let el_session = session_id.to_string();

                    tool_tasks.push(tokio::spawn(async move {
                        let tool_name_from_call = tool_call.function.name.clone();
                        let tool = match tool {
                            Some(t) => t,
                            None => {
                                return (
                                    tool_call.id.clone(),
                                    Err(anyhow!("Tool not found: {}", tool_call.function.name)),
                                    tool_name_from_call,
                                    0,
                                    String::new(),
                                );
                            }
                        };

                        let mut args: serde_json::Value =
                            serde_json::from_str(&tool_call.function.arguments).unwrap_or_default();

                        // Defensive: skip tool calls with empty/malformed arguments
                        // (e.g. from truncated model responses like Gemini MAX_TOKENS)
                        if tool_call.function.arguments.trim().is_empty() || args.is_null() {
                            let tool_name = tool.name().to_string();
                            log::warn!(
                                "Agent: Skipping tool '{}' with empty/malformed arguments \
                                 (model response may have been truncated by MAX_TOKENS)",
                                tool_name
                            );
                            return (
                                tool_call.id.clone(),
                                Ok(format!(
                                    "Tool '{}' skipped: empty or malformed arguments \
                                     (the model response was likely truncated). \
                                     Please retry with fewer tools or a simpler request.",
                                    tool_name
                                )),
                                tool_name,
                                0,
                                String::new(),
                            );
                        }

                        // Global dry-run injection
                        if dry_run {
                            if let Some(obj) = args.as_object_mut() {
                                obj.insert("dry_run".to_string(), serde_json::json!(true));
                            }
                        }

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
                                    0,
                                    String::new(),
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
                                    0,
                                    String::new(),
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
                                    0,
                                    String::new(),
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

                        let start = std::time::Instant::now();
                        let args_hash = crate::event_log::short_hash(&args.to_string());
                        el.append(&el_session, crate::event_log::EventKind::ToolCalled {
                            tool: tool.name().to_string(),
                            args_hash,
                        }).await;

                        // --- Whole-Workspace Directory Snapshot Before Mutation (High-Risk Tools) ---
                        let mut workspace_snapshot = None;
                        let is_high_risk = tool.name() == "shell" || tool.name() == "codeact";
                        if is_high_risk {
                            if let Ok(dir) = std::env::current_dir() {
                                if let Ok(snap) = ss.snapshot_dir(&dir).await {
                                    workspace_snapshot = Some((dir, snap));
                                }
                            }
                        }

                        // --- Snapshot Before Mutation ---
                        let mut snapshot_before_id = None;
                        let is_file_mutation = (tool.name() == "write_file" || tool.name() == "apply_patch" || tool.name() == "mutate_ast") 
                            && args["path"].is_string();
                        
                        if is_file_mutation {
                            if let Some(path_str) = args["path"].as_str() {
                                let path = std::path::Path::new(path_str);
                                if path.exists() {
                                    if let Ok(id) = ss.snapshot_file(path).await {
                                        snapshot_before_id = Some(id);
                                    }
                                } else {
                                    snapshot_before_id = Some("none".to_string());
                                }
                            }
                        }

                        let result = tool.call(args.clone()).await;
                        let latency_ms = start.elapsed().as_millis() as u64;

                        // --- Rollback Workspace on High-Risk Tool Failure ---
                        if result.is_err() {
                            if let Some((ref dir, ref snap)) = workspace_snapshot {
                                log::warn!("High-risk tool '{}' execution failed. Rolling back workspace...", tool.name());
                                let _ = ss.restore_dir(dir, snap).await;
                            }
                        }

                        // --- Snapshot After Mutation ---
                        if let Some(before_id) = snapshot_before_id {
                            if result.is_ok() {
                                if let Some(path_str) = args["path"].as_str() {
                                    let path = std::path::Path::new(path_str);
                                    if let Ok(after_id) = ss.snapshot_file(path).await {
                                        el.append(&el_session, crate::event_log::EventKind::FileMutated {
                                            path: path_str.to_string(),
                                            snapshot_before_id: before_id,
                                            snapshot_after_id: after_id,
                                        }).await;
                                    }
                                }
                            }
                        }

                        let mut result_str = match &result {
                            Ok(s) => s.clone(),
                            Err(e) => e.to_string(),
                        };

                        if result_str.len() > 2000 {
                            if tool.name() == "web_fetch" || tool.name() == "browser" {
                                log::info!("Agent: Compressing large result from '{}' ({} chars)", tool.name(), result_str.len());
                                let preview = result_str.chars().take(800).collect::<String>();
                                result_str = format!("{}... [TRUNCATED due to size. The full content was omitted to save tokens. Use more specific search queries if needed.]", preview);
                            } else if result_str.len() > 10000 {
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
                                format!("{}...", result_str.chars().take(100).collect::<String>())
                            } else {
                                result_str.clone()
                            }),
                        });

                        let _ = hooks
                            .trigger_after_tool_call(tool.name(), &result_str)
                            .await;
                        
                        let tool_name = tool.name().to_string();
                        
                        el.append(&el_session, crate::event_log::EventKind::ToolResult {
                            tool: tool_name.clone(),
                            success: result.is_ok(),
                            latency_ms,
                            output_hash: crate::event_log::short_hash(&result_str),
                        }).await;
                        
                        (tool_call.id.clone(), result.map_err(|e| anyhow!(e.0)), tool_name, latency_ms, tool_call.function.arguments.clone())
                    }));
                }

                let task_results = futures::future::join_all(tool_tasks).await;
                let mut tool_errors = Vec::new();

                for task_res in task_results {
                    if let Ok((tool_call_id, result_res, tool_name, latency_ms, tool_args)) = task_res {
                        let success = result_res.is_ok();
                         if success {
                            snapshot.successful_tool_calls += 1;
                        }
                        let error = result_res.as_ref().err().map(|e: &anyhow::Error| e.to_string());

                        // Record codeact scripts to skill library (before error is moved)
                        if tool_name == "codeact" {
                            if let Ok(args_val) = serde_json::from_str::<serde_json::Value>(&tool_args) {
                                if let Some(script) = args_val.get("script").and_then(|s| s.as_str()) {
                                    let mut lib = self.skill_library.lock().unwrap();
                                    if success {
                                        lib.add(crate::orchestration::skill_library::LabeledScript {
                                            id: uuid::Uuid::new_v4().to_string(),
                                            task_description: user_message.to_string(),
                                            script: script.to_string(),
                                            label: crate::orchestration::skill_library::Label::Success {
                                                verified_by: "runtime".to_string()
                                            },
                                            category: "codeact".to_string(),
                                            timestamp: chrono::Utc::now(),
                                            function_signature: None,
                                            usage_count: 1,
                                            lifecycle: crate::orchestration::skill_library::PrimitiveStage::Experimental,
                                            genome: Default::default(),
                                        });
                                    } else {
                                        lib.add(crate::orchestration::skill_library::LabeledScript {
                                            id: uuid::Uuid::new_v4().to_string(),
                                            task_description: user_message.to_string(),
                                            script: script.to_string(),
                                            label: crate::orchestration::skill_library::Label::RuntimeError {
                                                message: error.clone().unwrap_or_default(),
                                            },
                                            category: "codeact".to_string(),
                                            timestamp: chrono::Utc::now(),
                                            function_signature: None,
                                            usage_count: 0,
                                            lifecycle: crate::orchestration::skill_library::PrimitiveStage::Experimental,
                                            genome: Default::default(),
                                        });
                                    }
                                }
                            }
                        }

                        if let Some(store) = &self.session_store {
                            let _ = store.save_tool_metric(&tool_name, success, latency_ms, error).await;
                        }

                        let result = match result_res {
                            Ok(r) => r,
                            Err(e) => {
                                let error_string = format!("Tool '{}' failed with error: {}", tool_name, e);
                                log::error!("{}", error_string);
                                tool_errors.push(error_string.clone());
                                e.to_string()
                            }
                        };
                        
                        // ... (rest of tool result handling is the same)

                        if result.contains("### INJECTED PLAYBOOK") {
                            if let Some(line) = result.lines().next() {
                                let name = line.replace("### INJECTED PLAYBOOK: ", "").trim().to_string();
                                let _ = self.register_playbook(session_id, name, result.clone()).await;
                            }
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
                    // Classify each failure for strategic retry decisions
                    let classified: Vec<_> = tool_errors.iter().map(|err| {
                        let tool_name = err.split('\'').nth(1).unwrap_or("unknown");
                        let class = crate::orchestration::retry::classify_failure(err, tool_name, current_iteration > 1);
                        (tool_name.to_string(), class, err.clone())
                    }).collect();

                    let terminal: Vec<_> = classified.iter()
                        .filter(|(_, c, _)| matches!(c, crate::orchestration::retry::FailureClass::Terminal))
                        .map(|(t, _, e)| format!("{}: {}", t, e))
                        .collect();
                    let strategic: Vec<_> = classified.iter()
                        .filter(|(_, c, _)| matches!(c, crate::orchestration::retry::FailureClass::Strategic))
                        .map(|(t, _, e)| format!("{}: {}", t, e))
                        .collect();

                    let error_summary = if !terminal.is_empty() {
                        format!("TERMINAL (do not retry):\n{}\nSTRATEGIC (consider alternative):\n{}",
                            terminal.join("\n"), strategic.join("\n"))
                    } else {
                        tool_errors.join("\n")
                    };

                    let rescue_message = Message {
                        role: "system".to_string(),
                        content: Some(MessageContent::Text(format!(
                            "Tool failures classified:\n{}",
                            error_summary
                        ))),
                        ..Default::default()
                    };
                    let mut state = state_arc.lock().await;
                    state.history.push(rescue_message);
                }

                // Record iteration completion in event log
                self.event_log.append(session_id, crate::event_log::EventKind::IterationCompleted {
                    iteration: current_iteration,
                    progress_delta: 0.0,
                    entropy,
                }).await;

                // --- Progress Tracking and Termination ---
                if let TerminationPolicy::ProgressBased {..} = budget.policy {
                    let signal = progress_tracker.record(snapshot);
                    if signal != TerminationSignal::Continue {
                        let reason = format!("Execution halted due to: {:?}", signal);
                        log::error!("CRITICAL: {}", reason);
                        return Err(anyhow!(AgentError::new(AgentErrorCode::HangDetected, reason)));
                    }
                }
                // --- End Progress Tracking ---
                continue; 
            }
            
            // ... (rest of the response handling logic remains the same)
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
                        content: MessageContent::Text(thought_content.clone()),
                    });

                    self.event_log.append(session_id, crate::event_log::EventKind::ThoughtEmitted {
                        content_hash: crate::event_log::short_hash(&thought_content),
                    }).await;

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

            {
                let mut state = state_arc.lock().await;
                state.history.push(final_msg.clone());
            }

            if let Some(store) = &self.session_store {
                let _ = store.save_message(session_id, &final_msg).await;
            }

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

            // Record task outcome to research notebook for context reuse
            {
                let mut notebook = self.research_notebook.lock().await;
                if notebook.current_goal.is_empty() || notebook.current_goal == "Uninitialized" {
                    *notebook = pharmakon_common::ResearchNotebook::new(
                        &user_message.chars().take(80).collect::<String>()
                    );
                }
                if notebook.should_stop() {
                    notebook.max_steps += 5; // extend for continued use
                }
                notebook.step_count += 1;
                notebook.verified_facts.push(pharmakon_common::Fact {
                    content: format!("Task: {} → {}", 
                        user_message.chars().take(60).collect::<String>(),
                        final_content.chars().take(100).collect::<String>()),
                    source_url: session_id.to_string(),
                    confidence: 0.7,
                    timestamp: chrono::Utc::now(),
                });
            }

            let _ = self.event_tx.send(Event::AgentResponse {
                content: response
                    .content
                    .unwrap_or(MessageContent::Text("".to_string())),
            });
            return Ok(final_content);
        }
    }).await
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
        Some(results.join("
---
"))
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
            .join("
");

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
                    content: Some(MessageContent::Text(format!("Context:
{}", context))),
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
        let available = crate::providers::registry::ModelRegistry::list_available_models();
        if parts.len() < 2 {
            let mut resp = format!("Current model: {}\n\nAvailable models:\n", self.model_name().await);
            for m in &available {
                let marker = if m == &self.model_name().await { "●" } else { "○" };
                resp.push_str(&format!("  {} {}\n", marker, m));
            }
            resp.push_str("\nUsage: /model <model_id>");
            return Ok(resp);
        }
        let model_id = parts[1];
        if model_id == "auto" {
            self.economy.lock().unwrap().set_auto();
            return Ok("Switched to AUTO mode.".to_string());
        }
        if let Some(new_model) = crate::providers::registry::ModelRegistry::get_model(model_id) {
            let mut model = self.model.lock().await;
            *model = new_model;
            self.economy.lock().unwrap().set_manual(model_id);
            let _ = self.event_tx.send(Event::ModelSwitched { model_id: model_id.to_string() });
            Ok(format!("Switched to model: {}", model_id))
        } else {
            let mut resp = format!("Model not found: {}\n\nAvailable:\n", model_id);
            for m in &available { resp.push_str(&format!("  {}\n", m)); }
            Ok(resp)
        }
    }


    async fn handle_plan_command(&self, cmd: &str) -> Result<String> {
        let task = if cmd.len() > 5 { cmd[5..].trim().to_string() } else { String::new() };
        let session_id = self.session_id.lock().await.clone();
        let task = if task.is_empty() { "Perform a structural codebase overview and verify compilation".to_string() } else { task };
        match crate::orchestration::world::execute_world_model(self, &session_id, &task).await {
            Ok(result) => Ok(format!("World Model plan executed:\n{}", result)),
            Err(e) => Err(anyhow::anyhow!("World Model plan failed: {}", e)),
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

    /// Rollback a single file to a previously snapshotted state.
    ///
    /// Uses the SnapshotStore's content-addressed storage to restore
    /// the file to its exact state at the time the snapshot was taken.
    /// Safe: does not touch uncommitted changes outside the target file.
    pub async fn rollback_to_snapshot(
        &self,
        path: &std::path::Path,
        snapshot_id: &str,
    ) -> Result<()> {
        if snapshot_id == "none" {
            // File didn't exist before the mutation — remove it
            if path.exists() {
                tokio::fs::remove_file(path).await?;
                log::info!(
                    "Rollback: removed {} (file did not exist before mutation)",
                    path.display()
                );
            }
            return Ok(());
        }

        self.snapshot_store.restore(snapshot_id, path).await.map_err(|e| {
            anyhow!(
                "Rollback failed for {} (snapshot {}): {}",
                path.display(),
                snapshot_id,
                e
            )
        })?;

        log::info!(
            "Rollback: restored {} to snapshot {}",
            path.display(),
            snapshot_id.chars().take(8).collect::<String>()
        );
        Ok(())
    }

    /// Rollback all file mutations that occurred after a given event ID.
    ///
    /// Walks the event log in reverse from the latest event back to `event_id`,
    /// restoring each mutated file to its pre-mutation snapshot.
    /// This provides atomic rollback of an entire agent session segment.
    pub async fn rollback_to_event(&self, event_id: u64) -> Result<()> {
        let events = self.event_log.events_since(event_id).await;

        if events.is_empty() {
            log::info!("Rollback: no events to roll back (event_id={})", event_id);
            return Ok(());
        }

        // Process in reverse chronological order for correct restoration
        let mut rollback_count = 0;
        for event in events.iter().rev() {
            if let crate::event_log::EventKind::FileMutated {
                path,
                snapshot_before_id,
                ..
            } = &event.kind
            {
                let file_path = std::path::Path::new(path);
                self.rollback_to_snapshot(file_path, snapshot_before_id)
                    .await?;
                rollback_count += 1;
            }
        }

        log::info!(
            "Rollback complete: restored {} file(s) to state before event_id={}",
            rollback_count,
            event_id
        );
        Ok(())
    }
}