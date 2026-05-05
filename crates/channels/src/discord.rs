use async_trait::async_trait;
use anyhow::anyhow;
use pharmakon_core::agent::Agent;
use crate::Channel;
use std::sync::Arc;
use tokio::sync::Mutex;
use serenity::prelude::*;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;

pub struct DiscordChannel {
    pub token: String,
    pub id: String,
    pub http: Arc<serenity::http::Http>,
}

impl DiscordChannel {
    pub fn new(token: String) -> Self {
        let http = Arc::new(serenity::http::Http::new(&token));
        Self { 
            token,
            id: "discord".to_string(),
            http,
        }
    }
}

struct Handler {
    agent: Arc<Mutex<Agent>>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.bot {
            return;
        }

        log::info!("Discord received message: {}", msg.content);
        
        let mut agent_lock = self.agent.lock().await;
        match agent_lock.chat(&msg.content).await {
            Ok(response) => {
                if let Err(e) = msg.channel_id.say(&ctx.http, response).await {
                    log::error!("Discord send error: {}", e);
                }
            }
            Err(e) => {
                log::error!("Agent error in Discord: {}", e);
                let _ = msg.channel_id.say(&ctx.http, format!("Error: {}", e)).await;
            }
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        log::info!("Discord bot {} is connected!", ready.user.name);
    }
}

#[async_trait]
impl Channel for DiscordChannel {
    async fn run(&self, agent: Arc<Mutex<Agent>>) -> anyhow::Result<()> {
        log::info!("DiscordChannel starting...");
        
        let intents = GatewayIntents::GUILD_MESSAGES 
            | GatewayIntents::DIRECT_MESSAGES 
            | GatewayIntents::MESSAGE_CONTENT;

        let mut client = Client::builder(&self.token, intents)
            .event_handler(Handler { agent })
            .await
            .map_err(|e| anyhow!("Error creating Discord client: {}", e))?;

        if let Err(e) = client.start().await {
            log::error!("Discord client error: {}", e);
            return Err(anyhow!("Discord client failed: {}", e));
        }

        Ok(())
    }

    async fn send(&self, target: &str, content: &str) -> anyhow::Result<()> {
        let channel_id: u64 = target.parse()?;
        serenity::model::id::ChannelId::new(channel_id).say(&self.http, content).await
            .map_err(|e| anyhow!("Discord send error: {}", e))?;
        Ok(())
    }

    fn id(&self) -> &str {
        &self.id
    }
}
