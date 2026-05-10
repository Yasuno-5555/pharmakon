use crate::Channel;
use async_trait::async_trait;
use pharmakon_common::Event;
use pharmakon_core::agent::Agent;
use std::sync::Arc;
use teloxide::prelude::*;
use tokio::sync::Mutex;

pub struct TelegramChannel {
    pub token: String,
    pub bot: Bot,
    pub last_chat_id: Arc<Mutex<Option<ChatId>>>,
    pub chat_sessions: Arc<Mutex<std::collections::HashMap<ChatId, String>>>,
}

impl TelegramChannel {
    pub fn new(token: String) -> Self {
        let bot = Bot::new(token.clone());
        Self {
            token,
            bot,
            last_chat_id: Arc::new(Mutex::new(None)),
            chat_sessions: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }
}

#[async_trait]
impl Channel for TelegramChannel {
    async fn run(&self, agent: Arc<Agent>) -> anyhow::Result<()> {
        log::info!("TelegramChannel starting...");

        let bot = self.bot.clone();

        // Wait for connection to be available (simple retry)
        let mut retry_count = 0;
        while retry_count < 3 {
            match bot.get_me().await {
                Ok(me) => {
                    log::info!(
                        "Telegram bot {} is online!",
                        me.user.username.unwrap_or_default()
                    );
                    break;
                }
                Err(e) => {
                    log::warn!(
                        "Telegram connection failed (retry {}/3): {}",
                        retry_count + 1,
                        e
                    );
                    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                    retry_count += 1;
                }
            }
        }

        if retry_count == 3 {
            log::error!("Telegram failed to connect. Channel will remain inactive.");
            return Ok(()); // Don't crash the whole server
        }

        let handler = dptree::entry()
            .branch(Update::filter_message().endpoint(move |bot: Bot, msg: Message, agent: Arc<Agent>, last_chat_id: Arc<Mutex<Option<ChatId>>>, chat_sessions: Arc<Mutex<std::collections::HashMap<ChatId, String>>>| async move {
                {
                    let mut last_id = last_chat_id.lock().await;
                    *last_id = Some(msg.chat.id);
                }

                let session_id = {
                    let mut sessions = chat_sessions.lock().await;
                    sessions.entry(msg.chat.id)
                        .or_insert_with(|| format!("telegram-{}", msg.chat.id))
                        .clone()
                };

                if let Some(text) = msg.text() {
                    log::info!("Telegram received message from {}: {}", msg.chat.id, text);

                    if text.starts_with("/approve ") {
                        let id = text.trim_start_matches("/approve ").to_string();
                        agent.approve(id.clone(), true);
                        let _ = bot.send_message(msg.chat.id, format!("✅ Tool call approved: {}", id)).await;
                        return Ok(());
                    }

                    if text.starts_with("/deny ") {
                        let id = text.trim_start_matches("/deny ").to_string();
                        agent.approve(id.clone(), false);
                        let _ = bot.send_message(msg.chat.id, format!("❌ Tool call denied: {}", id)).await;
                        return Ok(());
                    }

                    // Auto-index user message for future recovery (DISABLED to prevent cross-session memory leaks)
                    /*
                    if user_message.len() > 10 {
                        if let Some(nexus) = &self.knowledge_nexus {
                            let id = uuid::Uuid::new_v4().to_string();
                            let _ = nexus
                                .remember_batch(vec![(id, user_message.to_string())])
                                .await;
                        }
                    }
                    */

                    if text == "/new" {
                        log::info!("Telegram: Resetting session as requested by user.");
                        let new_id = uuid::Uuid::new_v4().to_string();
                        {
                            let mut sessions = chat_sessions.lock().await;
                            sessions.insert(msg.chat.id, new_id.clone());
                        }
                        let _ = agent.reset_session_history(&session_id).await;
                        let _ = bot.send_message(msg.chat.id, "🔄 New session started. Previous context has been cleared for this chat.").await;
                        return Ok(());
                    }

                    let pairing_mgr = pharmakon_core::security::pairing::PairingManager::global();
                    let sender_id = msg.chat.id.to_string();

                    if !pairing_mgr.is_allowed("telegram", &sender_id) {
                        if text.starts_with("/") {
                             // Allow commands like /start?
                        } else {
                            let code = pairing_mgr.initiate_pairing("telegram", &sender_id);
                            let _ = bot.send_message(msg.chat.id, format!("🦞 Welcome to Pharmakon! This channel is currently locked.\n\nTo pair this device, run the following command in your terminal:\n\n`pharmakon pairing approve telegram {}`", code)).await;
                            return Ok(());
                        }
                    }

                    let text_owned = text.to_string();
                    let is_command = text_owned.starts_with("/");
                    if is_command {
                        let al = agent.clone();
                        match al.chat_on_session(&text_owned, &session_id).await {
                            Ok(r) => { if !r.is_empty() { let _ = bot.send_message(msg.chat.id, r).await; } }
                            Err(e) => { let _ = bot.send_message(msg.chat.id, format!("Error: {}", e)).await; }
                        }
                    } else {
                        let active_model = {
                            let m = agent.model.lock().await;
                            (*m).clone()
                        };

                        let complexity = pharmakon_core::orchestration::scheduler::classify_task_complexity(
                            &text_owned,
                            Some(&active_model),
                        ).await;

                        log::info!("Telegram message complexity: {:?}", complexity);

                        let w = agent.clone();
                        let b = bot.clone();
                        let cid = msg.chat.id;

                        match complexity {
                            pharmakon_core::orchestration::budget::TaskComplexity::Simple => {
                                // Simple conversational query: answer immediately on main agent/session
                                match w.chat_on_session(&text_owned, &session_id).await {
                                    Ok(r) => { if !r.is_empty() { let _ = b.send_message(cid, r).await; } }
                                    Err(e) => { let _ = b.send_message(cid, format!("💀 {}", e)).await; }
                                }
                            }
                            _ => {
                                // Complex engineering task: spawn a dedicated worker agent in a separate session
                                let worker_session_id = format!("worker-{}-{}", session_id, uuid::Uuid::new_v4());
                                let _ = b.send_message(
                                    cid, 
                                    format!(
                                        "🟢 Task dispatched to dedicated background Worker Agent [Session: {}].\n\
                                         You can continue chatting with me here while it works in the background!", 
                                        worker_session_id
                                    )
                                ).await;

                                // Build the dedicated worker agent
                                let mut worker_agent = Agent::new(active_model.clone(), worker_session_id.clone());
                                if let Some(store) = &w.session_store {
                                    worker_agent = worker_agent.with_store(store.clone());
                                }
                                if let Some(nexus) = &w.knowledge_nexus {
                                    worker_agent = worker_agent.with_knowledge_nexus(nexus.clone()).with_isolated_knowledge();
                                }
                                if let Some(search) = &w.semantic_search {
                                    worker_agent = worker_agent.with_semantic_search(search.clone());
                                }
                                worker_agent.fact_memory = w.fact_memory.clone();
                                worker_agent.territory_manager = w.territory_manager.clone();
                                worker_agent.set_soul(pharmakon_core::soul::Soul::expert("coder")).await;

                                tokio::spawn(async move {
                                    match worker_agent.chat(&text_owned).await {
                                        Ok(r) => {
                                            if !r.is_empty() {
                                                let _ = b.send_message(
                                                    cid,
                                                    format!("🏁 **Worker Agent Completed Task (Session: {})**\n\n{}", worker_session_id, r)
                                                ).await;
                                            }
                                        }
                                        Err(e) => {
                                            let _ = b.send_message(
                                                cid,
                                                format!("⚠️ **Worker Agent Failed (Session: {})**\n\n💀 {}", worker_session_id, e)
                                            ).await;
                                        }
                                    }
                                });
                            }
                        }
                    }
                }
                anyhow::Result::<()>::Ok(())
            }));

        let mut dispatcher = Dispatcher::builder(bot.clone(), handler)
            .dependencies(dptree::deps![
                agent.clone(),
                self.last_chat_id.clone(),
                self.chat_sessions.clone()
            ])
            .enable_ctrlc_handler()
            .build();

        // Spawn event listener
        let bot_for_events = bot.clone();
        let agent_for_events = agent.clone();
        let last_chat_id = self.last_chat_id.clone();
        tokio::spawn(async move {
            let event_tx = agent_for_events.event_tx.clone();
            let mut event_rx = event_tx.subscribe();
            log::info!("Telegram event listener started.");

            while let Ok(event) = event_rx.recv().await {
                match &event {
                    Event::ApprovalRequest { id, tool, args } => {
                        log::info!("Telegram received ApprovalRequest: {}", id);
                        let chat_id_opt = {
                            let last_id = last_chat_id.lock().await;
                            *last_id
                        };

                        if let Some(chat_id) = chat_id_opt {
                            let _ = bot_for_events.send_message(chat_id, format!("🛡️ **Tool Approval Required**\n\n**Tool:** `{}`\n**Args:** `{}`\n\nTo approve, send:\n`/approve {}`\n\nTo deny, send:\n`/deny {}`", tool, args, id, id)).await;
                        }
                    }
                    Event::AgentHangDetected { reason } => {
                        log::warn!("Telegram received HangDetected: {}", reason);
                        let chat_id_opt = {
                            let last_id = last_chat_id.lock().await;
                            *last_id
                        };

                        if let Some(chat_id) = chat_id_opt {
                            let _ = bot_for_events.send_message(chat_id, format!("🚨 **Watchdog Alert: Hang Detected**\n\n**Reason:** {}\n\nエージェントの暴走（無限ループ等）を検知したため、安全のためにプロセスを強制停止しました。コンテキストをクリアするために `/new` コマンドの実行をお勧めします。", reason)).await;
                        }
                    }
                    _ => {}
                }
            }
        });

        dispatcher.dispatch().await;

        Ok(())
    }

    async fn send(&self, target: &str, content: &str) -> anyhow::Result<()> {
        let chat_id: i64 = target.parse()?;
        self.bot.send_message(ChatId(chat_id), content).await?;
        Ok(())
    }

    fn id(&self) -> &str {
        "telegram"
    }
}
