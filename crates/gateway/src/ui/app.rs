use pharmakon_core::agent::Agent;
use pharmakon_core::automation::cron::CronManager;
use pharmakon_core::persistence::DbSessionStore;
use std::sync::Arc;
use tokio::sync::mpsc;
use std::path::{Path, PathBuf};
use syntect::parsing::SyntaxSet;
use syntect::highlighting::ThemeSet;

// ─── App State ───

#[derive(Clone, Debug, PartialEq)]
pub struct FileNode {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub children: Vec<FileNode>,
    pub expanded: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DiffLine {
    Unchanged { text: String, line_no: usize },
    Added { text: String, line_no: usize },
    Removed { text: String, line_no: usize },
}

#[derive(Clone, Debug, PartialEq)]
pub struct InlineSuggestion {
    pub ghost_text: String,
    pub position: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TerminalLine {
    pub text: String,
    pub is_input: bool,
    pub timestamp: String,
}

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
    // File tree state (hierarchical)
    pub file_tree_nodes: Vec<FileNode>,
    pub selected_file: Option<String>,
    pub file_content: String,
    pub workspace_root: String,
    pub last_snapshot_id: Option<String>,
    pub pending_approval: Option<(String, String, String)>,
    
    // Tab, Diff, Syntax and Suggestion state
    pub open_tabs: Vec<String>,
    pub active_tab_index: Option<usize>,
    pub original_content: String,
    pub diff_preview_mode: bool,
    pub show_save_confirm_dialog: bool,
    pub diff_lines: Vec<DiffLine>,
    pub inline_suggestion: Option<InlineSuggestion>,
    pub terminal_lines: Vec<TerminalLine>,
    pub terminal_input: String,
    pub syntax_set: SyntaxSet,
    pub theme_set: ThemeSet,
    pub event_tx: mpsc::UnboundedSender<UiEvent>,
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
    TerminalOutput { text: String, is_input: bool },
    FileTreeLoaded(Vec<FileNode>),
}


fn build_tree(root: &Path, gi: Option<&gitignore::File>, depth: usize) -> Vec<FileNode> {
    if depth > 10 { return Vec::new(); }
    let mut nodes = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for e in entries.flatten() {
            let path = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            
            // Skip target, node_modules, .git
            if name == ".git" || name == "target" || name == "node_modules" {
                continue;
            }
            
            if let Some(gi_file) = gi {
                if gi_file.is_excluded(&path).unwrap_or(false) {
                    continue;
                }
            }
            
            let is_dir = path.is_dir();
            let mut children = Vec::new();
            if is_dir {
                children = build_tree(&path, gi, depth + 1);
            }
            
            nodes.push(FileNode {
                name,
                path,
                is_dir,
                children,
                expanded: false,
            });
        }
    }
    // Sort nodes: directories first, then files alphabetically
    nodes.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            b.is_dir.cmp(&a.is_dir)
        } else {
            a.name.cmp(&b.name)
        }
    });
    nodes
}

impl AppData {
    pub fn new(
        agent: Arc<Agent>, db: Arc<DbSessionStore>,
        cron_manager: Arc<CronManager>, event_rx: mpsc::UnboundedReceiver<UiEvent>,
        event_tx: mpsc::UnboundedSender<UiEvent>,
    ) -> Self {
        let ws = std::env::current_dir().unwrap_or_default();
        let ws_str = ws.to_string_lossy().to_string();
        
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();
        
        // Spawn file tree building in background to avoid blocking GUI thread at startup
        let tx = event_tx.clone();
        let ws_clone = ws.clone();
        tokio::spawn(async move {
            let gitignore_path = ws_clone.join(".gitignore");
            let gi = gitignore::File::new(&gitignore_path).ok();
            let nodes = build_tree(&ws_clone, gi.as_ref(), 0);
            let _ = tx.send(UiEvent::FileTreeLoaded(nodes));
        });
        
        let file_tree_nodes = Vec::new(); // empty initially, loaded by background task

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
            event_rx, event_tx, agent, db, cron_manager,
            file_tree_nodes, selected_file: None, file_content: String::new(),
            workspace_root: ws_str,
            last_snapshot_id: None,
            pending_approval: None,
            open_tabs: Vec::new(),
            active_tab_index: None,
            original_content: String::new(),
            diff_preview_mode: false,
            show_save_confirm_dialog: false,
            diff_lines: Vec::new(),
            inline_suggestion: None,
            terminal_lines: Vec::new(),
            terminal_input: String::new(),
            syntax_set,
            theme_set,
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
                UiEvent::TerminalOutput { text, is_input } => {
                    let timestamp = chrono::Utc::now().format("%H:%M:%S").to_string();
                    self.terminal_lines.push(TerminalLine {
                        text,
                        is_input,
                        timestamp,
                    });
                    if self.terminal_lines.len() > 200 {
                        self.terminal_lines.remove(0);
                    }
                }
                UiEvent::FileTreeLoaded(nodes) => {
                    self.file_tree_nodes = nodes;
                    self.system_logs.push("📂 Workspace file tree loaded successfully".into());
                }
            }
            if self.system_logs.len() > 200 { self.system_logs.remove(0); }
            if self.messages.len() > 50 { self.messages.remove(0); }
        }
    }

    pub fn send_message(&mut self) {
        let msg = self.input_text.trim().to_string();
        if msg.is_empty() { return; }
        // Prepend workspace context so the agent knows which directory to operate on
        let workspace_hint = format!("[Current workspace: {}]\n{}", self.workspace_root, msg);
        self.messages.push(ChatMessage { role: "user".into(), content: workspace_hint.clone(), thought: None, tool_name: None });
        self.input_text.clear();
        let agent = self.agent.clone();
        tokio::spawn(async move { let _ = agent.chat(&workspace_hint).await; });
    }

    pub fn open_file(&mut self, path: &str) {
        let path_str = path.to_string();
        self.selected_file = Some(path_str.clone());
        
        // Add to tabs if not already open
        if !self.open_tabs.contains(&path_str) {
            self.open_tabs.push(path_str.clone());
        }
        self.active_tab_index = self.open_tabs.iter().position(|t| t == &path_str);
        
        if let Ok(content) = std::fs::read_to_string(path) {
            self.file_content = content.clone();
            self.original_content = content;
        }
        
        self.diff_preview_mode = false;
        self.diff_lines.clear();
        self.inline_suggestion = None;
    }

    pub fn close_tab(&mut self, index: usize) {
        if index < self.open_tabs.len() {
            let _removed = self.open_tabs.remove(index);
            if self.open_tabs.is_empty() {
                self.selected_file = None;
                self.file_content.clear();
                self.original_content.clear();
                self.active_tab_index = None;
            } else {
                let new_idx = if index >= self.open_tabs.len() {
                    self.open_tabs.len() - 1
                } else {
                    index
                };
                self.active_tab_index = Some(new_idx);
                let next_file = self.open_tabs[new_idx].clone();
                self.open_file(&next_file);
            }
        }
    }

    pub fn compute_diff(&mut self) {
        if let Some(ref _path) = self.selected_file {
            self.diff_lines.clear();
            
            let mut options = diffy::DiffOptions::new();
            options.set_context_len(100000); // include everything in context
            let patch = options.create_patch(&self.original_content, &self.file_content);
            let patch_str = patch.to_string();
            
            let mut line_no_orig = 1;
            let mut line_no_curr = 1;
            
            for line in patch_str.lines() {
                if line.starts_with("---") || line.starts_with("+++") || line.starts_with("@@") {
                    continue;
                }
                
                if line.starts_with('+') {
                    self.diff_lines.push(DiffLine::Added {
                        text: line[1..].to_string(),
                        line_no: line_no_curr,
                    });
                    line_no_curr += 1;
                } else if line.starts_with('-') {
                    self.diff_lines.push(DiffLine::Removed {
                        text: line[1..].to_string(),
                        line_no: line_no_orig,
                    });
                    line_no_orig += 1;
                } else if line.starts_with(' ') {
                    self.diff_lines.push(DiffLine::Unchanged {
                        text: line[1..].to_string(),
                        line_no: line_no_curr,
                    });
                    line_no_orig += 1;
                    line_no_curr += 1;
                } else if line.is_empty() {
                    self.diff_lines.push(DiffLine::Unchanged {
                        text: String::new(),
                        line_no: line_no_curr,
                    });
                    line_no_orig += 1;
                    line_no_curr += 1;
                }
            }
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
        let ws_path = Path::new(&self.workspace_root).to_path_buf();
        let tx = self.event_tx.clone();
        self.system_logs.push("Refreshing workspace tree...".into());
        tokio::spawn(async move {
            let gitignore_path = ws_path.join(".gitignore");
            let gi = gitignore::File::new(&gitignore_path).ok();
            let nodes = build_tree(&ws_path, gi.as_ref(), 0);
            let _ = tx.send(UiEvent::FileTreeLoaded(nodes));
        });
    }
}

