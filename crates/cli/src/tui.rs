use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use pharmakon_common::Event;
use pharmakon_core::agent::Agent;
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Alignment},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Gauge, Table, Row, Cell, Wrap},
    style::{Color, Style, Modifier, Stylize},
    text::{Span, Line, Text},
};
use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Detailed entry for tool execution logging.
#[allow(dead_code)]
struct DetailedToolLog {
    name: String,
    args: String,
    status: &'static str, // "RUNNING", "SUCCESS", "FAILED", "DENIED"
    result: String,
    latency_ms: Option<u64>,
    timestamp: chrono::DateTime<chrono::Local>,
    start_time: Instant,
}

/// Local state representing a pending user authorization request.
struct LocalApproval {
    id: String,
    tool: String,
    args: String,
}

/// Run the enhanced multi-pane Dashboard TUI for Pharmakon.
pub async fn run_tui(agent: Arc<Agent>, initial_message: Option<String>) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Subscribe to agent event loop
    let mut event_rx = agent.event_tx.subscribe();

    // Console state
    let mut messages: Vec<(String, String)> = Vec::new(); // (role, content)
    let mut tool_trace: Vec<String> = Vec::new();
    let mut detailed_tool_logs: Vec<DetailedToolLog> = Vec::new();
    let mut pending_approvals: Vec<LocalApproval> = Vec::new();
    let mut input_buffer = String::new();
    
    let active_model_init = {
        let m = agent.model.lock().await;
        m.name().to_string()
    };
    let mut active_model = active_model_init;
    let mut status_line = format!("🟢 System operational. Model: {}. Tab/1-4 to change views. Ctrl+C to quit.", active_model);

    let mut active_chat_task: Option<tokio::task::JoinHandle<()>> = None;

    // Navigation state
    let mut active_tab = 0;
    let tabs = vec![" 💬 CONSOLE ", " 🛡️  APPROVALS ", " 🧠 COGNITIVE MATRIX ", " 📊 TELEMETRY "];

    // Scroll Offsets
    let mut chat_scroll = 0;
    let mut rules_scroll = 0;

    // Send initial prompt if given
    if let Some(msg) = initial_message {
        messages.push(("user".to_string(), msg.clone()));
        let agent_clone = agent.clone();
        active_chat_task = Some(tokio::spawn(async move {
            if let Err(e) = agent_clone.chat(&msg).await {
                log::error!("Agent error: {}", e);
            }
        }));
    }

    loop {
        // Safe check and release of completed background chat task
        if let Some(ref handle) = active_chat_task {
            if handle.is_finished() {
                active_chat_task = None;
            }
        }

        // Drain events from agent and map to local states
        while let Ok(event) = event_rx.try_recv() {
            match event {
                Event::AgentThought { content } => {
                    let text = content.to_string();
                    tool_trace.push(format!("💭 Thought: {}", text));
                }
                Event::AgentThoughtChunk { chunk, .. } => {
                    if let Some(last) = tool_trace.last_mut() {
                        if last.starts_with("💭") {
                            last.push_str(&chunk);
                        } else {
                            tool_trace.push(format!("💭 Thought: {}", chunk));
                        }
                    } else {
                        tool_trace.push(format!("💭 Thought: {}", chunk));
                    }
                }
                Event::AgentResponse { content } => {
                    messages.push(("assistant".to_string(), content.to_string()));
                    status_line = "🟢 Ready.".to_string();
                }
                Event::AgentResponseChunk { chunk, .. } => {
                    if let Some(last) = messages.last_mut() {
                        if last.0 == "assistant" {
                            last.1.push_str(&chunk);
                        } else {
                            messages.push(("assistant".to_string(), chunk));
                        }
                    } else {
                        messages.push(("assistant".to_string(), chunk));
                    }
                }
                Event::ToolCall { name, args } => {
                    let args_str = serde_json::to_string_pretty(&args).unwrap_or_default();
                    let display_args = if args_str.len() > 60 {
                        format!("{}...", &args_str[..57])
                    } else {
                        args_str.clone()
                    };
                    
                    tool_trace.push(format!("🔧 Exec: {} ({})", name, display_args));
                    status_line = format!("⚙ Running: {}...", name);

                    // Add to detailed log history
                    detailed_tool_logs.push(DetailedToolLog {
                        name: name.clone(),
                        args: args_str,
                        status: "RUNNING",
                        result: String::new(),
                        latency_ms: None,
                        timestamp: chrono::Local::now(),
                        start_time: Instant::now(),
                    });
                }
                Event::ToolResult { result } => {
                    let display = if result.len() > 80 {
                        format!("{}...", &result[..77])
                    } else {
                        result.clone()
                    };
                    tool_trace.push(format!("  ✓ Result: {}", display));
                    status_line = "🟢 Idle.".to_string();

                    // Update corresponding detailed tool log
                    if let Some(entry) = detailed_tool_logs.iter_mut().rev().find(|e| e.status == "RUNNING") {
                        entry.status = if result.to_lowercase().contains("error") || result.to_lowercase().contains("failed") {
                            "FAILED"
                        } else {
                            "SUCCESS"
                        };
                        entry.result = result;
                        entry.latency_ms = Some(entry.start_time.elapsed().as_millis() as u64);
                    }
                }
                Event::ApprovalRequest { id, tool, args } => {
                    let args_str = serde_json::to_string_pretty(&args).unwrap_or_default();
                    tool_trace.push(format!("🛡 Blocked: {} needs authorization [id: {}]", tool, id));
                    status_line = "🛡 Waiting for operator authorization...".to_string();

                    // Track pending approval
                    pending_approvals.push(LocalApproval {
                        id,
                        tool,
                        args: args_str,
                    });

                    // Force transition to Approval tab for immediate action
                    active_tab = 1;
                }
                Event::Error { message } => {
                    tool_trace.push(format!("❌ Error: {}", message));
                    status_line = "🔴 Error encountered.".to_string();
                }
                Event::AgentHangDetected { reason } => {
                    tool_trace.push(format!("⏱ Stall detected: {}", reason));
                    status_line = "🟡 Cognitive stall warning!".to_string();
                }
                Event::TokenUsageUpdate { total_tokens, total_cost } => {
                    // Handled automatically through direct Agent economy state queries,
                    // but kept as log tracer
                    log::debug!("Telemetry update: tokens={}, cost={}", total_tokens, total_cost);
                }
                Event::ModelSwitched { model_id } => {
                    active_model = model_id.clone();
                    tool_trace.push(format!("🔄 Active model switched to: {}", model_id));
                    status_line = format!("🟢 Active model updated: {}", model_id);
                    messages.push(("system".to_string(), format!("🔄 Model successfully switched to: {}", model_id)));
                }
                _ => {}
            }
        }

        // Draw operator terminal frame
        terminal.draw(|f| {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Tab navigation
                    Constraint::Min(0),    // Main workspace
                    Constraint::Length(1), // Interactive Status bar
                ].as_ref())
                .split(f.area());

            // 1. Render Top Tab Navigation Bar
            let tab_style = Style::default().fg(Color::DarkGray);
            let active_tab_style = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
            let block = Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(Color::Rgb(40, 40, 50)));
            
            let tabs_widget = Tabs::new(tabs.iter().map(|t| Span::raw(*t)).collect::<Vec<_>>())
                .block(block)
                .select(active_tab)
                .style(tab_style)
                .highlight_style(active_tab_style)
                .divider(Span::raw("│"));
            f.render_widget(tabs_widget, layout[0]);

            // 2. Render Main View depending on active Tab
            match active_tab {
                0 => {
                    // CONSOLE TAB: Split Chat & Live tool activity
                    let inner_layout = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)].as_ref())
                        .split(layout[1]);

                    // Chat layout (Left)
                    let chat_layout = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Min(0), Constraint::Length(3)].as_ref())
                        .split(inner_layout[0]);

                    // Render Chat Scroll window
                    let mut display_msgs = Vec::new();
                    for (role, content) in &messages {
                        let (prefix, color) = match role.as_str() {
                            "user" => ("🧑 OPERATOR │ ", Color::Green),
                            "assistant" => ("💊 PHARMAKON │ ", Color::Cyan),
                            "system" => ("⚙ SYSTEM   │ ", Color::Yellow),
                            _ => ("   INFO     │ ", Color::DarkGray),
                        };
                        let style = Style::default().fg(color);
                        
                        let lines: Vec<&str> = content.split('\n').collect();
                        if let Some(first_line) = lines.first() {
                            display_msgs.push(ListItem::new(format!("{}{}", prefix, first_line)).style(style));
                        }
                        for line in lines.iter().skip(1) {
                            display_msgs.push(ListItem::new(format!("              │ {}", line)).style(style));
                        }
                        display_msgs.push(ListItem::new("")); // Spacer between messages
                    }

                    // Implement chat scrolling limit safely
                    let sliced_msgs = if display_msgs.len() > chat_scroll {
                        display_msgs[chat_scroll..].to_vec()
                    } else {
                        display_msgs
                    };

                    let chat_block = List::new(sliced_msgs)
                        .block(Block::default()
                            .title(Span::styled(" Cognitive Stream ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)))
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Rgb(60, 60, 80)))
                        );
                    f.render_widget(chat_block, chat_layout[0]);

                    // Message input field
                    let prompt_prefix = if input_buffer.starts_with('/') { "❯/ " } else { "❯  " };
                    let input_display_text = format!("{}{}", prompt_prefix, input_buffer);
                    
                    let (border_color, box_title) = if active_chat_task.is_some() {
                        (
                            Color::Yellow,
                            Span::styled(" ⏳ Active Process in Progress... Press ESC to cancel and type prompt ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                        )
                    } else if input_buffer.starts_with('/') {
                        (
                            Color::Magenta,
                            Span::styled(" Command Mode (Enter to run command) ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD))
                        )
                    } else {
                        (
                            Color::Green,
                            Span::styled(" Interactive Chat (Enter to send message) ", Style::default().fg(Color::Green))
                        )
                    };

                    let input = Paragraph::new(input_display_text)
                        .block(Block::default()
                            .borders(Borders::ALL)
                            .title(box_title)
                            .border_style(Style::default().fg(border_color))
                        )
                        .style(Style::default().fg(Color::White));
                    f.render_widget(input, chat_layout[1]);

                    // Render Live Tool Activity (Right column) - uses non-blocking `active_model`
                    let trace_block = List::new(
                        tool_trace
                            .iter()
                            .rev()
                            .take(100)
                            .map(|s| {
                                let style = if s.starts_with("💭") {
                                    Style::default().fg(Color::Rgb(150, 150, 180)).add_modifier(Modifier::ITALIC)
                                } else if s.contains("❌") {
                                    Style::default().fg(Color::Red)
                                } else if s.contains("🛡") {
                                    Style::default().fg(Color::LightYellow).add_modifier(Modifier::BOLD)
                                } else {
                                    Style::default().fg(Color::Magenta)
                                };
                                ListItem::new(s.as_str()).style(style)
                            })
                            .collect::<Vec<_>>()
                    )
                    .block(Block::default()
                        .title(Span::styled(format!(" Active Process (Model: {}) ", active_model), Style::default().fg(Color::Magenta)))
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Rgb(60, 60, 80)))
                    );
                    f.render_widget(trace_block, inner_layout[1]);
                }
                1 => {
                    // APPROVALS TAB
                    let list_block = Block::default()
                        .title(Span::styled(" Authorizations Queue ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)))
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Rgb(80, 80, 50)));

                    if pending_approvals.is_empty() {
                        let no_approval_txt = Text::from(vec![
                            Line::from(""),
                            Line::from("  🛡️  SECURE ENCLAVE STATUS: DEPLOYED & HEALTHY"),
                            Line::from("  No tools are currently gated or blocked waiting for authorization."),
                            Line::from(""),
                            Line::from("  High-risk actions (e.g., shell executions, file mutations outside secure paths)"),
                            Line::from("  will prompt for live operator authorization here in real-time."),
                        ]);
                        let p = Paragraph::new(no_approval_txt)
                            .block(list_block)
                            .style(Style::default().fg(Color::DarkGray))
                            .alignment(Alignment::Center);
                        f.render_widget(p, layout[1]);
                    } else {
                        // Display active pending authorization with warning
                        let active = &pending_approvals[0];
                        let warn_style = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
                        let approval_content = vec![
                            Line::from(""),
                            Line::from(Span::styled("  ⚠️  CRITICAL OPERATOR ACCESS AUTH REQUIRED", warn_style)),
                            Line::from("  An autonomous agent is attempting a system command policy-gated for safety."),
                            Line::from(""),
                            Line::from(format!("  • Tool Name:     {}", active.tool)),
                            Line::from(format!("  • Request ID:    {}", active.id)),
                            Line::from("  • Proposed Arguments:"),
                            Line::from(""),
                        ];
                        
                        let arg_lines: Vec<Line> = active.args.lines().map(|l| Line::from(format!("    {}", l))).collect();
                        
                        let mut final_text = Text::from(approval_content);
                        for line in arg_lines {
                            final_text.lines.push(line);
                        }
                        
                        final_text.lines.push(Line::from(""));
                        final_text.lines.push(Line::from(Span::styled("  ────────────────────────────────────────────────────────", Style::default().fg(Color::DarkGray))));
                        final_text.lines.push(Line::from("  ACTION KEYSTROKES:"));
                        final_text.lines.push(Line::from(vec![
                            Span::styled("    [Ctrl + A] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                            Span::raw("Authorize and Execute Action"),
                        ]));
                        final_text.lines.push(Line::from(vec![
                            Span::styled("    [Ctrl + X] ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                            Span::raw("Reject and Terminate Action"),
                        ]));

                        let p = Paragraph::new(final_text)
                            .block(list_block)
                            .style(Style::default().fg(Color::White))
                            .wrap(Wrap { trim: false });
                        f.render_widget(p, layout[1]);
                    }
                }
                2 => {
                    // COGNITIVE MATRIX TAB
                    let matrix_layout = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
                        .split(layout[1]);

                    // Soul metadata (Left column)
                    let get_soul_fut = async {
                        let pm = agent.prompt_manager.lock().await;
                        pm.soul().clone()
                    };
                    let soul = tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(get_soul_fut)
                    });

                    let mut soul_lines = vec![
                        Line::from(""),
                        Line::from(vec![
                            Span::styled("  Identity Profile: ", Style::default().fg(Color::DarkGray)),
                            Span::styled(&soul.name, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                        ]),
                        Line::from(vec![
                            Span::styled("  Active Version:   ", Style::default().fg(Color::DarkGray)),
                            Span::raw(&soul.version),
                        ]),
                        Line::from(vec![
                            Span::styled("  Author Credit:    ", Style::default().fg(Color::DarkGray)),
                            Span::raw(&soul.author),
                        ]),
                        Line::from(""),
                        Line::from("  Identity Attributes:"),
                    ];

                    for t in &soul.traits {
                        soul_lines.push(Line::from(format!("    ▪ {}", t).fg(Color::Magenta)));
                    }

                    soul_lines.push(Line::from(""));
                    soul_lines.push(Line::from("  Loaded Sub-agent Controls:"));
                    soul_lines.push(Line::from(format!("    - Default Temp:   {}", soul.temperature_override.unwrap_or(0.7))));
                    soul_lines.push(Line::from(format!("    - Allowed Tools:  {}", if soul.tool_allowlist.is_some() { "RESTRICTED" } else { "UNRESTRICTED" })));
                    soul_lines.push(Line::from(format!("    - Memory Vector:  {:?}", soul.rag_strategy)));

                    let soul_block = Paragraph::new(Text::from(soul_lines))
                        .block(Block::default()
                            .title(Span::styled(" System Soul Configuration ", Style::default().fg(Color::Cyan)))
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Rgb(60, 60, 80)))
                        );
                    f.render_widget(soul_block, matrix_layout[0]);

                    // System Prompt / Rules view (Right column)
                    let prompt_rules_block = Block::default()
                        .title(Span::styled(" Active Cognitive Instructions ", Style::default().fg(Color::Yellow)))
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Rgb(60, 60, 80)));

                    let rules_lines: Vec<Line> = soul.system_prompt
                        .lines()
                        .skip(rules_scroll)
                        .map(|l| Line::from(l))
                        .collect();

                    let rules_p = Paragraph::new(Text::from(rules_lines))
                        .block(prompt_rules_block)
                        .style(Style::default().fg(Color::Rgb(200, 200, 210)))
                        .wrap(Wrap { trim: false });
                    f.render_widget(rules_p, matrix_layout[1]);
                }
                3 => {
                    // TELEMETRY & ECONOMY TAB
                    let telemetry_layout = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
                        .split(layout[1]);

                    // Left Column: Budgets & Coefficients
                    let eco_left_layout = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(3), // Budget Gauge
                            Constraint::Min(0),    // Production statistics
                        ].as_ref())
                        .split(telemetry_layout[0]);

                    // Economy state queries
                    let mut economy = agent.economy.lock().unwrap();
                    let rem_budget = economy.budget.remaining();
                    let total_capacity = economy.budget.total_budget;
                    let percentage = if total_capacity > 0 {
                        (rem_budget as f64 / total_capacity as f64).clamp(0.0, 1.0)
                    } else {
                        1.0
                    };

                    let budget_gauge = Gauge::default()
                        .block(Block::default().title(" Cognitive Token Budget Capacity ").borders(Borders::NONE))
                        .gauge_style(Style::default().fg(if percentage > 0.5 { Color::Green } else if percentage > 0.2 { Color::Yellow } else { Color::Red }))
                        .percent((percentage * 100.0) as u16)
                        .label(format!("Reserve: {} / Limit: {}", rem_budget, total_capacity));
                    f.render_widget(budget_gauge, eco_left_layout[0]);

                    // Clone production function to allow safe mutable split-borrows of economy
                    let production = economy.production.clone();
                    let bellman_val = economy.bellman.bellman_iteration(rem_budget, production.theta, &production);

                    // Production function & macroeconomic indices
                    let production_stats = vec![
                        Line::from(""),
                        Line::from(vec![
                            Span::styled("  DSGE Cognitive Production Function Model:", Style::default().add_modifier(Modifier::BOLD)),
                        ]),
                        Line::from(format!("    ▪ Learned capacity alpha (asymptotic quality):   {:.4}", production.alpha)),
                        Line::from(format!("    ▪ Convergence speed beta (tokens multiplier):    {:.4}", production.beta)),
                        Line::from(format!("    ▪ Target complexity scale coefficient theta:     {:.4}", production.theta)),
                        Line::from(""),
                        Line::from(vec![
                            Span::styled("  Live Cognitive Macroeconomics State Index:", Style::default().add_modifier(Modifier::BOLD)),
                        ]),
                        Line::from(format!("    ▪ Rolling Cognitive Average Entropy:             {:.4}", economy.macro_state.average_entropy)),
                        Line::from(format!("    ▪ Live API Provider Liquidity Index:              {:.4}", economy.macro_state.model_liquidity)),
                        Line::from(format!("    ▪ Adaptive Shadow Price Action Inflation:        {:.4}", economy.macro_state.context_inflation)),
                        Line::from(""),
                        Line::from(vec![
                            Span::styled("  Direct Token Economy Balances:", Style::default().add_modifier(Modifier::BOLD)),
                        ]),
                        Line::from(format!("    ▪ Accumulated API Latency Spent:                  {} ms", bellman_val.round())),
                        Line::from(format!("    ▪ Estimated Financial Cost:                       ${:.4} USD", tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async { *agent.total_cost.lock().await })
                        }))),
                    ];

                    let eco_left_block = Paragraph::new(Text::from(production_stats))
                        .block(Block::default()
                            .title(Span::styled(" DSGE Mathematical Telemetry ", Style::default().fg(Color::Green)))
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Rgb(60, 60, 80)))
                        );
                    f.render_widget(eco_left_block, eco_left_layout[1]);

                    // Right Column: Live Model Performance Statistics
                    let model_entries = &economy.model_perf.entries;
                    let mut rows = Vec::new();
                    for (model_id, stats) in model_entries {
                        let truncated_id = if model_id.len() > 24 {
                            format!("...{}", &model_id[model_id.len() - 21..])
                        } else {
                            model_id.clone()
                        };
                        let success_rate = if stats.calls > 0 {
                            (stats.successes as f64 / stats.calls as f64 * 100.0).round() as u64
                        } else {
                            100
                        };
                        rows.push(Row::new(vec![
                            Cell::from(truncated_id).style(Style::default().fg(Color::Cyan)),
                            Cell::from(stats.calls.to_string()),
                            Cell::from(format!("{}%", success_rate)).style(Style::default().fg(if success_rate > 80 { Color::Green } else { Color::Yellow })),
                            Cell::from(format!("{}ms", stats.last_latency_ms)),
                            Cell::from(stats.errors.to_string()).style(Style::default().fg(if stats.errors > 0 { Color::Red } else { Color::DarkGray })),
                        ]));
                    }

                    let table = Table::new(
                        rows,
                        [
                            Constraint::Percentage(40),
                            Constraint::Percentage(15),
                            Constraint::Percentage(15),
                            Constraint::Percentage(15),
                            Constraint::Percentage(15),
                        ]
                    )
                    .header(Row::new(vec!["Model Profile", "Calls", "Success", "Latency", "Errors"]).style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)))
                    .block(Block::default()
                        .title(Span::styled(" Live Provider Performance Routing ", Style::default().fg(Color::Yellow)))
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Rgb(60, 60, 80)))
                    );
                    f.render_widget(table, telemetry_layout[1]);
                }
                _ => {}
            }

            // 3. Render Status Line
            let status = Paragraph::new(status_line.as_str())
                .style(Style::default().fg(Color::DarkGray));
            f.render_widget(status, layout[2]);
        })?;

        // Handle operator keyboard input asynchronously with brief polling
        if event::poll(Duration::from_millis(100))? {
            if let CEvent::Key(key) = event::read()? {
                // Global quit handles
                if key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL {
                    break;
                }
                if key.code == KeyCode::Char('d') && key.modifiers == KeyModifiers::CONTROL {
                    break;
                }

                // Global authorization shortcuts
                if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('a') {
                    if !pending_approvals.is_empty() {
                        let approval = pending_approvals.remove(0);
                        agent.approve(approval.id, true);
                        status_line = format!("🟢 Action authorized: {}", approval.tool);
                    }
                    continue;
                }
                if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('x') {
                    if !pending_approvals.is_empty() {
                        let approval = pending_approvals.remove(0);
                        agent.approve(approval.id, false);
                        status_line = format!("🔴 Action rejected: {}", approval.tool);
                    }
                    continue;
                }

                // View controls (Tabs)
                if key.code == KeyCode::Tab {
                    active_tab = (active_tab + 1) % tabs.len();
                    continue;
                }
                if let KeyCode::Char(c) = key.code {
                    if c == '1' { active_tab = 0; continue; }
                    if c == '2' { active_tab = 1; continue; }
                    if c == '3' { active_tab = 2; continue; }
                    if c == '4' { active_tab = 3; continue; }
                }

                // View specific scroll / inputs
                match active_tab {
                    0 => {
                        // Console input interaction
                        match key.code {
                            KeyCode::Char(c) => {
                                input_buffer.push(c);
                            }
                            KeyCode::Backspace => {
                                input_buffer.pop();
                            }
                            KeyCode::Up => {
                                if chat_scroll > 0 { chat_scroll -= 1; }
                            }
                            KeyCode::Down => {
                                if chat_scroll < messages.len() { chat_scroll += 1; }
                            }
                            KeyCode::Enter => {
                                let msg = input_buffer.trim().to_string();
                                if !msg.is_empty() {
                                    if msg == "/reset" || msg == "/new" {
                                        let _ = agent.reset_history().await;
                                        messages.clear();
                                        tool_trace.clear();
                                        detailed_tool_logs.clear();
                                        tool_trace.push("🔄 Workspace cognitive history cleared.".to_string());
                                        messages.push(("system".to_string(), "🔄 New session started. All context and cognitive history cleared.".to_string()));
                                        input_buffer.clear();
                                        continue;
                                    }
                                    if msg == "/exit" || msg == "/quit" {
                                        break;
                                    }

                                    messages.push(("user".to_string(), msg.clone()));
                                    input_buffer.clear();
                                    status_line = "⏳ Thinking...".to_string();

                                    let agent_clone = agent.clone();
                                    let event_tx = agent.event_tx.clone();
                                    active_chat_task = Some(tokio::spawn(async move {
                                        match agent_clone.chat(&msg).await {
                                            Ok(resp) => {
                                                if msg.starts_with("/model") {
                                                    let _ = event_tx.send(Event::AgentResponse {
                                                        content: pharmakon_common::MessageContent::Text(resp),
                                                    });
                                                }
                                            }
                                            Err(e) => {
                                                let _ = event_tx.send(Event::Error {
                                                    message: format!("Command error: {}", e),
                                                });
                                            }
                                        }
                                    }));
                                }
                            }
                            KeyCode::Esc => {
                                if let Some(ref handle) = active_chat_task {
                                    handle.abort();
                                    active_chat_task = None;
                                    status_line = "🔴 Execution cancelled by operator. Ready for input.".to_string();
                                    tool_trace.push("❌ Execution aborted by operator via ESC.".to_string());
                                    messages.push(("system".to_string(), "🔴 Execution cancelled by operator. Ready for input.".to_string()));
                                } else {
                                    input_buffer.clear();
                                }
                            }
                            _ => {}
                        }
                    }
                    2 => {
                        // Matrix tab scrolling rules
                        match key.code {
                            KeyCode::Up => {
                                if rules_scroll > 0 { rules_scroll -= 1; }
                            }
                            KeyCode::Down => {
                                rules_scroll += 1;
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

/// Simple line-based REPL fallback when TUI initialization fails.
pub async fn run_repl(agent: Arc<Agent>) -> Result<()> {
    use std::io::{self, Write};

    println!("💊 Pharmakon REPL (Interactive Operator Shell)");
    println!("Type commands to interact with the agent. /quit to exit, /reset to clean context.");
    println!();

    let mut event_rx = agent.event_tx.subscribe();

    loop {
        print!("pharmakon> ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim().to_string();

        if input.is_empty() {
            continue;
        }

        match input.as_str() {
            "/quit" | "/exit" => break,
            "/reset" => {
                let _ = agent.reset_history().await;
                println!("History reset.");
                continue;
            }
            _ => {}
        }

        // Drain any pending events
        while let Ok(event) = event_rx.try_recv() {
            match event {
                Event::AgentResponse { content } => {
                    println!("\n{}", content);
                }
                Event::ToolCall { name, args } => {
                    println!("  🔧 {} ({})", name, args);
                }
                Event::ToolResult { result } => {
                    println!("    ✓ {}", result);
                }
                Event::Error { message } => {
                    eprintln!("  ❌ {}", message);
                }
                _ => {}
            }
        }

        match agent.chat(&input).await {
            Ok(response) => {
                if !response.is_empty() {
                    println!("\n{}", response);
                }
            }
            Err(e) => {
                eprintln!("Error: {}", e);
            }
        }
        println!();
    }

    Ok(())
}
