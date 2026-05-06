use crate::Channel;
use anyhow::anyhow;
use async_trait::async_trait;
use pharmakon_core::agent::Agent;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use std::sync::Arc;
use tokio::sync::Mutex;

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
    agent: Arc<Agent>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.bot {
            return;
        }

        log::info!("Discord received message: {}", msg.content);

        let agent_clone = self.agent.clone();
        let content = msg.content.clone();
        tokio::spawn(async move {
            match agent_clone.chat(&content).await {
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
        });
    }

    async fn ready(&self, _: Context, ready: Ready) {
        log::info!("Discord bot {} is connected!", ready.user.name);
    }
}

#[async_trait]
impl Channel for DiscordChannel {
    async fn run(&self, agent: Arc<Agent>) -> anyhow::Result<()> {
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
        serenity::model::id::ChannelId::new(channel_id)
            .say(&self.http, content)
            .await
            .map_err(|e| anyhow!("Discord send error: {}", e))?;
        Ok(())
    }

    fn id(&self) -> &str {
        &self.id
    }
}
