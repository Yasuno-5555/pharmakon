use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use pharmakon_common::Event;
use pharmakon_core::agent::Agent;
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    style::{Color, Style},
};
use std::io;
use std::sync::Arc;

/// Run the TUI with a direct Agent connection (no gateway needed).
pub async fn run_tui(agent: Arc<Agent>, initial_message: Option<String>) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Subscribe to agent events
    let mut event_rx = agent.event_tx.subscribe();

    let mut messages: Vec<(String, String)> = Vec::new(); // (role, content)
    let mut tool_trace: Vec<String> = Vec::new();
    let mut input_buffer = String::new();
    let mut status_line = String::from("🟢 Ready. Type a message and press Enter. Ctrl+C to quit.");

    // Send initial message if provided
    if let Some(msg) = initial_message {
        messages.push(("user".to_string(), msg.clone()));
        let agent_clone = agent.clone();
        tokio::spawn(async move {
            if let Err(e) = agent_clone.chat(&msg).await {
                log::error!("Agent error: {}", e);
            }
        });
    }

    loop {
        // Drain events from agent
        while let Ok(event) = event_rx.try_recv() {
            match event {
                Event::AgentThought { content } => {
                    tool_trace.push(format!("💭 {}", content));
                }
                Event::AgentThoughtChunk { chunk, .. } => {
                    if let Some(last) = tool_trace.last_mut() {
                        if last.starts_with("💭") {
                            last.push_str(&chunk);
                        } else {
                            tool_trace.push(format!("💭 {}", chunk));
                        }
                    } else {
                        tool_trace.push(format!("💭 {}", chunk));
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
                    let args_str = serde_json::to_string(&args).unwrap_or_default();
                    let display_args = if args_str.len() > 80 {
                        format!("{}...", &args_str[..77])
                    } else {
                        args_str
                    };
                    tool_trace.push(format!("🔧 {} ({})", name, display_args));
                    status_line = format!("⚙ Running: {}...", name);
                }
                Event::ToolResult { result } => {
                    let display = if result.len() > 100 {
                        format!("{}...", &result[..97])
                    } else {
                        result
                    };
                    tool_trace.push(format!("  ✓ {}", display));
                }
                Event::Error { message } => {
                    tool_trace.push(format!("❌ Error: {}", message));
                    status_line = "🔴 Error occurred.".to_string();
                }
                Event::AgentHangDetected { reason } => {
                    tool_trace.push(format!("⏱ Hang detected: {}", reason));
                }
                Event::ApprovalRequest { id, tool, args } => {
                    let args_str = serde_json::to_string(&args).unwrap_or_default();
                    tool_trace.push(format!("🛡 Approval needed: {} ({}) [id: {}]", tool, args_str, id));
                }
                _ => {}
            }
        }

        terminal.draw(|f| {
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(70), Constraint::Percentage(30)].as_ref())
                .split(f.area());

            // Left: Chat
            let left_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(0), Constraint::Length(3)].as_ref())
                .split(main_chunks[0]);

            let msg_items: Vec<ListItem> = messages
                .iter()
                .rev()
                .take(50)
                .map(|(role, content)| {
                    let prefix = match role.as_str() {
                        "user" => "🧑 You",
                        "assistant" => "🦞 Pharmakon",
                        _ => "  •",
                    };
                    let style = if role == "assistant" {
                        Style::default().fg(Color::Cyan)
                    } else if role == "user" {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default()
                    };
                    ListItem::new(format!("{}: {}", prefix, content)).style(style)
                })
                .collect();
            let msg_block = List::new(msg_items)
                .block(Block::default().title(" Conversation ").borders(Borders::ALL));
            f.render_widget(msg_block, left_chunks[0]);

            let input = Paragraph::new(input_buffer.as_str())
                .block(Block::default().borders(Borders::ALL).title(" Message (Enter to send, Ctrl+C to quit) "))
                .style(Style::default().fg(Color::Yellow));
            f.render_widget(input, left_chunks[1]);

            // Right: Tools & Status
            let right_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(0), Constraint::Length(1)].as_ref())
                .split(main_chunks[1]);

            let trace_items: Vec<ListItem> = tool_trace
                .iter()
                .rev()
                .take(30)
                .map(|s| ListItem::new(s.as_str()))
                .collect();
            let trace_block = List::new(trace_items)
                .block(Block::default().title(" Tool Trace ").borders(Borders::ALL));
            f.render_widget(trace_block, right_chunks[0]);

            let status = Paragraph::new(status_line.as_str())
                .style(Style::default().fg(Color::DarkGray));
            f.render_widget(status, right_chunks[1]);
        })?;

        // Handle keyboard input
        if event::poll(std::time::Duration::from_millis(50))? {
            if let CEvent::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('c') if key.modifiers == event::KeyModifiers::CONTROL => break,
                    KeyCode::Char('d') if key.modifiers == event::KeyModifiers::CONTROL => break,
                    KeyCode::Char('l') if key.modifiers == event::KeyModifiers::CONTROL => {
                        // Clear screen (redraw handled automatically)
                    }
                    KeyCode::Char('r') if key.modifiers == event::KeyModifiers::CONTROL => {
                        let _ = agent.reset_history().await;
                        messages.clear();
                        tool_trace.clear();
                        tool_trace.push("🔄 History reset.".to_string());
                    }
                    KeyCode::Char(c) => {
                        input_buffer.push(c);
                    }
                    KeyCode::Backspace => {
                        input_buffer.pop();
                    }
                    KeyCode::Enter => {
                        let msg = input_buffer.trim().to_string();
                        if !msg.is_empty() {
                            messages.push(("user".to_string(), msg.clone()));
                            input_buffer.clear();
                            status_line = "⏳ Thinking...".to_string();

                            let agent_clone = agent.clone();
                            tokio::spawn(async move {
                                if let Err(e) = agent_clone.chat(&msg).await {
                                    log::error!("Agent chat error: {}", e);
                                }
                            });
                        }
                    }
                    KeyCode::Esc => {
                        input_buffer.clear();
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

/// Simple line-based REPL (fallback when TUI can't initialize).
pub async fn run_repl(agent: Arc<Agent>) -> Result<()> {
    use std::io::{self, Write};

    println!("🦞 Pharmakon REPL");
    println!("Type your message and press Enter. Type /quit to exit, /reset to clear history.");
    println!();

    let mut event_rx = agent.event_tx.subscribe();

    loop {
        print!("> ");
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
            "/tools" => {
                let reg = agent.registry.lock().await;
                let tools = reg.all_metadata();
                println!("Available tools ({}):", tools.len());
                for t in tools.iter().take(20) {
                    println!("  - {}: {}", t.name, t.description);
                }
                if tools.len() > 20 {
                    println!("  ... and {} more", tools.len() - 20);
                }
                continue;
            }
            _ => {}
        }

        // Drain any pending events before sending
        while let Ok(event) = event_rx.try_recv() {
            match event {
                Event::AgentResponse { content } => {
                    println!("\n{}", content);
                }
                Event::ToolCall { name, args } => {
                    let args_str = serde_json::to_string(&args).unwrap_or_default();
                    let display = if args_str.len() > 100 { format!("{}...", &args_str[..97]) } else { args_str };
                    println!("  🔧 {} ({})", name, display);
                }
                Event::ToolResult { result } => {
                    let display = if result.len() > 200 { format!("{}...", &result[..197]) } else { result };
                    println!("    ✓ {}", display);
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
