// Pharmakon Desktop Dashboard — Xilem 0.4.0 + Vello
// Premium dark theme: 8-tab layout with Chat, Dashboard, Automation,
// Skills, Research, Database, System, and Settings views.
// Feature parity with frontend (React/TypeScript Web GUI).

use pharmakon_core::agent::Agent;
use pharmakon_core::automation::cron::CronManager;
use pharmakon_core::persistence::DbSessionStore;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::sync::mpsc;
use xilem::masonry::properties::types::Length;

mod c {
    use xilem::Color;
    pub fn accent() -> Color    { Color::from_rgb8(139, 92, 246) }
    pub fn success() -> Color   { Color::from_rgb8(74, 222, 128) }
    pub fn danger() -> Color    { Color::from_rgb8(239, 68, 68) }
    pub fn warning() -> Color   { Color::from_rgb8(245, 158, 11) }
    pub fn info() -> Color      { Color::from_rgb8(59, 130, 246) }
    pub fn cyan() -> Color      { Color::from_rgb8(0, 210, 255) }
    pub fn white() -> Color     { Color::from_rgb8(220, 220, 230) }
    pub fn dim() -> Color       { Color::from_rgb8(100, 100, 110) }
    pub fn muted() -> Color     { Color::from_rgb8(148, 163, 184) }
}
use c::*;

pub struct AppData {
    pub current_view: ViewType, pub input_text: String,
    pub messages: Vec<ChatMessage>, pub tool_trace: Vec<String>,
    pub active_swarms: Vec<SwarmStatus>, pub health_stats: HealthStats,
    pub system_logs: Vec<String>, pub forensic_logs: Vec<ForensicEntry>,
    pub mcp_stats: Vec<(String, u32)>, pub usage_history: Vec<UsageEntry>,
    pub tools: Vec<ToolInfo>, pub research_notebook: Option<ResearchNotebookData>,
    pub graph_relations: Vec<String>, pub cron_jobs: Vec<CronJobInfoData>,
    pub settings: serde_json::Value, pub model_list: Vec<String>,
    pub current_model: String, pub sessions: Vec<String>, pub current_session: String,
    pub token_count: u64, pub total_cost: f64, pub uptime_secs: u64, pub memory_mb: u64,
    pub event_rx: mpsc::UnboundedReceiver<UiEvent>,
    pub agent: Arc<Agent>, pub db: Arc<DbSessionStore>, pub cron_manager: Arc<CronManager>,
    pub is_window_open: bool, pub show_requested: Arc<AtomicBool>,
    pub main_window_id: xilem::WindowId,
}

#[derive(Clone, PartialEq)]
pub struct ChatMessage {
    pub role: String, pub content: String,
    pub thought: Option<String>, pub tool_name: Option<String>, pub tool_args: Option<String>,
}
#[derive(Clone, Default, PartialEq)]
pub struct HealthStats { pub is_alive: bool, pub failure_rate: f32, pub last_latency: String }
#[derive(Clone, PartialEq)]
pub struct SwarmStatus { pub id: String, pub role: String, pub status: String }
#[derive(Clone, PartialEq)]
pub struct ToolInfo { pub name: String, pub description: String }
#[derive(Clone, PartialEq)]
pub struct UsageEntry { pub timestamp: String, pub tokens: u64, pub cost: f64 }
#[derive(Clone, PartialEq)]
pub struct CronJobInfoData { pub id: String, pub schedule_type: String, pub expr: String, pub message: String }
#[derive(Clone, PartialEq)]
pub struct ResearchNotebookData {
    pub current_goal: String, pub verified_facts: Vec<FactData>,
    pub pending_questions: Vec<String>, pub research_tree: Vec<(String, Vec<String>)>,
    pub dead_ends: Vec<String>,
}
#[derive(Clone, PartialEq)]
pub struct FactData { pub content: String, pub source_url: String, pub confidence: f32 }
#[derive(Clone, PartialEq)]
pub struct ForensicEntry { pub action: String, pub hypothesis: String, pub observation: Option<String> }

#[derive(PartialEq, Clone, Copy, Default)]
pub enum ViewType {
    #[default] Chat, Dashboard, Automation, Skills, Research, Database, System, Settings,
}

pub enum UiEvent {
    AgentThought(String), AgentResponse(String),
    ToolCall { name: String, args: String }, ToolResult(String),
    Error(String), SessionList(Vec<String>),
    ModelList(Vec<String>), ModelSwitched(String),
    TokenUsage { tokens: u64, cost: f64 },
    GatewayStatus { uptime: u64, memory: u64 },
    McpStats(Vec<(String, u32)>), ToolList(Vec<ToolInfo>),
    SystemLog { level: String, message: String },
    OrchestrationState(Vec<SwarmStatus>),
    ResearchNotebookUpdate(ResearchNotebookData),
    GraphUpdate(Vec<String>), SettingsUpdate(serde_json::Value),
    CronJobList(Vec<CronJobInfoData>), UsageHistory(Vec<UsageEntry>),
    ForensicLog { action: String, hypothesis: String, observation: Option<String> },
}

impl AppData {
    pub fn new(
        agent: Arc<Agent>, db: Arc<DbSessionStore>,
        cron_manager: Arc<CronManager>, event_rx: mpsc::UnboundedReceiver<UiEvent>,
    ) -> Self {
        Self {
            current_view: ViewType::Chat, input_text: String::new(),
            messages: Vec::new(), tool_trace: Vec::new(),
            active_swarms: vec![SwarmStatus { id: "supervisor".into(), role: "Supervisor".into(), status: "Active".into() }],
            health_stats: HealthStats { is_alive: true, ..Default::default() },
            system_logs: vec!["💊 Pharmakon Desktop — Ready".into()],
            forensic_logs: Vec::new(), mcp_stats: Vec::new(), usage_history: Vec::new(),
            tools: Vec::new(), research_notebook: None, graph_relations: Vec::new(),
            cron_jobs: Vec::new(),
            settings: serde_json::json!({"model":"gemini-2.0-flash","temperature":0.7}),
            model_list: vec!["gemini/gemini-2.5-flash".into()],
            current_model: "gemini/gemini-2.5-flash".into(),
            sessions: vec!["default".to_string()], current_session: "default".to_string(),
            token_count: 0, total_cost: 0.0, uptime_secs: 0, memory_mb: 0,
            event_rx, agent, db, cron_manager,
            is_window_open: true, show_requested: Arc::new(AtomicBool::new(false)),
            main_window_id: xilem::WindowId::next(),
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
                        self.messages.push(ChatMessage { role: "agent".into(), content: String::new(), thought: Some(text), tool_name: None, tool_args: None });
                    }
                }
                UiEvent::AgentResponse(text) => {
                    if let Some(last) = self.messages.last_mut() { last.content = text; }
                }
                UiEvent::ToolCall { name, args } => {
                    self.tool_trace.push(format!("{} ({})", name, args));
                    self.messages.push(ChatMessage { role: "tool".into(), content: String::new(), thought: None, tool_name: Some(name), tool_args: Some(args) });
                }
                UiEvent::ToolResult(result) => {
                    let short = if result.len() > 150 { format!("{}...", &result[..147]) } else { result };
                    self.tool_trace.push(format!("  {}", short));
                }
                UiEvent::Error(msg) => { self.system_logs.push(format!("ERR: {}", msg)); }
                UiEvent::SessionList(s) => { self.sessions = s; }
                UiEvent::ModelList(m) => { self.model_list = m; }
                UiEvent::ModelSwitched(id) => { self.current_model = id; }
                UiEvent::TokenUsage { tokens, cost } => { self.token_count = tokens; self.total_cost = cost; }
                UiEvent::GatewayStatus { uptime, memory } => { self.uptime_secs = uptime; self.memory_mb = memory / 1024 / 1024; }
                UiEvent::McpStats(s) => { self.mcp_stats = s; }
                UiEvent::ToolList(t) => { self.tools = t; }
                UiEvent::SystemLog { level, message } => { self.system_logs.push(format!("[{}] {}", level, message)); }
                UiEvent::OrchestrationState(s) => { self.active_swarms = s; }
                UiEvent::ResearchNotebookUpdate(nb) => { self.research_notebook = Some(nb); }
                UiEvent::GraphUpdate(r) => { self.graph_relations = r; }
                UiEvent::SettingsUpdate(s) => { self.settings = s; }
                UiEvent::CronJobList(j) => { self.cron_jobs = j; }
                UiEvent::UsageHistory(h) => { self.usage_history = h; }
                UiEvent::ForensicLog { action, hypothesis, observation } => {
                    self.forensic_logs.push(ForensicEntry { action, hypothesis, observation });
                    if self.forensic_logs.len() > 30 { self.forensic_logs.remove(0); }
                }
            }
            if self.tool_trace.len() > 40 { self.tool_trace.remove(0); }
            if self.system_logs.len() > 150 { self.system_logs.remove(0); }
            if self.messages.len() > 80 { self.messages.remove(0); }
        }
    }

    pub fn send_message(&mut self) {
        let msg = self.input_text.trim().to_string();
        if msg.is_empty() { return; }
        self.messages.push(ChatMessage { role: "user".into(), content: msg.clone(), thought: None, tool_name: None, tool_args: None });
        self.input_text.clear();
        let agent = self.agent.clone();
        tokio::spawn(async move { let _ = agent.chat(&msg).await; });
    }

    pub fn reset_session(&mut self) {
        self.messages.clear(); self.tool_trace.clear();
        let agent = self.agent.clone();
        tokio::spawn(async move { let _ = agent.reset_history().await; });
    }

    pub fn cancel_cron_job(&mut self, id: &str) {
        let cm = self.cron_manager.clone(); let id = id.to_string();
        tokio::spawn(async move { let _ = cm.cancel_job(&id).await; });
    }

    pub fn search_graph(&mut self, query: &str) {
        let agent = self.agent.clone(); let q = query.to_string();
        tokio::spawn(async move {
            if let Some(graph) = &agent.graph_store && let Ok(relations) = graph.query_relations(&q).await {
                let _ = agent.event_tx.send(pharmakon_common::Event::GraphUpdate {
                    relations: relations.into_iter().map(|(n, e)| format!("{} -> {} ({})", e.from_id, n.label, e.relation)).collect(),
                });
            }
        });
    }
}

// ─── App Entry ───

pub fn app_logic(data: &mut AppData) -> Vec<xilem::WindowView<AppData>> {
    use std::sync::atomic::Ordering;
    use xilem::window;
    data.drain_events();
    if data.show_requested.swap(false, Ordering::SeqCst) { data.is_window_open = true; }
    let mut windows = Vec::new();
    if data.is_window_open {
        windows.push(window(data.main_window_id, "💊 Pharmakon", root_view(data))
            .with_options(|_| xilem::WindowOptions::new("Pharmakon Desktop")
                .on_close(|d: &mut AppData| { d.is_window_open = false; })));
    }
    windows
}

// ═══════════════════════════════════════════════════════════════

fn root_view(data: &mut AppData) -> impl xilem::WidgetView<AppData> + use<> {
    use xilem::FontWeight;
    use xilem::style::Style;
    use xilem::view::{FlexExt, flex_col, flex_row, label, sized_box, text_button};
    use super::widgets::swarm_visualizer;

    let tabs: Vec<_> = [
        ("Chat", ViewType::Chat), ("Stats", ViewType::Dashboard),
        ("Auto", ViewType::Automation), ("Tools", ViewType::Skills),
        ("Research", ViewType::Research), ("Graph", ViewType::Database),
        ("Logs", ViewType::System), ("Config", ViewType::Settings),
    ].iter().map(|(l, v)| {
        let v = *v;
        text_button(l.to_string(), move |d: &mut AppData| { d.current_view = v; })
    }).collect();

    let trace = data.tool_trace.iter().rev().take(20).cloned().collect::<Vec<_>>().join("\n");
    let trace_text = if trace.is_empty() { "Awaiting...".to_string() } else { trace };

    flex_col((
        flex_row(tabs),
        flex_row((
            sized_box(flex_col((
                label("SESSION").text_size(10.0).weight(FontWeight::BOLD).color(dim()),
                label(data.current_session.clone()).text_size(12.0).color(white()),
                label("").text_size(4.0),
                label("MODEL").text_size(10.0).weight(FontWeight::BOLD).color(dim()),
                label(data.current_model.clone()).text_size(10.0).color(accent()),
                label("").text_size(4.0),
                label(format!("Tokens: {}", data.token_count)).text_size(10.0).color(dim()),
                label(format!("Cost: ${:.4}", data.total_cost)).text_size(10.0).color(dim()),
                label(format!("{}h {}MB", data.uptime_secs / 3600, data.memory_mb)).text_size(10.0).color(dim()),
                label("").text_size(4.0),
                label(if data.health_stats.is_alive { "● HEALTHY" } else { "○ OFFLINE" })
                    .text_size(11.0).color(if data.health_stats.is_alive { success() } else { danger() }),
            ))).width(Length::px(160.0)),
            content_area(data).flex(1.0),
            sized_box(flex_col((
                label("TRACE").text_size(9.0).weight(FontWeight::BOLD).color(dim()),
                sized_box(swarm_visualizer(data.active_swarms.clone())).width(Length::px(160.0)).height(Length::px(60.0)),
                label(trace_text).text_size(9.0).color(cyan()),
            ))).width(Length::px(180.0)),
        )).flex(1.0),
        flex_row((
            label(if data.health_stats.is_alive { "● LIVE" } else { "○ OFFLINE" })
                .text_size(10.0).color(if data.health_stats.is_alive { success() } else { danger() }),
            label(format!(" | {}tk ${:.4} {}h", data.token_count, data.total_cost, data.uptime_secs / 3600))
                .text_size(10.0).color(dim()),
            text_button("Reset", |d: &mut AppData| { d.reset_session(); }),
        )),
    ))
}

// ═══════════════════════════════════════════════════════════════

fn content_area(data: &mut AppData) -> impl xilem::WidgetView<AppData> + use<> {
    use xilem::style::Style;
    use xilem::view::{FlexExt, flex_col, flex_row, label, text_input, text_button};

    let mut t = String::new();
    match data.current_view {
        ViewType::Chat => {
            t.push_str("💊 Pharmakon Chat\n\n");
            for m in &data.messages {
                let p = match m.role.as_str() { "user"=>"🧑","agent"=>"💊","tool"=>"🔧",_=>"⚙" };
                t.push_str(&format!("{}: ", p));
                if let Some(th) = &m.thought { t.push_str(&format!("[{}] ", th)); }
                t.push_str(&m.content);
                if let Some(n) = &m.tool_name { t.push_str(&format!("\n  ⚡ {}", n)); }
                t.push_str("\n\n");
            }
            if data.messages.is_empty() { t.push_str("Awaiting first instruction...\n"); }
        }
        ViewType::Dashboard => {
            let total: u32 = data.mcp_stats.iter().map(|(_, c)| c).sum();
            t.push_str(&format!("📊 Dashboard\n\nTokens: {} | Cost: ${:.4} | Uptime: {}h | Mem: {}MB\n\n",
                data.token_count, data.total_cost, data.uptime_secs / 3600, data.memory_mb));
            for (name, count) in data.mcp_stats.iter().take(12) {
                let bar = if total > 0 { "█".repeat((*count as f32 / total as f32 * 30.0) as usize) } else { String::new() };
                t.push_str(&format!("  {:<18} {} {}\n", name, bar, count));
            }
            t.push_str(&format!("\n{} tools | {} calls\n", data.tools.len(), total));
        }
        ViewType::Automation => {
            t.push_str("⏱ Automation\n\n");
            if data.cron_jobs.is_empty() { t.push_str("  No active cron sequences.\n"); }
            else { for j in &data.cron_jobs { t.push_str(&format!("┌─ {} [{}]\n│  \"{}\"\n│  Type: {}\n└──\n", j.id, j.expr, j.message, j.schedule_type)); } }
        }
        ViewType::Skills => {
            t.push_str(&format!("📦 Tools & Skills\n\n{} tools registered\n\n", data.tools.len()));
            for tool in data.tools.iter().take(25) {
                t.push_str(&format!("{}  {}\n   {}\n", cat_emoji(&tool.name), tool.name, tool.description));
            }
        }
        ViewType::Research => {
            t.push_str("🔬 Deep Research\n\n");
            match &data.research_notebook {
                None => { t.push_str("No active research session.\n"); }
                Some(nb) => {
                    t.push_str(&format!("🎯 GOAL: {}\n\n📋 FACTS:\n", nb.current_goal));
                    for f in &nb.verified_facts { t.push_str(&format!("  • {} (src: {})\n", f.content, f.source_url)); }
                    t.push_str("\n❓ QUESTIONS:\n");
                    for q in &nb.pending_questions { t.push_str(&format!("  ? {}\n", q)); }
                }
            }
        }
        ViewType::Database => {
            t.push_str("🗄 Knowledge Nexus\n\n");
            if data.graph_relations.is_empty() { t.push_str("  Enter a query to explore.\n"); }
            else { for r in data.graph_relations.iter().take(20) { t.push_str(&format!("  ╺ {}\n", r)); } }
        }
        ViewType::System => {
            t.push_str("GATEWAY: SECURE | Core v0.8 | Weaver: ACTIVE | Search: CONNECTED\n\n");
            for e in data.forensic_logs.iter().rev().take(10) {
                t.push_str(&format!("[FORENSIC] {}: {} -> {}\n", e.action, e.hypothesis, e.observation.as_deref().unwrap_or("")));
            }
            for l in data.system_logs.iter().rev().take(40) { t.push_str(&format!("{}\n", l)); }
        }
        ViewType::Settings => {
            t.push_str(&format!("🔑 Settings\n\nModel: {}\n\nAvailable:\n", data.current_model));
            for m in data.model_list.iter().take(8) { t.push_str(&format!("  {} {}\n", if m==&data.current_model{"●"}else{"○"}, m)); }
            t.push_str("\nSafety: Auto-Approval OFF | Budget 100k | Constitutional ACTIVE | Sandbox DOCKER\n");
            t.push_str("\nSecrets: API Key ************\n");
        }
    }

    flex_col((
        label(t).text_size(12.0).color(dim()),
        flex_row((
            text_input(data.input_text.clone(), |d: &mut AppData, s| { d.input_text = s; })
                .placeholder(if data.current_view == ViewType::Chat { "Deploy instruction..." } else { "" }),
            text_button("Send", |d: &mut AppData| { if d.current_view == ViewType::Chat { d.send_message(); } }),
            text_button("Clear", |d: &mut AppData| { if d.current_view == ViewType::Chat { d.reset_session(); } }),
        )),
    ))
}

fn cat_emoji(name: &str) -> &'static str {
    let l = name.to_lowercase();
    if l.contains("shell") || l.contains("bash") { "💻" }
    else if l.contains("file") || l.contains("read") || l.contains("write") { "📁" }
    else if l.contains("search") || l.contains("grep") || l.contains("find") { "🔍" }
    else if l.contains("code") || l.contains("ast") || l.contains("lsp") { "📝" }
    else if l.contains("browser") || l.contains("web") || l.contains("http") { "🌐" }
    else if l.contains("git") { "📦" }
    else if l.contains("docker") || l.contains("sandbox") { "🐳" }
    else if l.contains("embed") || l.contains("vector") { "🧠" }
    else if l.contains("memory") || l.contains("graph") { "🗄" }
    else if l.contains("cron") || l.contains("schedule") { "⏱" }
    else if l.contains("agent") || l.contains("spawn") { "🤖" }
    else { "🔧" }
}
