use async_trait::async_trait;
use anyhow::anyhow;
use pharmakon_core::agent::Agent;
use crate::Channel;
use std::sync::Arc;
use tokio::sync::Mutex;
use teloxide::prelude::*;

pub struct TelegramChannel {
    pub token: String,
    pub bot: Bot,
}

impl TelegramChannel {
    pub fn new(token: String) -> Self {
        let bot = Bot::new(token.clone());
        Self { token, bot }
    }
}

#[async_trait]
impl Channel for TelegramChannel {
    async fn run(&self, agent: Arc<Mutex<Agent>>) -> anyhow::Result<()> {
        log::info!("TelegramChannel starting...");
        
        let bot = self.bot.clone();
        
        teloxide::repl(bot, move |bot: Bot, msg: Message| {
            let agent = agent.clone();
            async move {
                if let Some(text) = msg.text() {
                    log::info!("Telegram received message from {}: {}", msg.chat.id, text);
                    
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

                    let mut agent_lock = agent.lock().await;
                    match agent_lock.chat(text).await {
                        Ok(response) => {
                            bot.send_message(msg.chat.id, response).await?;
                        }
                        Err(e) => {
                            log::error!("Agent error in Telegram: {}", e);
                            bot.send_message(msg.chat.id, format!("Error: {}", e)).await?;
                        }
                    }
                }
                Ok(())
            }
        }).await;

        // teloxide::repl runs forever, so we won't actually reach here unless it fails.
        Err(anyhow!("Telegram REPL exited unexpectedly"))
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
