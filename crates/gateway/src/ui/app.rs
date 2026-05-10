// Pharmakon IDE — egui-based embedded GUI
// Cursor-style: file tree + code viewer + agent terminal + status bar
// Lightweight companion to the WebUI dashboard (localhost:19999)

use pharmakon_core::agent::Agent;
use pharmakon_core::automation::cron::CronManager;
use pharmakon_core::persistence::DbSessionStore;
use std::sync::Arc;
use tokio::sync::mpsc;

// ─── App State ───

pub struct AppData {
    pub input_text: String,
    pub messages: Vec<ChatMessage>,
    pub tool_trace: Vec<String>,
    pub system_logs: Vec<String>,
    pub model_list: Vec<String>,
    pub current_model: String,
    pub token_count: u64,
    pub total_cost: f64,
    pub uptime_secs: u64,
    pub memory_mb: u64,
    pub tools: Vec<ToolInfo>,
    pub health_stats: HealthStats,
    pub active_swarms: Vec<SwarmStatus>,
    pub cron_jobs: Vec<CronJobInfoData>,
pub graph_relations: Vec<String>,
    pub cognitive_timeline: Vec<TimelineEvent>,
    pub plan_dag: Vec<DagNode>,
    pub event_rx: mpsc::UnboundedReceiver<UiEvent>,
    pub agent: Arc<Agent>,
    pub db: Arc<DbSessionStore>,
    pub cron_manager: Arc<CronManager>,
    // File tree state
    pub file_tree: Vec<String>,
    pub selected_file: Option<String>,
    pub file_content: String,
    pub workspace_root: String,
    pub last_snapshot_id: Option<String>,
    pub pending_approval: Option<(String, String, String)>,
}

#[derive(Clone, PartialEq)]
pub struct ChatMessage {
    pub role: String, pub content: String,
    pub thought: Option<String>, pub tool_name: Option<String>,
}

#[derive(Clone, Default, PartialEq)]
pub struct HealthStats { pub is_alive: bool, pub failure_rate: f32, pub last_latency: String }

#[derive(Clone, PartialEq)]
pub struct ToolInfo { pub name: String, pub description: String }

#[derive(Clone, PartialEq)]
pub struct SwarmStatus { pub id: String, pub role: String, pub status: String }

#[derive(Clone, PartialEq)]
pub struct TimelineEvent { pub timestamp: String, pub event: String, pub kind: TimelineKind }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimelineKind { Plan, Verify, Execute, Fail, Rollback, Gate }
#[derive(Clone, PartialEq)]
pub struct DagNode { pub id: String, pub label: String, pub status: DagStatus, pub children: Vec<usize> }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DagStatus { Pending, Running, Success, Failed, Gated }

pub struct CronJobInfoData { pub id: String, pub schedule_type: String, pub expr: String, pub message: String }

pub enum UiEvent {
    AgentThought(String), AgentResponse(String),
    ToolCall { name: String, args: String }, ToolResult(String),
    Error(String), ModelList(Vec<String>), ModelSwitched(String),
    TokenUsage { tokens: u64, cost: f64 },
    GatewayStatus { uptime: u64, memory: u64 },
    McpStats(Vec<(String, u32)>), ToolList(Vec<ToolInfo>),
    SystemLog { level: String, message: String },
    OrchestrationState(Vec<SwarmStatus>),
    GraphUpdate(Vec<String>), SettingsUpdate(serde_json::Value),
    CronJobList(Vec<CronJobInfoData>),
    PlanDagUpdate(Vec<DagNode>),
    TimelineEvent(TimelineEvent),
    SnapshotCreated(String),
    ApprovalRequest { id: String, tool: String, args: String },
    ApprovalResolved(String),
}

impl AppData {
    pub fn new(
        agent: Arc<Agent>, db: Arc<DbSessionStore>,
        cron_manager: Arc<CronManager>, event_rx: mpsc::UnboundedReceiver<UiEvent>,
    ) -> Self {
        let ws = std::env::current_dir().unwrap_or_default();
        let mut file_tree = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&ws) {
            for e in entries.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                let prefix = if e.path().is_dir() { "📁 " } else { "📄 " };
                file_tree.push(format!("{}{}", prefix, n));
            }
        }
        Self {
            input_text: String::new(), messages: Vec::new(), tool_trace: Vec::new(),
            system_logs: vec!["💊 Pharmakon IDE — Ready".into()],
            model_list: vec!["gemini/gemini-2.5-flash".into()],
            current_model: "gemini/gemini-2.5-flash".into(),
            token_count: 0, total_cost: 0.0, uptime_secs: 0, memory_mb: 0,
            tools: Vec::new(), health_stats: HealthStats { is_alive: true, ..Default::default() },
            active_swarms: vec![SwarmStatus { id: "supervisor".into(), role: "Supervisor".into(), status: "Active".into() }],
            cron_jobs: Vec::new(), graph_relations: Vec::new(),
            cognitive_timeline: vec![TimelineEvent { timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(), event: "IDE started".into(), kind: TimelineKind::Plan }],
            plan_dag: Vec::new(),
            event_rx, agent, db, cron_manager,
            file_tree, selected_file: None, file_content: String::new(),
            workspace_root: ws.to_string_lossy().to_string(),
            last_snapshot_id: None,
            pending_approval: None,
        }
    }

    pub fn drain_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                UiEvent::AgentThought(text) => {
                    if let Some(last) = self.messages.last_mut() && last.role == "agent" {
                        let t = last.thought.take().unwrap_or_default();
                        last.thought = Some(format!("{}{}", t, text));
                    } else {
                        self.messages.push(ChatMessage { role: "agent".into(), content: String::new(), thought: Some(text), tool_name: None });
                    }
                }
                UiEvent::AgentResponse(text) => {
                    if let Some(last) = self.messages.last_mut() { last.content = text; }
                }
                UiEvent::ToolCall { name, args } => {
                    self.tool_trace.push(format!("⚡ {} {}", name, args));
                    if self.tool_trace.len() > 100 { self.tool_trace.remove(0); }
                }
                UiEvent::ToolResult(r) => {
                    let short = if r.len() > 120 { format!("{}...", &r[..117]) } else { r };
                    self.tool_trace.push(format!("  └─ {}", short));
                }
                UiEvent::Error(msg) => { self.system_logs.push(format!("ERR: {}", msg)); }
                UiEvent::ModelList(m) => { self.model_list = m; }
                UiEvent::ModelSwitched(id) => { self.current_model = id; }
                UiEvent::TokenUsage { tokens, cost } => { self.token_count = tokens; self.total_cost = cost; }
                UiEvent::GatewayStatus { uptime, memory } => { self.uptime_secs = uptime; self.memory_mb = memory / 1024 / 1024; }
                UiEvent::McpStats(_s) => {}
                UiEvent::ToolList(t) => { self.tools = t; }
                UiEvent::SystemLog { level, message } => {
                    self.system_logs.push(format!("[{}] {}", level, message));
                }
                UiEvent::OrchestrationState(s) => { self.active_swarms = s; }
                UiEvent::GraphUpdate(r) => { self.graph_relations = r; }
                UiEvent::SettingsUpdate(_) => {}
                UiEvent::CronJobList(j) => { self.cron_jobs = j; }
                UiEvent::PlanDagUpdate(dag) => { self.plan_dag = dag; }
                UiEvent::TimelineEvent(ev) => { self.cognitive_timeline.push(ev); if self.cognitive_timeline.len() > 50 { self.cognitive_timeline.remove(0); } }
                UiEvent::SnapshotCreated(id) => { self.last_snapshot_id = Some(id); }
                UiEvent::ApprovalRequest { id, tool, args } => {
                    self.pending_approval = Some((id, tool, args));
                }
                UiEvent::ApprovalResolved(id) => {
                    if self.pending_approval.as_ref().map(|x| &x.0) == Some(&id) {
                        self.pending_approval = None;
                    }
                }
            }
            if self.system_logs.len() > 200 { self.system_logs.remove(0); }
            if self.messages.len() > 50 { self.messages.remove(0); }
        }
    }

    pub fn send_message(&mut self) {
        let msg = self.input_text.trim().to_string();
        if msg.is_empty() { return; }
        self.messages.push(ChatMessage { role: "user".into(), content: msg.clone(), thought: None, tool_name: None });
        self.input_text.clear();
        let agent = self.agent.clone();
        tokio::spawn(async move { let _ = agent.chat(&msg).await; });
    }

    pub fn open_file(&mut self, path: &str) {
        self.selected_file = Some(path.to_string());
        if let Ok(content) = std::fs::read_to_string(path) {
            self.file_content = content;
        }
    }

    
    pub fn rollback(&mut self) {
        if let Some(ref snap_id) = self.last_snapshot_id.clone() {
            let sid = snap_id.clone();
            self.system_logs.push(format!("Rolling back to snapshot {}", &sid[..sid.len().min(8)]));
            let agent = self.agent.clone();
            tokio::spawn(async move {
                let events = agent.event_log.events_since(0u64).await; if !events.is_empty() {
                    let last_event_id = events.last().unwrap().id;
                    let _ = agent.rollback_to_event(last_event_id).await;
                }
            });
            let ev = TimelineEvent { timestamp: chrono::Utc::now().format("%H:%M:%S").to_string(), event: format!("Rollback → {}", &sid[..sid.len().min(8)]), kind: TimelineKind::Rollback };
            self.cognitive_timeline.push(ev);
        }
    }

    pub fn resolve_approval(&mut self, approved: bool) {
        if let Some((id, _, _)) = self.pending_approval.take() {
            self.agent.approve(id, approved);
        }
    }

    pub fn refresh_file_tree(&mut self) {
        self.file_tree.clear();
        if let Ok(entries) = std::fs::read_dir(&self.workspace_root) {
            for e in entries.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                let prefix = if e.path().is_dir() { "📁 " } else { "📄 " };
                self.file_tree.push(format!("{}{}", prefix, n));
            }
        }
    }
}
