use anyhow::Result;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Terminal,
};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use reqwest;
use std::io;
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};
use futures_util::{StreamExt, SinkExt};
use pharmakon_common::{Event, Request};
use tokio::sync::mpsc;

async fn wait_for_gateway() -> Result<()> {
    let health_url = "http://127.0.0.1:18789/health";
    let mut attempts = 0;
    println!("Waiting for gateway to be ready...");
    loop {
        match reqwest::get(health_url).await {
            Ok(response) if response.status().is_success() => {
                println!("Gateway is ready.");
                return Ok(());
            }
            _ => {
                if attempts >= 20 {
                    return Err(anyhow::anyhow!("Gateway not ready after {} attempts.", attempts));
                }
                attempts += 1;
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

pub async fn run_tui() -> Result<()> {
    // Wait for gateway to be available
    if let Err(e) = wait_for_gateway().await {
        eprintln!("{}", e);
        return Ok(());
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let url = "ws://127.0.0.1:18789/ws";
    tracing::debug!("Connecting to WebSocket at {}", url);
    let (ws_stream, _) = match connect_async(url).await {
        Ok(v) => {
            tracing::debug!("WebSocket handshake successful.");
            v
        },
        Err(e) => {
            tracing::error!("WebSocket handshake failed: {}", e);
            disable_raw_mode()?;
            execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
            return Err(anyhow::anyhow!("Could not connect to Gateway at {}: {}. Is it running?", url, e));
        }
    };

    let (mut ws_write, mut ws_read) = ws_stream.split();
    let (tx, mut rx) = mpsc::channel(100);

    // Background task to read from WS
    tokio::spawn(async move {
        while let Some(Ok(WsMessage::Text(text))) = ws_read.next().await {
            tracing::debug!(target: "tui", "Received event: {}", text);
            if let Ok(event) = serde_json::from_str::<Event>(&text) {
                if tx.send(event).await.is_err() {
                    tracing::error!("Failed to send event to TUI main loop.");
                    break;
                }
            }
        }
    });

    let mut messages: Vec<String> = Vec::new();
    let mut thoughts: Vec<String> = Vec::new();
    let mut input_buffer = String::new();

    loop {
        // Drain events
        while let Ok(event) = rx.try_recv() {
            match event {
                Event::AgentThought { content } => thoughts.push(format!("Thinking: {}", content)),
                Event::AgentThoughtChunk { chunk, .. } => {
                    if let Some(last) = thoughts.last_mut() {
                        if last.starts_with("Thinking:") {
                            last.push_str(&chunk);
                        } else {
                            thoughts.push(format!("Thinking: {}", chunk));
                        }
                    } else {
                        thoughts.push(format!("Thinking: {}", chunk));
                    }
                }
                Event::AgentResponse { content } => messages.push(format!("Agent: {}", content)),
                Event::ToolCall { name, args } => thoughts.push(format!("⚒ Tool Call: {} ({})", name, args)),
                Event::ToolResult { result } => thoughts.push(format!("✓ Tool Result: {}", result)),
                _ => {}
            }
        }

        terminal.draw(|f| {
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(75),
                    Constraint::Percentage(25),
                ].as_ref())
                .split(f.area());

            let left_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(0),
                    Constraint::Length(3),
                    Constraint::Percentage(30),
                ].as_ref())
                .split(main_chunks[0]);

            let msg_list: Vec<ListItem> = messages.iter().rev().map(|m| ListItem::new(m.as_str())).collect();
            let msg_block = List::new(msg_list)
                .block(Block::default().title(" Conversation History ").borders(Borders::ALL));
            f.render_widget(msg_block, left_chunks[0]);

            let input = Paragraph::new(input_buffer.as_str())
                .block(Block::default().borders(Borders::ALL).title(" Your Message (Enter to send) "));
            f.render_widget(input, left_chunks[1]);
            
            let thought_list: Vec<ListItem> = thoughts.iter().rev().take(20).map(|m| ListItem::new(m.as_str())).collect();
            let thought_block = List::new(thought_list)
                .block(Block::default().title(" Agent Reasoning & Tool Trace ").borders(Borders::ALL));
            f.render_widget(thought_block, left_chunks[2]);

            // Right Panel: Stats & Swarm
            let right_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(10), // Stats
                    Constraint::Min(0),     // Swarm
                ].as_ref())
                .split(main_chunks[1]);

            let stats = vec![
                ListItem::new("System Health: OK"),
                ListItem::new("CPU: 12%"),
                ListItem::new("MEM: 456MB"),
                ListItem::new("Status: ONLINE"),
                ListItem::new("----------------"),
                ListItem::new("Gateway: 127.0.0.1:18789"),
                ListItem::new("Latency: 45ms"),
            ];
            let stats_block = List::new(stats)
                .block(Block::default().title(" Metrics ").borders(Borders::ALL));
            f.render_widget(stats_block, right_chunks[0]);

            let swarm = vec![
                ListItem::new("● Supervisor (Active)"),
                ListItem::new("○ Researcher (Idle)"),
                ListItem::new("○ Coder (Idle)"),
            ];
            let swarm_block = List::new(swarm)
                .block(Block::default().title(" Autonomy Matrix ").borders(Borders::ALL));
            f.render_widget(swarm_block, right_chunks[1]);
        })?;

        if event::poll(std::time::Duration::from_millis(50))? {
            if let CEvent::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('c') if key.modifiers == event::KeyModifiers::CONTROL => break,
                    KeyCode::Char('r') => {
                        let req = Request::ResetHistory;
                        let msg = serde_json::to_string(&req)?;
                        ws_write.send(WsMessage::Text(msg.into())).await?;
                        messages.clear();
                        thoughts.push("History Reset".to_string());
                    }
                    KeyCode::Char(c) => {
                        input_buffer.push(c);
                    }
                    KeyCode::Backspace => {
                        input_buffer.pop();
                    }
                    KeyCode::Enter => {
                        if !input_buffer.is_empty() {
                            let user_message = format!("You: {}", input_buffer);
                            messages.push(user_message);

                            let req = Request::SendMessage { message: input_buffer.clone() };
                            let msg = serde_json::to_string(&req)?;
                            tracing::debug!(target: "tui", "Sending request: {}", msg);
                            if let Err(e) = ws_write.send(WsMessage::Text(msg.into())).await {
                                tracing::error!("Failed to send message: {}", e);
                            }
                            input_buffer.clear();
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
