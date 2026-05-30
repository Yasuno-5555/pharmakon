use crate::Channel;
use anyhow::anyhow;
use async_trait::async_trait;
use pharmakon_common::Event;
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
    pub last_channel_id: Arc<Mutex<Option<serenity::model::id::ChannelId>>>,
}

impl DiscordChannel {
    pub fn new(token: String) -> Self {
        let http = Arc::new(serenity::http::Http::new(&token));
        Self {
            token,
            id: "discord".to_string(),
            http,
            last_channel_id: Arc::new(Mutex::new(None)),
        }
    }
}

struct Handler {
    agent: Arc<Agent>,
    last_channel_id: Arc<Mutex<Option<serenity::model::id::ChannelId>>>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.bot {
            return;
        }

        {
            let mut last_id = self.last_channel_id.lock().await;
            *last_id = Some(msg.channel_id);
        }

        let content = msg.content.trim();

        if content.starts_with("/approve ") {
            let id = content.trim_start_matches("/approve ").to_string();
            self.agent.approve(id.clone(), true);
            let _ = msg
                .channel_id
                .say(&ctx.http, format!("✅ Tool call approved: {}", id))
                .await;
            return;
        }

        if content.starts_with("/deny ") {
            let id = content.trim_start_matches("/deny ").to_string();
            self.agent.approve(id.clone(), false);
            let _ = msg
                .channel_id
                .say(&ctx.http, format!("❌ Tool call denied: {}", id))
                .await;
            return;
        }

        log::info!("Discord received message: {}", content);

        let active_model = {
            let m = self.agent.model.lock().await;
            m.clone()
        };

        let complexity = pharmakon_core::orchestration::scheduler::classify_task_complexity(
            content,
            Some(&active_model),
        )
        .await;

        log::info!("Discord message complexity: {:?}", complexity);

        let w = self.agent.clone();
        let http = ctx.http.clone();
        let cid = msg.channel_id;
        let content_owned = content.to_string();
        let session_id = format!("discord-{}", msg.channel_id);

        match complexity {
            pharmakon_core::orchestration::budget::TaskComplexity::Simple => {
                tokio::spawn(async move {
                    match w.chat_on_session(&content_owned, &session_id).await {
                        Ok(r) => {
                            if !r.is_empty() {
                                let _ = cid.say(&http, r).await;
                            }
                        }
                        Err(e) => {
                            let _ = cid.say(&http, format!("💀 {}", e)).await;
                        }
                    }
                });
            }
            _ => {
                let _ = cid.say(&http, "🟢 Task dispatched to worker agent.\nYou can send other messages or status requests while it runs.").await;
                tokio::spawn(async move {
                    match w.chat_on_session(&content_owned, &session_id).await {
                        Ok(r) => {
                            if !r.is_empty() {
                                let _ = cid.say(&http, r).await;
                            }
                        }
                        Err(e) => {
                            let _ = cid.say(&http, format!("💀 {}", e)).await;
                        }
                    }
                });
            }
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        log::info!("Discord bot {} is connected!", ready.user.name);
    }
}

#[async_trait]
impl Channel for DiscordChannel {
    async fn run(&self, agent: Arc<Agent>) -> anyhow::Result<()> {
        log::info!("DiscordChannel starting...");

        let last_channel_id = self.last_channel_id.clone();
        let http = self.http.clone();
        let agent_for_events = agent.clone();

        // Spawn event listener
        tokio::spawn(async move {
            let event_tx = agent_for_events.event_tx.clone();
            let mut event_rx = event_tx.subscribe();
            log::info!("Discord event listener started.");

            while let Ok(event) = event_rx.recv().await {
                match &event {
                    Event::ApprovalRequest { id, tool, args } => {
                        log::info!("Discord received ApprovalRequest: {}", id);
                        let channel_id_opt = {
                            let last_id = last_channel_id.lock().await;
                            *last_id
                        };

                        if let Some(channel_id) = channel_id_opt {
                            let _ = channel_id.say(&http, format!("🛡️ **Tool Approval Required**\n\n**Tool:** `{}`\n**Args:** `{}`\n\nTo approve, send:\n`/approve {}`\n\nTo deny, send:\n`/deny {}`", tool, args, id, id)).await;
                        }
                    }
                    Event::AgentHangDetected { reason } => {
                        log::warn!("Discord received HangDetected: {}", reason);
                        let channel_id_opt = {
                            let last_id = last_channel_id.lock().await;
                            *last_id
                        };

                        if let Some(channel_id) = channel_id_opt {
                            let _ = channel_id.say(&http, format!("🚨 **Watchdog Alert: Hang Detected**\n\n**Reason:** {}\n\nエージェントの暴走（無限ループ等）を検知したため、安全のためにプロセスを強制停止しました。", reason)).await;
                        }
                    }
                    _ => {}
                }
            }
        });

        let intents = GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::DIRECT_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT;

        let mut client = Client::builder(&self.token, intents)
            .event_handler(Handler {
                agent,
                last_channel_id: self.last_channel_id.clone(),
            })
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
