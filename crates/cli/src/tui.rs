use anyhow::Result;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, List, ListItem},
    Terminal,
};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};
use futures_util::{StreamExt, SinkExt};
use pharmakon_common::{Event, Request};
use tokio::sync::mpsc;

pub async fn run_tui() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let url = "ws://127.0.0.1:18789/ws";
    let (ws_stream, _) = match connect_async(url).await {
        Ok(v) => v,
        Err(e) => {
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
            if let Ok(event) = serde_json::from_str::<Event>(&text) {
                let _ = tx.send(event).await;
            }
        }
    });

    let mut messages: Vec<String> = Vec::new();
    let mut thoughts: Vec<String> = Vec::new();

    loop {
        // Drain events
        while let Ok(event) = rx.try_recv() {
            match event {
                Event::AgentThought { content } => thoughts.push(format!("Thinking: {}", content)),
                Event::AgentResponse { content } => messages.push(format!("Agent: {}", content)),
                Event::ToolCall { name, args } => thoughts.push(format!("Tool Call: {} ({})", name, args)),
                Event::ToolResult { result } => thoughts.push(format!("Tool Result: {}", result)),
                _ => {}
            }
        }

        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(70),
                    Constraint::Percentage(30),
                ].as_ref())
                .split(f.area());

            let msg_list: Vec<ListItem> = messages.iter().rev().map(|m| ListItem::new(m.as_str())).collect();
            let msg_block = List::new(msg_list)
                .block(Block::default().title("Conversation History").borders(Borders::ALL));
            f.render_widget(msg_block, chunks[0]);

            let thought_list: Vec<ListItem> = thoughts.iter().rev().take(10).map(|m| ListItem::new(m.as_str())).collect();
            let thought_block = List::new(thought_list)
                .block(Block::default().title("Agent Thoughts (Log)").borders(Borders::ALL));
            f.render_widget(thought_block, chunks[1]);
        })?;

        if event::poll(std::time::Duration::from_millis(50))? {
            if let CEvent::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('r') => {
                        let req = Request::ResetHistory;
                        let msg = serde_json::to_string(&req)?;
                        ws_write.send(WsMessage::Text(msg.into())).await?;
                        messages.clear();
                        thoughts.push("History Reset".to_string());
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
