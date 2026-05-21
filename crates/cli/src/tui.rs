use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use pharmakon_common::Event;
use pharmakon_core::agent::Agent;
use pharmakon_core::providers::registry::ModelRegistry;
use pharmakon_common::Config;
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    style::{Color, Style},
    text::Span,
};
use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

struct DetailedToolLog {
    name: String,
    args: String,
    status: &'static str,
    result: String,
    latency_ms: Option<u64>,
    timestamp: chrono::DateTime<chrono::Local>,
    start_time: Instant,
}

struct LocalApproval {
    id: String,
    tool: String,
    args: String,
}

/// Format a tool call as a compact inline card.
fn format_tool_call(name: &str, args: &str) -> String {
    let (glyph, label, _) = get_tool_family_info(name);
    format!(
        " {} {} │ {} {}",
        glyph, label, name, truncate(args, 60)
    )
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_string() }
    else { format!("{}...", &s[..max.saturating_sub(3)]) }
}

/// Run the Pharmakon TUI — Claude Code inspired single-pane chat interface.
pub async fn run_tui(agent: Arc<Agent>, initial_message: Option<String>) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut event_rx = agent.event_tx.subscribe();

    // Chat messages: (role, content)
    let mut messages: Vec<(String, String)> = Vec::new();
    let mut detailed_tool_logs: Vec<DetailedToolLog> = Vec::new();
    let mut pending_approvals: Vec<LocalApproval> = Vec::new();
    let mut input_buffer = String::new();

    let active_model_init = {
        let m = agent.model.lock().await;
        m.name().to_string()
    };
    let mut active_model = active_model_init;
    let mut status = String::from("Ready.");

    let mut active_chat_task: Option<tokio::task::JoinHandle<()>> = None;
    let mut chat_scroll: Option<usize> = None; // None = auto-scroll

    // Resize buffer for event processing
    let mut event_buf = Vec::with_capacity(32);

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

    // Spawn crossterm event reader task
    let (crossterm_tx, mut crossterm_rx) = tokio::sync::mpsc::channel::<CEvent>(128);
    tokio::spawn(async move {
        loop {
            match event::poll(Duration::from_millis(100)) {
                Ok(true) => {
                    if let Ok(ev) = event::read() {
                        if crossterm_tx.send(ev).await.is_err() {
                            break;
                        }
                    }
                }
                Ok(false) => {}
                Err(_) => {
                    break;
                }
            }
        }
    });

    // Helper macro / lambda to draw the UI state
    let mut draw_ui = |terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
                       active_model: &str,
                       status: &str,
                       active_chat_task: &Option<tokio::task::JoinHandle<()>>,
                       messages: &[(String, String)],
                       chat_scroll: Option<usize>,
                       input_buffer: &str| -> Result<()> {
        terminal.draw(|f| {
            let area = f.area();
            if area.height < 5 || area.width < 20 {
                return;
            }

            let vert = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1), // header / status bar
                    Constraint::Min(1),    // main chat
                    Constraint::Length(3), // input box
                ])
                .split(area);

            // ── Header bar ──
            let header_text = format!(
                " 💊 Pharmakon  |  {}  |  {}",
                active_model,
                if active_chat_task.is_some() { "⚙ working..." } else { status }
            );
            let header = Paragraph::new(header_text)
                .style(Style::default().fg(Color::Rgb(140, 140, 160)));
            f.render_widget(header, vert[0]);

            // ── Chat area ──
            let mut display_lines: Vec<(Color, String)> = Vec::new();
            for (role, content) in messages.iter() {
                match role.as_str() {
                    "user" => {
                        display_lines.push((Color::Green, format!("> {}", content)));
                    }
                    "assistant" => {
                        for line in content.split('\n') {
                            let color = if line.starts_with("```") {
                                Color::Rgb(180, 140, 100)
                            } else if line.starts_with('#') || line.starts_with("**") {
                                Color::Cyan
                            } else if line.starts_with("  ") || line.starts_with('\t') {
                                Color::Rgb(160, 160, 170)
                            } else {
                                Color::White
                            };
                            display_lines.push((color, format!(" {}", line)));
                        }
                        display_lines.push((Color::DarkGray, String::new()));
                    }
                    "thought" => {
                        display_lines.push((Color::Rgb(120, 120, 140), format!("  💭 {}", content)));
                    }
                    "tool" => {
                        display_lines.push((Color::Magenta, format!(" {}", content)));
                    }
                    "result" => {
                        display_lines.push((Color::Rgb(140, 200, 160), content.clone()));
                        display_lines.push((Color::DarkGray, String::new()));
                    }
                    "error" => {
                        display_lines.push((Color::Red, content.clone()));
                    }
                    "system" => {
                        display_lines.push((Color::Yellow, format!(" ⚙ {}", content)));
                        display_lines.push((Color::DarkGray, String::new()));
                    }
                    _ => {
                        display_lines.push((Color::DarkGray, content.clone()));
                    }
                }
            }

            // If waiting for response, show a subtle indicator
            if active_chat_task.is_some() {
                let dots = (std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()
                    / 500
                    % 4) as usize;
                let indicator = format!(" 🧐 {}", ".".repeat(dots));
                display_lines.push((Color::Rgb(100, 100, 120), indicator));
            }

            let chat_height = vert[1].height.saturating_sub(1) as usize;
            let total_lines = display_lines.len();

            let start_idx = if let Some(scroll) = chat_scroll {
                if total_lines > chat_height + scroll {
                    total_lines - chat_height - scroll
                } else {
                    0
                }
            } else { total_lines.saturating_sub(chat_height) };

            let visible_lines: Vec<ListItem> = display_lines[start_idx..]
                .iter()
                .map(|(color, text)| ListItem::new(text.as_str()).style(Style::default().fg(*color)))
                .collect();

            let chat_block = List::new(visible_lines)
                .block(Block::default().borders(Borders::TOP).border_style(Style::default().fg(Color::Rgb(50, 50, 60))));
            f.render_widget(chat_block, vert[1]);

            // ── Input box ──
            let input_prefix = if input_buffer.starts_with('/') { "/" } else { "> " };
            let input_text = format!("{}{}", input_prefix, input_buffer);

            let (border_color, input_title) = if active_chat_task.is_some() {
                (Color::Yellow, " ⏳ processing... (ESC to cancel) ")
            } else if input_buffer.starts_with('/') {
                (Color::Magenta, " cmd ")
            } else {
                (Color::Rgb(80, 80, 100), "")
            };

            let input_para = Paragraph::new(input_text)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(border_color))
                        .title(Span::styled(input_title, Style::default().fg(Color::DarkGray))),
                )
                .style(Style::default().fg(Color::White));
            f.render_widget(input_para, vert[2]);
        })?;
        Ok(())
    };

    // Perform initial draw
    draw_ui(&mut terminal, &active_model, &status, &active_chat_task, &messages, chat_scroll, &input_buffer)?;

    loop {
        let mut need_redraw = false;
        let is_busy = active_chat_task.is_some();

        tokio::select! {
            // 1. Drain events from agent
            event_res = event_rx.recv() => {
                match event_res {
                    Ok(event) => {
                        event_buf.clear();
                        event_buf.push(event);
                        while let Ok(ev) = event_rx.try_recv() {
                            event_buf.push(ev);
                        }

                        for event in &event_buf {
                            match event {
                                Event::AgentResponse { content } => {
                                    messages.push(("assistant".to_string(), content.to_string()));
                                    chat_scroll = None; // auto-scroll on new response
                                    status = "Ready.".to_string();
                                }
                                Event::AgentResponseChunk { chunk, .. } => {
                                    if let Some(last) = messages.last_mut() {
                                        if last.0 == "assistant" {
                                            last.1.push_str(chunk);
                                        } else {
                                            messages.push(("assistant".to_string(), chunk.clone()));
                                            chat_scroll = None;
                                        }
                                    } else {
                                        messages.push(("assistant".to_string(), chunk.clone()));
                                        chat_scroll = None;
                                    }
                                }
                                Event::AgentThought { content } => {
                                    let thought = content.to_string();
                                    messages.push(("thought".to_string(), thought));
                                }
                                Event::ToolCall { name, args } => {
                                    let args_str = serde_json::to_string_pretty(args).unwrap_or_default();
                                    status = format!("Running {}...", name);
                                    let line = format_tool_call(name, &args_str);
                                    messages.push(("tool".to_string(), line));

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
                                    status = "Ready.".to_string();
                                    let mut tool_name = "tool".to_string();
                                    if let Some(entry) = detailed_tool_logs.iter_mut().rev().find(|e| e.status == "RUNNING") {
                                        entry.status = "DONE";
                                        entry.result = result.clone();
                                        entry.latency_ms = Some(entry.start_time.elapsed().as_millis() as u64);
                                        tool_name = entry.name.clone();
                                    }
                                    let preview = if result.trim().is_empty() {
                                        "ok".to_string()
                                    } else {
                                        let trimmed = result.trim();
                                        let first_line = trimmed.lines().next().unwrap_or("");
                                        truncate(first_line, 100)
                                    };
                                    messages.push(("result".to_string(), format!("  ✔ {} ➔ {}", tool_name, preview)));
                                }
                                Event::ApprovalRequest { id, tool, args } => {
                                    let args_str = serde_json::to_string_pretty(args).unwrap_or_default();
                                    status = "Awaiting authorization...".to_string();
                                    pending_approvals.push(LocalApproval {
                                        id: id.clone(),
                                        tool: tool.clone(),
                                        args: args_str,
                                    });
                                }
                                Event::Error { message } => {
                                    status = format!("Error: {}", message);
                                    messages.push(("error".to_string(), format!("✖ {}", message)));
                                }
                                Event::ModelSwitched { model_id } => {
                                    active_model = model_id.clone();
                                    messages.push(("system".to_string(), format!("Switched to: {}", model_id)));
                                    status = format!("Model: {}", model_id);
                                }
                                _ => {}
                            }
                        }
                        need_redraw = true;
                    }
                    Err(_) => {
                        break;
                    }
                }
            }
            // 2. Keyboard / Crossterm input
            crossterm_res = crossterm_rx.recv() => {
                match crossterm_res {
                    Some(CEvent::Key(key)) => {
                        // Global: Ctrl+C / Ctrl+D → quit
                        if key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL {
                            agent.shutdown();
                            break;
                        }
                        if key.code == KeyCode::Char('d') && key.modifiers == KeyModifiers::CONTROL {
                            agent.shutdown();
                            break;
                        }

                        // Approvals
                        if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('a') {
                            if !pending_approvals.is_empty() {
                                let a = pending_approvals.remove(0);
                                agent.approve(a.id, true);
                                status = format!("Approved: {}", a.tool);
                            }
                            need_redraw = true;
                            continue;
                        }
                        if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('x') {
                            if !pending_approvals.is_empty() {
                                let a = pending_approvals.remove(0);
                                agent.approve(a.id, false);
                                status = format!("Rejected: {}", a.tool);
                            }
                            need_redraw = true;
                            continue;
                        }

                        match key.code {
                            KeyCode::PageUp => {
                                chat_scroll = Some(chat_scroll.unwrap_or(0).saturating_add(5));
                                need_redraw = true;
                            }
                            KeyCode::PageDown => {
                                if let Some(s) = chat_scroll.as_mut() {
                                    *s = s.saturating_sub(5);
                                    if *s == 0 { chat_scroll = None; }
                                }
                                need_redraw = true;
                            }
                            KeyCode::Esc => {
                                if let Some(ref handle) = active_chat_task {
                                    handle.abort();
                                    active_chat_task = None;
                                    status = "Cancelled.".to_string();
                                    messages.push(("system".to_string(), "Cancelled.".to_string()));
                                } else {
                                    input_buffer.clear();
                                }
                                need_redraw = true;
                            }
                            KeyCode::Enter => {
                                if is_busy {
                                    continue;
                                }
                                let msg = input_buffer.trim().to_string();
                                if msg.is_empty() { continue; }
                                input_buffer.clear();

                                if msg == "/reset" || msg == "/new" {
                                    let _ = agent.reset_history().await;
                                    messages.clear();
                                    detailed_tool_logs.clear();
                                    messages.push(("system".to_string(), "Session reset.".to_string()));
                                    status = "Ready.".to_string();
                                    need_redraw = true;
                                    continue;
                                }
                                if msg == "/revert" || msg == "/undo" {
                                    let session_id = {
                                        let sid = agent.session_id.lock().await;
                                        sid.clone()
                                    };
                                    match agent.revert_last_mutation(&session_id).await {
                                        Ok(feedback) => {
                                            messages.push(("system".to_string(), feedback.clone()));
                                            status = feedback;
                                        }
                                        Err(e) => {
                                            messages.push(("error".to_string(), format!("Revert failed: {}", e)));
                                            status = format!("Revert failed: {}", e);
                                        }
                                    }
                                    need_redraw = true;
                                    continue;
                                }
                                if msg.starts_with("/model") {
                                    let parts: Vec<&str> = msg["/model".len()..].trim().split_whitespace().collect();
                                    if !parts.is_empty() {
                                        let mut save = false;
                                        let mut model_id = parts[0];
                                        if parts[0] == "--save" && parts.len() > 1 {
                                            save = true;
                                            model_id = parts[1];
                                        } else if parts.len() > 1 && parts[1] == "--save" {
                                            save = true;
                                        }
                                        if let Some(new_m) = ModelRegistry::get_model(model_id) {
                                            let mut current = agent.model.lock().await;
                                            *current = new_m;
                                            messages.push(("system".to_string(), format!("Switched model to {}", model_id)));
                                            status = format!("Model: {}", model_id);

                                            let session_id = { agent.session_id.lock().await.clone() };
                                            let states = agent.session_states.lock().await;
                                            let est_tokens = if let Some(state) = states.get(&session_id) {
                                                let s = state.lock().await;
                                                s.history.iter().map(|m| m.content.as_ref().map(|c| c.to_string().len()).unwrap_or(0)).sum::<usize>() / 4
                                            } else {
                                                0
                                            };
                                            let est_cost = (est_tokens as f64) * 0.000001;
                                            messages.push(("system".to_string(), format!("⚠ Context re-processing cost: ~{} tokens (${:.4})", est_tokens, est_cost)));

                                            if save {
                                                if let Ok(mut cfg) = Config::load() {
                                                    if let Some(idx) = model_id.find('/') {
                                                        cfg.default_agent.provider = model_id[..idx].to_string();
                                                        cfg.default_agent.model = model_id[idx+1..].to_string();
                                                    } else {
                                                        cfg.default_agent.model = model_id.to_string();
                                                    }
                                                    if cfg.save().is_ok() {
                                                        messages.push(("system".to_string(), "Configuration saved permanently.".to_string()));
                                                    }
                                                }
                                            }
                                        } else {
                                            messages.push(("error".to_string(), format!("Model not found or invalid API key: {}", model_id)));
                                        }
                                    } else {
                                        messages.push(("system".to_string(), "Usage: /model [--save] <model_id>".to_string()));
                                    }
                                    need_redraw = true;
                                    continue;
                                }
                                if msg == "/exit" || msg == "/quit" { break; }

                                messages.push(("user".to_string(), msg.clone()));
                                status = "Thinking...".to_string();

                                let agent_clone = agent.clone();
                                active_chat_task = Some(tokio::spawn(async move {
                                    if let Err(e) = agent_clone.chat(&msg).await {
                                        log::error!("Agent error: {}", e);
                                    }
                                }));
                                need_redraw = true;
                            }
                            KeyCode::Char(c) => {
                                input_buffer.push(c);
                                need_redraw = true;
                            }
                            KeyCode::Backspace => {
                                input_buffer.pop();
                                need_redraw = true;
                            }
                            _ => {}
                        }
                    }
                    Some(CEvent::Resize(_, _)) => {
                        need_redraw = true;
                    }
                    _ => {}
                }
            }
            // 3. Task completion
            res = async {
                if let Some(ref mut handle) = active_chat_task {
                    let _ = handle.await;
                } else {
                    futures::future::pending::<()>().await;
                }
            } => {
                active_chat_task = None;
                need_redraw = true;
            }
            // 4. Animation timer (only active while thinking)
            _ = async {
                if is_busy {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                } else {
                    futures::future::pending::<()>().await;
                }
            } => {
                need_redraw = true;
            }
        }

        if need_redraw {
            draw_ui(&mut terminal, &active_model, &status, &active_chat_task, &messages, chat_scroll, &input_buffer)?;
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(())
}

/// Simple REPL fallback when TUI fails — shows events live during chat.
pub async fn run_repl(agent: Arc<Agent>) -> Result<()> {
    use std::io::{self, Write};

    println!("\u{1F48A} Pharmakon REPL");
    println!("/quit to exit, /reset to clean context.");
    println!();

    let agent_clone = agent.clone();
    let ctrlc_task = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            println!("\n🛑 [System] Interrupt received. Shutting down agent gracefully...");
            agent_clone.shutdown();
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            std::process::exit(130);
        }
    });

    let mut event_rx = agent.event_tx.subscribe();
    // Track whether we're inside a chat call to show events live
    let mut in_chat = false;

    loop {
        // Drain pending events (from previous chat if any)
        while let Ok(event) = event_rx.try_recv() {
            match event {
                Event::AgentResponse { content } => {
                    println!("\n{}\n", content);
                }
                Event::AgentResponseChunk { chunk, .. } if in_chat => {
                    print!("{}", chunk);
                    io::stdout().flush().ok();
                }
                Event::ToolCall { name, args } => {
                    let args_str = serde_json::to_string_pretty(&args).unwrap_or_default();
                    println!("  \u{1F527} {} ({})", name, truncate(&args_str, 80));
                }
                Event::ToolResult { result } => {
                    let preview = if result.trim().is_empty() {
                        "ok".to_string()
                    } else {
                        truncate(result.trim(), 120)
                    };
                    println!("    \u{2714} {}", preview);
                }
                Event::Error { message } => {
                    eprintln!("  \u{2716} {}", message);
                }
                _ => {}
            }
        }

        // Prompt
        print!("> ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim().to_string();
        if input.is_empty() { continue; }

        match input.as_str() {
            "/quit" | "/exit" => break,
            "/reset" => {
                let _ = agent.reset_history().await;
                println!("Session reset.");
                continue;
            }
            "/revert" | "/undo" => {
                let session_id = {
                    let sid = agent.session_id.lock().await;
                    sid.clone()
                };
                match agent.revert_last_mutation(&session_id).await {
                    Ok(feedback) => println!("⚙️ [System]: {}", feedback),
                    Err(e) => eprintln!("❌ Revert failed: {}", e),
                }
                continue;
            }
            _ if input.starts_with("/model") => {
                let parts: Vec<&str> = input["/model".len()..].trim().split_whitespace().collect();
                if !parts.is_empty() {
                    let mut save = false;
                    let mut model_id = parts[0];
                    if parts[0] == "--save" && parts.len() > 1 {
                        save = true;
                        model_id = parts[1];
                    } else if parts.len() > 1 && parts[1] == "--save" {
                        save = true;
                    }
                    if let Some(new_m) = ModelRegistry::get_model(model_id) {
                        let mut current = agent.model.lock().await;
                        *current = new_m;
                        println!("⚙️ [System]: Switched model to {}", model_id);

                        // Context re-processing cost calculation
                        let session_id = { agent.session_id.lock().await.clone() };
                        let states = agent.session_states.lock().await;
                        let est_tokens = if let Some(state) = states.get(&session_id) {
                            let s = state.lock().await;
                            s.history.iter().map(|m| m.content.as_ref().map(|c| c.to_string().len()).unwrap_or(0)).sum::<usize>() / 4
                        } else {
                            0
                        };
                        let est_cost = (est_tokens as f64) * 0.000001;
                        println!("⚠ Context re-processing cost: ~{} tokens (${:.4})", est_tokens, est_cost);

                        if save {
                            if let Ok(mut cfg) = Config::load() {
                                if let Some(idx) = model_id.find('/') {
                                    cfg.default_agent.provider = model_id[..idx].to_string();
                                    cfg.default_agent.model = model_id[idx+1..].to_string();
                                } else {
                                    cfg.default_agent.model = model_id.to_string();
                                }
                                if cfg.save().is_ok() {
                                    println!("⚙️ [System]: Configuration saved permanently.");
                                }
                            }
                        }
                    } else {
                        eprintln!("❌ Model not found or invalid API key: {}", model_id);
                    }
                } else {
                    println!("Usage: /model [--save] <model_id>");
                }
                continue;
            }
            _ => {}
        }

        // Send and show live events
        in_chat = true;
        match agent.chat(&input).await {
            Ok(response) => {
                if !response.is_empty() {
                    println!("\n{}\n", response);
                }
            }
            Err(e) => {
                eprintln!("\u{2716} Error: {}", e);
            }
        }
        in_chat = false;
        println!();
    }

    ctrlc_task.abort();
    Ok(())
}

fn get_tool_family_info(name: &str) -> (&'static str, &'static str, Color) {
    match name {
        "list_dir" | "view_file" => ("\u{25B7}", "read", Color::Blue),
        "modify_code" | "replace_file_content" | "multi_replace_file_content" | "write_to_file" => ("\u{25C6}", "patch", Color::Green),
        "shell" | "codeact" => ("\u{25B6}", "run", Color::Magenta),
        "grep_search" => ("\u{2315}", "find", Color::Cyan),
        _ => ("\u{2022}", "tool", Color::DarkGray),
    }
}
