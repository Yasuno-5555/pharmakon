use crate::orchestration::budget::{self, IterationSnapshot, ProgressTracker, TerminationSignal, TerminationPolicy};
use crate::orchestration::dsge_integration::AgentEconomy;

/// Sentinel value for SnapshotStore: file did not exist before mutation.
const SNAPSHOT_DID_NOT_EXIST: &str = "__snapshot_did_not_exist__";
use crate::model::{
    AgentError, AgentErrorCode, AgentModel, CompletionRequest,
    Message, MessageContent, ToolDefinition, ToolCategory,
};
use crate::system_prompt::SystemPromptManager;
use anyhow::{Result, anyhow};
use pharmakon_common::Event;
use pharmakon_memory::BeliefSystem;
/// Wrapper for broadcast::Sender::send that logs warnings on overflow.
/// Closed errors are expected in one-shot mode (no receiver subscribed) — suppressed.
macro_rules! try_send_event {
    ($tx:expr, $event:expr) => {
        if let Err(e) = $tx.send($event) {
            let err_str = e.to_string();
            if !err_str.contains("closed") {
                log::warn!("Event bus error: {}", err_str);
            }
        }
    };
}
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
    pub last_accessed: std::time::Instant,
}

pub struct Agent {
    // ── Model & Model Routing ──
    pub model: Arc<Mutex<Arc<dyn AgentModel>>>,
    pub model_router: Arc<crate::model_router::ModelRouter>,
    pub planner_model: Option<Arc<Mutex<Arc<dyn AgentModel>>>>,
    pub fallback_models: Arc<StdMutex<Vec<String>>>,
    pub economy: Arc<std::sync::Mutex<AgentEconomy>>,
    pub total_tokens: Arc<std::sync::atomic::AtomicU64>,
    pub total_cost: Arc<Mutex<f64>>,
    pub token_budget: u64,
    pub usage_history: Arc<Mutex<Vec<(chrono::DateTime<chrono::Utc>, u64, f64)>>>,

    // ── Session & Conversation ──
    pub session_id: Arc<Mutex<String>>,
    pub session_states: Arc<Mutex<std::collections::HashMap<String, Arc<Mutex<SessionState>>>>>,
    pub session_store: Option<Arc<crate::persistence::DbSessionStore>>,
    pub trajectory: Arc<Mutex<crate::trajectory::Trajectory>>,
    pub compactor: Arc<Mutex<crate::memory::compactor::ContextCompactor>>,
    pub interaction_count: Arc<std::sync::atomic::AtomicU32>,
    pub start_time: std::time::Instant,

    // ── Prompt & System Context ──
    pub prompt_manager: Arc<Mutex<SystemPromptManager>>,
    pub context_manager: Arc<Mutex<crate::context::ContextManager>>,
    pub active_categories: Arc<Mutex<std::collections::HashSet<crate::model::ToolCategory>>>,
    pub hooks: Arc<crate::hooks::HookRegistry>,
    pub research_notebook: Arc<Mutex<crate::orchestration::research::ResearchNotebook>>,

    // ── Event Bus ──
    pub event_tx: broadcast::Sender<Event>,
    pub approval_tx: broadcast::Sender<(String, bool)>,

    // ── Tools & Execution ──
    pub registry: Arc<Mutex<pharmakon_tools::registry::ToolMetaRegistry>>,
    pub tool_scheduler: Arc<crate::orchestration::tool_scheduler::ToolScheduler>,
    pub policy_engine: Arc<crate::security::policy::PolicyEngine>,
    pub tool_call_counts: Arc<Mutex<std::collections::HashMap<String, u32>>>,
    pub dry_run: Arc<std::sync::atomic::AtomicBool>,
    pub governor: Arc<crate::orchestration::governor::ToolGovernor>,

    // ── Event Log & Rollback ──
    pub event_log: Arc<crate::event_log::EventLog>,
    pub snapshot_store: Arc<crate::snapshot_store::SnapshotStore>,
    pub last_workspace_snapshot: Arc<Mutex<Option<chrono::DateTime<chrono::Utc>>>>,

    // ── Long-term Memory ──
    pub fact_memory: Option<Arc<Mutex<crate::memory::BeliefSystem>>>,
    pub semantic_search: Option<Arc<pharmakon_memory::semantic_search::SemanticSearch>>,
    pub knowledge_nexus: Option<Arc<pharmakon_memory::weaver::KnowledgeNexus>>,
    pub graph_store: Option<Arc<crate::memory::graph::GraphStore>>,

    // ── Health & Security ──
    pub health_monitor: crate::orchestration::health_monitor::HealthMonitor,

    // ── Territory (workspace awareness) ──
    pub territory_manager: Arc<crate::orchestration::territory::TerritoryManager>,

    // ── Swarm Economy ──
    pub bank_of_pharmakon: Arc<Mutex<crate::orchestration::economy_v2::BankOfPharmakon>>,

    // ── Background Processes ──
    pub dream_started: Arc<std::sync::atomic::AtomicBool>,
    pub cron_manager: Arc<StdMutex<Option<Arc<crate::automation::cron::CronManager>>>>,
    pub skill_library: Arc<std::sync::Mutex<crate::orchestration::skill_library::RhaiSkillLibrary>>,
    pub shutdown_token: Arc<std::sync::atomic::AtomicBool>,

    // ── Peripheral I/O ──
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
            interaction_count: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            fallback_models: self.fallback_models.clone(),
            total_tokens: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            total_cost: Arc::new(Mutex::new(0.0)),
            token_budget: self.token_budget,
            start_time: self.start_time,
            dry_run: self.dry_run.clone(),
            tool_call_counts: Arc::new(Mutex::new(std::collections::HashMap::new())),
            territory_manager: self.territory_manager.clone(),
            research_notebook: self.research_notebook.clone(),
            usage_history: self.usage_history.clone(),
            event_log: self.event_log.clone(),
            snapshot_store: self.snapshot_store.clone(),
            last_workspace_snapshot: Arc::new(Mutex::new(None)), // fresh cooldown for clone
            registry: self.registry.clone(),
            governor: self.governor.clone(),
            dream_started: self.dream_started.clone(),
            cron_manager: self.cron_manager.clone(),
            economy: self.economy.clone(),
            bank_of_pharmakon: self.bank_of_pharmakon.clone(),
            skill_library: self.skill_library.clone(),
            vision_stream: self.vision_stream.clone(),
            tool_scheduler: self.tool_scheduler.clone(),
            model_router: self.model_router.clone(),
            shutdown_token: self.shutdown_token.clone(),
        }
    }
}

impl Agent {
    pub fn new(model: Arc<dyn AgentModel>, session_id: String) -> Self {
        let (event_tx, _) = broadcast::channel(1024);
        let (approval_tx, _) = broadcast::channel(1024);
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
        active_categories.insert(ToolCategory::FileSystem);
        active_categories.insert(ToolCategory::System);
        active_categories.insert(ToolCategory::Coding);
        active_categories.insert(ToolCategory::Network);
        active_categories.insert(ToolCategory::Media);
        active_categories.insert(ToolCategory::Autonomous);
        active_categories.insert(ToolCategory::Orchestration);
        active_categories.insert(ToolCategory::Custom("generic".to_string()));
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

        let economy = Arc::new(std::sync::Mutex::new(AgentEconomy::new(0.5)));
        let fallback_models = Arc::new(StdMutex::new(Vec::new()));
        let token_budget = 250_000;

        // Clone for struct fields before moving originals into ModelRouter
        let mr_event_tx = event_tx.clone();
        let mr_total_tokens = total_tokens.clone();

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
            fallback_models: fallback_models.clone(),
            total_tokens,
            total_cost,
            token_budget: 250_000,
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
            snapshot_store: {
                let ss = Arc::new(crate::snapshot_store::SnapshotStore::new(
                    home.join(".pharmakon").join("snapshots"),
                ));
                let ss_clone = ss.clone();
                tokio::spawn(async move {
                    if let Err(e) = ss_clone.prune_on_startup().await {
                        log::warn!("SnapshotStore: startup prune failed: {}", e);
                    }
                });
                ss
            },
            last_workspace_snapshot: Arc::new(Mutex::new(None)),
            dream_started: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            cron_manager: Arc::new(StdMutex::new(None)),
            registry,
            skill_library: Arc::new(std::sync::Mutex::new(crate::orchestration::skill_library::RhaiSkillLibrary::new())),
            economy: economy.clone(),
            bank_of_pharmakon: Arc::new(Mutex::new(crate::orchestration::economy_v2::BankOfPharmakon::new(100_000))),
            governor: Arc::new(crate::orchestration::governor::ToolGovernor::new(Default::default())),
            vision_stream: None,
            tool_scheduler: Arc::new(crate::orchestration::tool_scheduler::ToolScheduler::new(
                std::env::current_dir().unwrap_or_default(),
            )),
            model_router: Arc::new(crate::model_router::ModelRouter::new(
                economy,
                mr_event_tx,
                mr_total_tokens,
                token_budget,
                fallback_models,
            )),
            shutdown_token: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Signal all background tasks to shut down gracefully.
    pub fn shutdown(&self) {
        self.shutdown_token.store(true, std::sync::atomic::Ordering::SeqCst);
        log::info!("Agent shutdown signal sent");
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
        
        // 1. If session exists, update last_accessed and return
        if let Some(state) = states.get(session_id) {
            if let Ok(mut s) = state.try_lock() {
                s.last_accessed = std::time::Instant::now();
            }
            return state.clone();
        }

        // 2. Load from store / Create new
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
            last_accessed: std::time::Instant::now(),
        }));

        // 3. Perform LRU eviction if cached sessions exceed limit
        let limit = std::env::var("PHARMAKON_MAX_CACHED_SESSIONS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(100);

        if states.len() >= limit {
            let mut oldest_id: Option<String> = None;
            let mut oldest_time = std::time::Instant::now();

            for (id, st) in states.iter() {
                if let Ok(s) = st.try_lock() {
                    if s.last_accessed < oldest_time {
                        oldest_time = s.last_accessed;
                        oldest_id = Some(id.clone());
                    }
                }
            }

            if let Some(id_to_evict) = oldest_id {
                log::info!("Evicting cold session state: {} to prevent memory leak", id_to_evict);
                states.remove(&id_to_evict);
            }
        }

        states.insert(session_id.to_string(), state.clone());
        state
    }

    pub fn with_store(mut self, store: Arc<crate::persistence::DbSessionStore>) -> Self {
        self.session_store = Some(store.clone());
        // Update registry deps lazily if locked, otherwise immediately
        if let Ok(mut reg) = self.registry.try_lock() {
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
        if let Ok(mut reg) = self.registry.try_lock() {
            reg.update_deps(|deps| {
                deps.nexus = Some(nexus);
            });
        }
        self
    }

    pub async fn sync_registry_deps(&self) {
        let mut reg = self.registry.lock().await;
        let store = self.session_store.clone();
        let nexus = self.knowledge_nexus.clone();
        reg.update_deps(|deps| {
            if let Some(s) = store {
                deps.store = Some(s);
            }
            if let Some(n) = nexus {
                deps.nexus = Some(n);
            }
        });
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

    /// Ensure Dream Mode background decay loop is started (once per process).
    fn ensure_dream_mode(&self) {
        if self.dream_started.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let skill_lib = self.skill_library.clone();
        let shutdown = self.shutdown_token.clone();
        tokio::spawn(async move {
            log::info!("Dream Mode started");
            while !shutdown.load(std::sync::atomic::Ordering::SeqCst) {
                tokio::time::sleep(std::time::Duration::from_secs(300)).await;
                if shutdown.load(std::sync::atomic::Ordering::SeqCst) { break; }
                let mut lib = skill_lib.lock().unwrap();
                lib.decay();
                log::debug!("Dream Mode: decay cycle complete, {} entries", lib.entries.len());
            }
            log::info!("Dream Mode stopped");
        });
    }

    /// Gather parallel context from semantic search and Knowledge Nexus.
    async fn gather_context(&self, user_message: &str) {
        let semantic_search = self.semantic_search.clone();
        let knowledge_nexus = self.knowledge_nexus.clone();
        let current_sess = self.session_id.lock().await.clone();

        let (semantic_res, nexus_res) = tokio::join!(
            async {
                if let Some(search) = semantic_search {
                    if let Ok(mems) = search.search_with_limit(user_message, 3).await {
                        let filtered: Vec<String> = mems.into_iter().filter(|m| {
                            if m.starts_with("[Session: ") {
                                m.starts_with(&format!("[Session: {}]", current_sess))
                            } else {
                                true
                            }
                        }).collect();
                        Some(filtered)
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            async {
                if let Some(nexus) = knowledge_nexus {
                    if let Ok(mems) = nexus.smart_search(user_message, 8).await {
                        let filtered: Vec<String> = mems.into_iter().filter(|m| {
                            if m.starts_with("[Session: ") {
                                m.starts_with(&format!("[Session: {}]", current_sess))
                            } else {
                                true
                            }
                        }).collect();
                        Some(filtered)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        );

        if let Some(memories) = semantic_res
            && !memories.is_empty() {
                let memory_context = memories.join("\n---\n");
                self.add_to_working_memory(
                    format!("Long-term Memories:\n{}", memory_context),
                    0.7,
                    "SemanticSearch".to_string(),
                ).await;
            }

        if let Some(memories) = nexus_res
            && !memories.is_empty() {
                let memory_context = memories.join("\n---\n");
                let _ = self.hooks.trigger_context_recovered(&memory_context).await;
                self.add_to_working_memory(
                    format!("Knowledge Nexus Insights (Hybrid + Graph):\n{}", memory_context),
                    0.9,
                    "KnowledgeNexus".to_string(),
                ).await;
            }
    }

    /// Extract <think>...</think> blocks from content, returning (cleaned_content, thoughts).
    fn extract_thoughts(content: &str) -> (String, Vec<String>) {
        let mut cleaned = content.to_string();
        let mut thoughts = Vec::new();
        while let Some(start) = cleaned.find("<think>") {
            if let Some(end) = cleaned[start..].find("</think>") {
                let absolute_end = start + end + 8;
                let thought = cleaned[start + 7..start + end].trim().to_string();
                thoughts.push(thought);
                cleaned.replace_range(start..absolute_end, "");
            } else {
                break;
            }
        }
        (cleaned.trim().to_string(), thoughts)
    }

    /// Process the final assistant response: save to history, trigger reflection,
    /// index to nexus, update research notebook.
    async fn process_final_response(
        &self,
        final_content: &str,
        raw_response: &pharmakon_common::agent_types::CompletionResponse,
        user_message: &str,
        session_id: &str,
        state_arc: &Arc<Mutex<SessionState>>,
    ) {
        let final_msg = Message {
            role: "assistant".to_string(),
            content: if final_content.is_empty() { None } else { Some(MessageContent::Text(final_content.to_string())) },
            ..Default::default()
        };
        let _ = self.hooks.trigger_message_sent(&final_msg).await;

        {
            let mut state = state_arc.lock().await;
            state.history.push(final_msg.clone());
        }

        if let Some(store) = &self.session_store {
            if let Err(e) = store.save_message(session_id, &final_msg).await {
                log::warn!("Failed to save final message: {}", e);
            }
        }

        let count = self.interaction_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        let reflection_interval = std::env::var("PHARMAKON_REFLECTION_INTERVAL")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(5);
        if count.is_multiple_of(reflection_interval) {
            let agent_clone = self.clone();
            tokio::spawn(async move {
                if let Err(e) = agent_clone.reflect().await {
                    log::error!("Error during background reflection: {}", e);
                }
            });
        }

        if let Some(nexus) = &self.knowledge_nexus {
            let nexus = nexus.clone();
            let content = final_content.to_string();
            tokio::spawn(async move {
                let id = uuid::Uuid::new_v4().to_string();
                if let Err(e) = nexus.remember_batch(vec![(id, content)]).await {
                    log::warn!("Failed to index to KnowledgeNexus: {}", e);
                }
            });
        }

        {
            let mut notebook = self.research_notebook.lock().await;
            if notebook.current_goal.is_empty() || notebook.current_goal == "Uninitialized" {
                *notebook = pharmakon_common::ResearchNotebook::new(
                    &user_message.chars().take(80).collect::<String>()
                );
            }
            if notebook.should_stop() && notebook.verified_facts.len() > 50 {
                notebook.verified_facts.remove(0);
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

        try_send_event!(self.event_tx, Event::AgentResponse {
            content: raw_response.content.clone().unwrap_or(MessageContent::Text(String::new())),
        });
    }

    /// Detect time/date/weather queries and fetch real-time context automatically.
    /// Returns Some(fact_string) if a real-time query is detected.
    fn detect_real_time_query(message: &str) -> Option<String> {
        let lower = message.to_lowercase();
        let is_time_query = lower.contains("時刻") || lower.contains("時間") || lower.contains("今")
            || lower.contains("日付") || lower.contains("今日") || lower.contains("何日")
            || lower.contains("date") || lower.contains("time") || lower.contains("what time")
            || lower.contains("current time") || lower.contains("today")
            || lower.contains("weather") || lower.contains("天気")
            || lower.contains("曜日") || lower.contains("day of week");
        if !is_time_query {
            return None;
        }
        let date_output = std::process::Command::new("date").output().ok()?;
        let date_str = String::from_utf8_lossy(&date_output.stdout).to_string().trim().to_string();
        if date_str.is_empty() {
            return None;
        }
        Some(format!(
            "[Real-time context automatically fetched for this query]\n\
             Current system time: {}\n\
             (Use this information to answer the user's question about the current time/date.)",
            date_str
        ))
    }

    /// Build tool definitions: core + BM25 search results + serendipity injection.
    async fn build_tool_definitions(&self, user_message: &str) -> Option<Vec<ToolDefinition>> {
        let mut reg = self.registry.lock().await;
        let active_cats = self.active_categories.lock().await;

        let mut tools_to_inject = reg.all_metadata()
            .iter()
            .filter(|m| m.category == ToolCategory::Core)
            .cloned()
            .collect::<Vec<_>>();

        let search_results = reg.search(user_message, 15);
        for meta in search_results {
            if !tools_to_inject.iter().any(|t| t.name == meta.name) {
                tools_to_inject.push(meta);
            }
        }

        let non_core: Vec<_> = reg.all_metadata().iter()
            .filter(|m| m.category != ToolCategory::Core)
            .cloned()
            .collect();
        if !non_core.is_empty() {
            let n_sample = 3.min(non_core.len());
            let sampled: Vec<_> = non_core.iter()
                .filter(|m| !tools_to_inject.iter().any(|t| t.name == m.name))
                .collect();
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

        if active_tools.is_empty() { None } else { Some(active_tools) }
    }

    pub async fn revert_last_mutation(&self, session_id: &str) -> Result<String> {
        let events = self.event_log.session_events(session_id).await;
        let last_mutation = events.iter().rev().find_map(|e| match &e.kind {
            crate::event_log::EventKind::FileMutated { path, snapshot_before_id, .. } => {
                Some((path.clone(), snapshot_before_id.clone()))
            }
            _ => None,
        });

        match last_mutation {
            Some((path, before_id)) => {
                let path_buf = std::path::PathBuf::from(&path);
                if before_id == SNAPSHOT_DID_NOT_EXIST {
                    if path_buf.exists() {
                        std::fs::remove_file(&path_buf)?;
                        Ok(format!("Successfully reverted: Deleted newly created file '{}'", path))
                    } else {
                        Ok(format!("File '{}' was newly created but already deleted.", path))
                    }
                } else {
                    self.snapshot_store.restore(&before_id, &path_buf).await?;
                    Ok(format!("Successfully reverted '{}' to its state before mutation.", path))
                }
            }
            None => Err(anyhow!("No recent file mutations found to revert in session '{}'", session_id)),
        }
    }

    pub async fn chat_on_session(&self, user_message: &str, session_id: &str) -> Result<String> {
        self.sync_registry_deps().await;
        self.ensure_dream_mode();
        CURRENT_SESSION_ID.scope(session_id.to_string(), async {
            if user_message.starts_with("/model") {
                return self.handle_model_command(user_message).await;
            }
            if user_message.starts_with("/plan") {
                return self.handle_plan_command(user_message).await;
            }

            // Reset step count on new user message and dynamically shift goal if needed
            {
                let mut notebook = self.research_notebook.lock().await;
                notebook.step_count = 0; // reset sequential steps to allow 10 more autonomous steps
                
                let new_goal = user_message.chars().take(80).collect::<String>();
                if notebook.current_goal.is_empty() || notebook.current_goal == "Uninitialized" {
                    *notebook = pharmakon_common::ResearchNotebook::new(&new_goal);
                } else {
                    let prev_lower = notebook.current_goal.to_lowercase();
                    let new_lower = new_goal.to_lowercase();
                    let prev_goal_words: std::collections::HashSet<_> = prev_lower.split_whitespace().collect();
                    let new_goal_words: std::collections::HashSet<_> = new_lower.split_whitespace().collect();
                    let intersection = prev_goal_words.intersection(&new_goal_words).count();
                    if intersection == 0 {
                        log::info!("ResearchNotebook: Goal changed. Resetting notebook.");
                        *notebook = pharmakon_common::ResearchNotebook::new(&new_goal);
                    }
                }
            }

            let state_arc = self.get_session_state(session_id).await;

            let redacted_user_message = crate::security::redaction::redact_text(user_message);
            let user_msg = Message {
                role: "user".to_string(),
                content: Some(MessageContent::Text(redacted_user_message.clone())),
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
                    if let Err(e) = context_engine.prune_history(&mut history).await {
                        log::warn!("Failed to prune history: {}", e);
                    }
                }
                state.history = history;

                let prune_threshold = std::env::var("PHARMAKON_PRUNE_THRESHOLD")
                    .ok()
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(20);
                if state.history.len() > prune_threshold {
                    let compactor = self.compactor.lock().await;
                    if let Ok(compacted) = compactor.compact(state.history.clone()).await {
                        // Prune "I can't" patterns from compacted context to prevent
                        // self-reinforcing loops (e.g., "I can't check settings")
                        let filtered: Vec<Message> = compacted.into_iter().map(|mut msg| {
                            if let Some(ref mut content) = msg.content {
                                if let Some(text) = content.as_text() {
                                    let lower = text.to_lowercase();
                                    if msg.role == "assistant" && (lower.contains("cannot directly") || lower.contains("can't check") || lower.contains("できません")) {
                                        // Replace self-limiting statements with neutral capability note
                                        *content = MessageContent::Text(
                                            "[Previous response omitted — contained self-limiting statement]".to_string()
                                        );
                                    }
                                }
                            }
                            msg
                        }).collect();
                        state.history = filtered;
                    }
                }
            }

        try_send_event!(self.event_tx, Event::AgentThought {
            content: MessageContent::Text("Thinking...".to_string()),
        });

        // Parallel context gathering
        self.gather_context(&redacted_user_message).await;

        // P1: Auto-detect time/date/weather queries and inject real-time context
        if let Some(rt_context) = Self::detect_real_time_query(&redacted_user_message) {
            self.add_to_working_memory(rt_context.clone(), 1.0, "RealTimeContext".to_string()).await;
            let mut state = state_arc.lock().await;
            state.history.push(Message {
                role: "system".to_string(),
                content: Some(MessageContent::Text(rt_context)),
                ..Default::default()
            });
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
            {
                let mut econ = self.economy.lock().unwrap();
                econ.sample_system_telemetry();
                econ.update_inflation(current_iteration as u64 * 400, 4000);
            }
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
                let max_entropy = std::env::var("PHARMAKON_MAX_ENTROPY")
                    .ok()
                    .and_then(|v| v.parse::<f32>().ok())
                    .unwrap_or(0.95);
                let entropy_signal = progress_tracker.check_entropy(entropy, max_entropy);
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

            if let TerminationPolicy::FixedIterations(max_iters) = budget.policy
                && current_iteration > max_iters {
                    log::info!(
                        "Fixed iteration limit reached ({}). Task finished.",
                        max_iters
                    );
                    let final_response = {
                        let state = state_arc.lock().await;
                        // Search backward for last assistant message; fall back to last tool result
                        let last_assistant = state.history.iter().rev()
                            .find(|m| m.role == "assistant")
                            .and_then(|m| m.content.as_ref().map(|c| c.to_string()));
                        match last_assistant {
                            Some(content) if !content.trim().is_empty() => content,
                            _ => {
                                // Fallback: use last tool result
                                state.history.iter().rev()
                                    .find(|m| m.role == "tool")
                                    .and_then(|m| m.content.as_ref().map(|c| {
                                        let text = c.to_string();
                                        if text.len() > 200 {
                                            format!("{}...", text.chars().take(200).collect::<String>())
                                        } else { text }
                                    }))
                                    .unwrap_or_else(|| "Task completed (iteration limit reached).".to_string())
                            }
                        }
                    };
                    break Ok(final_response);
                }
            
            let mut snapshot = IterationSnapshot::new();
            // --- End of Loop: Budget and Progress Checks ---

            log::info!("[SESSION: {}] Agent iteration start ({})...", session_id, current_iteration);
            
            // ... (rest of the message preparation logic remains the same)
            let mut messages_to_send;
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
                            "No playbook active. Use `playbook` tool: action='suggest' with query describing your task to find the right playbook, then action='inject' to activate it. Built-in playbooks: web_research, deep_research, code_review, security_audit, rust_refactor, implement_feature, bug_hunt, dependency_update, project_setup, write_docs, data_analysis, general_task.".to_string()
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
            { let shadow = self.economy.lock().unwrap().shadow_directive(); if !shadow.is_empty() && !messages_to_send.is_empty() && let Some(ref mut c) = messages_to_send[0].content { let text = c.as_text().unwrap_or(""); *c = pharmakon_common::agent_types::MessageContent::Text(format!("{}\n{}", text, shadow)); } }
            }

            let tool_definitions = self.build_tool_definitions(user_message).await;

            let target_model = {
                let m = self.model.lock().await;
                let default_model = (*m).clone();
                if default_model.name() == "mock-model" || default_model.name() == "test" {
                    default_model
                } else {
                    self.economy.lock().unwrap().select_model(user_message, match complexity { budget::TaskComplexity::Simple => 0.2, budget::TaskComplexity::Standard => 0.5, budget::TaskComplexity::Deep => 0.8 }).unwrap_or(default_model)
                }
            };

            let complexity_value = match complexity {
                budget::TaskComplexity::Simple => 0.2,
                budget::TaskComplexity::Standard => 0.5,
                budget::TaskComplexity::Deep => 0.8,
            };

            let (response, _target_model) = self.model_router.execute_with_fallback(
                messages_to_send,
                tool_definitions,
                target_model,
                session_id,
                start_time,
                complexity_value,
            ).await?;

            // ── Hard token budget enforcement ──
            let used = self.total_tokens.load(std::sync::atomic::Ordering::SeqCst);
            let token_limit = self.token_budget;
            if used > token_limit {
                let reason = format!(
                    "Token budget exhausted: {} / {} tokens used. Stopping execution.",
                    used, token_limit
                );
                log::error!("{}", reason);
                self.event_log.append(session_id, crate::event_log::EventKind::SessionEvent {
                    action: "budget_exhausted".to_string(),
                    detail: reason.clone(),
                }).await;
                return Err(anyhow!(AgentError::new(AgentErrorCode::HangDetected, reason)));
            }
            if used > token_limit * 8 / 10 {
                // Inject budget pressure directive into next system message
                let mut state = state_arc.lock().await;
                state.history.push(Message {
                    role: "system".to_string(),
                    content: Some(MessageContent::Text(format!(
                        "BUDGET WARNING: {:.0}% of your token budget used ({} / {}). Be extremely concise. Skip explanations. Return final answers immediately when possible.",
                        (used as f64 / token_limit as f64) * 100.0, used, token_limit
                    ))),
                    ..Default::default()
                });
            }

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

            try_send_event!(self.event_tx, Event::InteractionFinished {
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
                    let lws = self.last_workspace_snapshot.clone();
                    let el_session = session_id.to_string();
                    // Capture tool list for hallucination-tolerant error messages
                    let known_tool_names: Vec<String> = {
                        let reg = self.registry.lock().await;
                        reg.all_metadata().iter().map(|m| m.name.clone()).collect()
                    };

                    tool_tasks.push(tokio::spawn(async move {
                        let tool_name_from_call = tool_call.function.name.clone();
                        let tool = match tool {
                            Some(t) => t,
                            None => {
                                // Suggest similar tools to help the LLM recover from hallucinations
                                let query = &tool_call.function.name;
                                let suggestions: Vec<&str> = known_tool_names
                                    .iter()
                                    .filter(|n| {
                                        // Fuzzy match: any word overlap
                                        let q_parts: Vec<&str> = query.split('_').collect();
                                        q_parts.iter().any(|p| n.contains(p))
                                    })
                                    .take(5)
                                    .map(|s| s.as_str())
                                    .collect();
                                let hint = if suggestions.is_empty() {
                                    format!("Available tools include: {}", known_tool_names.iter().take(8).map(|s| s.as_str()).collect::<Vec<_>>().join(", "))
                                } else {
                                    format!("Did you mean one of: {}? (Full list: {})",
                                        suggestions.join(", "),
                                        known_tool_names.iter().take(5).map(|s| s.as_str()).collect::<Vec<_>>().join(", "))
                                };
                                return (
                                    tool_call.id.clone(),
                                    Err(anyhow!("Tool '{}' not found. {}", query, hint)),
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
                        if dry_run
                            && let Some(obj) = args.as_object_mut() {
                                obj.insert("dry_run".to_string(), serde_json::json!(true));
                            }

                        try_send_event!(event_tx, Event::ToolCall {
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
                            try_send_event!(event_tx, Event::ApprovalRequest {
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

                        try_send_event!(event_tx, Event::ForensicLog {
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
                            // Cooldown: only snapshot the whole workspace once per 60 seconds
                            // to prevent unbounded storage growth from rapid shell/codeact calls.
                            let should_snapshot = {
                                let mut last = lws.lock().await;
                                let cooldown = chrono::Duration::seconds(60);
                                let now = chrono::Utc::now();
                                if last.is_none_or(|t| now - t > cooldown) {
                                    *last = Some(now);
                                    true
                                } else {
                                    log::debug!(
                                        "SnapshotStore: skipping whole-workspace snapshot (cooldown active, last was {:?} ago)",
                                        last.map(|t| (now - t).num_seconds())
                                    );
                                    false
                                }
                            };
                            if should_snapshot
                                && let Ok(dir) = std::env::current_dir()
                                    && let Ok(snap) = ss.snapshot_dir(&dir).await {
                                        workspace_snapshot = Some((dir, snap));
                                    }
                        }

                        // --- Snapshot Before Mutation ---
                        let mut snapshot_before_id = None;
                        let is_file_mutation = (tool.name() == "write_file" || tool.name() == "apply_patch" || tool.name() == "mutate_ast") 
                            && args["path"].is_string();
                        
                        if is_file_mutation
                            && let Some(path_str) = args["path"].as_str() {
                                let path = std::path::Path::new(path_str);
                                if path.exists() {
                                    if let Ok(id) = ss.snapshot_file(path).await {
                                        snapshot_before_id = Some(id);
                                    }
                                } else {
                                    snapshot_before_id = Some(SNAPSHOT_DID_NOT_EXIST.to_string());
                                }
                            }

                        let timeout_secs = std::env::var("PHARMAKON_TOOL_TIMEOUT_SECS")
                            .ok()
                            .and_then(|v| v.parse::<u64>().ok())
                            .unwrap_or(30);

                        let result = match tokio::time::timeout(
                            std::time::Duration::from_secs(timeout_secs),
                            tool.call(args.clone())
                        ).await {
                            Ok(res) => res,
                            Err(_) => Err(pharmakon_common::AgentError(format!("Tool execution timed out after {} seconds.", timeout_secs))),
                        };
                        let latency_ms = start.elapsed().as_millis() as u64;

                        // --- Rollback Workspace on High-Risk Tool Failure ---
                        if result.is_err()
                            && let Some((ref dir, ref snap)) = workspace_snapshot {
                                log::warn!("High-risk tool '{}' execution failed. Rolling back workspace...", tool.name());
                                let _ = ss.restore_dir(dir, snap).await;
                            }

                        // --- Snapshot After Mutation ---
                        if let Some(before_id) = snapshot_before_id
                            && result.is_ok()
                                && let Some(path_str) = args["path"].as_str() {
                                    let path = std::path::Path::new(path_str);
                                    if let Ok(after_id) = ss.snapshot_file(path).await {
                                        el.append(&el_session, crate::event_log::EventKind::FileMutated {
                                            path: path_str.to_string(),
                                            snapshot_before_id: before_id,
                                            snapshot_after_id: after_id,
                                        }).await;
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

                        try_send_event!(event_tx, Event::ForensicLog {
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
                        if tool_name == "codeact"
                            && let Ok(args_val) = serde_json::from_str::<serde_json::Value>(&tool_args)
                                && let Some(script) = args_val.get("script").and_then(|s| s.as_str()) {
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

                        if let Some(store) = &self.session_store {
                            if let Err(e) = store.save_tool_metric(&tool_name, success, latency_ms, error).await {
                            log::warn!("Failed to save tool metric: {}", e);
                        }
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

                        if result.contains("### INJECTED PLAYBOOK")
                            && let Some(line) = result.lines().next() {
                                let name = line.replace("### INJECTED PLAYBOOK: ", "").trim().to_string();
                                if let Err(e) = self.register_playbook(session_id, name, result.clone()).await {
                                    log::warn!("Failed to register playbook: {}", e);
                                }
                            }
                        let redacted_result = crate::security::redaction::redact_text(&result);
                        try_send_event!(self.event_tx, Event::ToolResult {
                            result: redacted_result.clone(),
                        });
                        let _ = self.record_step(crate::trajectory::TrajectoryStep::Observation {
                            result: redacted_result.clone(),
                            action_id: None,
                            timestamp: chrono::Utc::now(),
                        }).await;
                        let tool_result_msg = Message {
                            role: "tool".to_string(),
                            name: Some(tool_name),
                            content: Some(MessageContent::Text(redacted_result)),
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
                                if let Err(e) = store.save_message(session_id, &tool_result_msg).await {
                                    log::warn!("Failed to save tool result message: {}", e);
                                }
                            }
                        let mut state = state_arc.lock().await;
                        state.history.push(tool_result_msg);
                    }
                }

                if !tool_errors.is_empty() {
                    // ── Autonomous Recovery Directive ──
                    // Extract suggested alternatives from "Did you mean" errors
                    let mut alternatives: Vec<String> = Vec::new();
                    let mut api_key_missing = false;
                    let mut not_found_tools: Vec<String> = Vec::new();

                    for err in &tool_errors {
                        if let Some(did_you_mean) = err.split("Did you mean one of:").nth(1)
                            && let Some(tools_part) = did_you_mean.split('?').next() {
                                for t in tools_part.split(',') {
                                    let clean = t.trim().trim_matches('`');
                                    if !clean.is_empty() && !alternatives.contains(&clean.to_string()) {
                                        alternatives.push(clean.to_string());
                                    }
                                }
                            }
                        if err.contains("API_KEY not found") || err.contains("API key") {
                            api_key_missing = true;
                        }
                        // Extract the hallucinated tool name
                        if let Some(tool_name) = err.split('\'').nth(1)
                            && !not_found_tools.contains(&tool_name.to_string()) {
                                not_found_tools.push(tool_name.to_string());
                            }
                    }

                    let recovery_msg = if !alternatives.is_empty() {
                        format!(
                            "AUTONOMOUS RECOVERY: Tool failed. Available alternatives you MUST try NOW: {}. Do NOT ask the user — just call the alternative tool immediately with the same parameters.",
                            alternatives.join(", ")
                        )
                    } else if api_key_missing {
                        "AUTONOMOUS RECOVERY: API key missing for the attempted service. Use `search` (DuckDuckGo, free) or `duckduckgo_search` as a fallback. Do NOT give up — try the free alternatives immediately.".to_string()
                    } else if !not_found_tools.is_empty() {
                        format!(
                            "AUTONOMOUS RECOVERY: Tools not found: {}. Use `discover_tools` with a relevant query to find the correct tool, then call it. Do NOT retry the same missing tool name.",
                            not_found_tools.join(", ")
                        )
                    } else {
                        format!(
                            "Tool errors occurred: {}. Try an alternative approach or tool. Do NOT give up.",
                            tool_errors.first().map(|s| s.as_str()).unwrap_or("unknown error")
                        )
                    };

                    let rescue_message = Message {
                        role: "system".to_string(),
                        content: Some(MessageContent::Text(recovery_msg)),
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
            let raw_content = response
                .content
                .as_ref()
                .map(|c| c.to_string())
                .unwrap_or_default();

            if raw_content.trim().is_empty() && response.tool_calls.is_none() {
                log::warn!(
                    "Model returned empty content with no tool calls. Constructing fallback execution acknowledgment."
                );
                
                let last_tool_info = {
                    let state = state_arc.lock().await;
                    state.history.iter().rev()
                        .find(|m| m.role == "tool")
                        .map(|m| (m.name.clone().unwrap_or_else(|| "unknown".to_string()), m.content.as_ref().map(|c| c.to_string()).unwrap_or_default()))
                };

                let fallback = match last_tool_info {
                    Some((name, content)) => {
                        let preview = if content.len() > 150 {
                            format!("{}...", content.chars().take(150).collect::<String>())
                        } else {
                            content
                        };
                        format!("Tool '{}' execution completed. Result preview:\n\n{}\n\n(Note: The model completed its execution path but did not generate further text.)", name, preview)
                    }
                    None => "Tool execution completed successfully, but the model did not generate additional commentary.".to_string()
                };
                
                break Ok(fallback);
            }
            
            log::info!("[SESSION: {}] Processing final content response...", session_id);

            let (final_content, thoughts) = Self::extract_thoughts(&raw_content);

            for thought in &thoughts {
                let _ = self.record_step(crate::trajectory::TrajectoryStep::Thought {
                    content: thought.clone(),
                    timestamp: chrono::Utc::now(),
                }).await;
                try_send_event!(self.event_tx, Event::AgentThought {
                    content: MessageContent::Text(thought.clone()),
                });
                self.event_log.append(session_id, crate::event_log::EventKind::ThoughtEmitted {
                    content_hash: crate::event_log::short_hash(thought),
                }).await;
            }

            let _ = self.record_step(crate::trajectory::TrajectoryStep::Response {
                content: final_content.clone(),
                timestamp: chrono::Utc::now(),
            }).await;

            self.process_final_response(
                &final_content,
                &response,
                user_message,
                session_id,
                &state_arc,
            ).await;

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
                    role: "user".to_string(),
                    content: Some(MessageContent::Text(format!("Context:\n{}", context))),
                    ..Default::default()
                },
            ],
            temperature: Some(0.3),
            max_tokens: Some(200),
            tools: None,
            complexity: None,
            system_instruction: Some(system_prompt.to_string()),
        };

        let model = self.model.lock().await;
        if let Ok(response) = model.complete(request).await
            && let Some(insight) = response.content.as_ref().and_then(|c| c.as_text())
                && !insight.trim().is_empty() {
                    log::info!("Agent Reflection Insight: {}", insight);

                    // Save to fact memory for long-term recall
                    let sess_id = self.session_id.lock().await.clone();
                    let tagged_insight = format!("[Session: {}] {}", sess_id, insight);
                    if let Some(fact_mem) = &self.fact_memory {
                        let mut fm = fact_mem.lock().await;
                        fm.add_belief(&tagged_insight, 0.9, "learned_context")?;
                    }

                    // Also index into semantic search for recovery across sessions
                    if let Some(search) = &self.semantic_search {
                        if let Err(e) = search.remember(&tagged_insight).await {
                            log::warn!("Failed to save reflection insight to search: {}", e);
                        }
                    }
                }
        Ok(())
    }

    pub async fn add_fact(&self, fact: &str) -> Result<()> {
        if let Some(fact_memory) = &self.fact_memory {
            let mut fm = fact_memory.lock().await;
            let sess_id = self.session_id.lock().await.clone();
            let tagged_fact = format!("[Session: {}] {}", sess_id, fact);
            fm.add_belief(&tagged_fact, 0.8, "learned_fact")?;
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
            try_send_event!(self.event_tx, Event::ModelSwitched { model_id: model_id.to_string() });
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

    pub fn with_isolated_knowledge(mut self) -> Self {
        if let Some(ref nexus) = self.knowledge_nexus {
            self.knowledge_nexus = Some(Arc::new(nexus.isolated()));
        }
        if let Some(ref search) = self.semantic_search {
            self.semantic_search = Some(Arc::new(search.isolated()));
        }
        if self.fact_memory.is_some() {
            if let Ok(mut bs) = crate::memory::BeliefSystem::new() {
                bs.set_path(std::env::temp_dir().join(format!("beliefs_isolated_{}.json", uuid::Uuid::new_v4())));
                self.fact_memory = Some(Arc::new(Mutex::new(bs)));
            }
        }
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

        if let Some(nexus) = &self.knowledge_nexus {
            if let Err(e) = nexus.delete_by_session(session_id).await {
                log::warn!("reset_session_history: Failed to clean KnowledgeNexus for session {}: {}", session_id, e);
            }
        }

        if let Some(semantic) = &self.semantic_search {
            if let Err(e) = semantic.delete_by_session(session_id).await {
                log::warn!("reset_session_history: Failed to clean SemanticSearch for session {}: {}", session_id, e);
            }
        }

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
        let state = self.health_monitor.update_state(0);
        self.health_monitor.trigger_auto_remedy_if_needed();
        let report = self.health_monitor.status_report();
        Ok(format!("HEARTBEAT_OK State: {:?}, Detail: {}", state, report))
    }

    pub async fn perform_maintenance(&self) -> Result<()> {
        log::info!("Agent: Performing autonomous maintenance...");
        // Snapshot store cleanup (age-based + quota enforcement)
        if let Err(e) = self.snapshot_store.maintenance().await {
            log::warn!("Agent maintenance: snapshot store cleanup failed: {}", e);
        }
        // SQLite persistence cleanup (old messages, stale telemetry records)
        if let Some(ref store) = self.session_store
            && let Err(e) = store.maintenance().await {
                log::warn!("Agent maintenance: DB cleanup failed: {}", e);
            }
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
        if snapshot_id == SNAPSHOT_DID_NOT_EXIST {
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