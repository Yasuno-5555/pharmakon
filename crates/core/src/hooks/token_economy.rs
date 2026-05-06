use crate::hooks::{Hook, HookContext};
use async_trait::async_trait;
use pharmakon_common::{Event, Message, MessageContent};
use std::sync::Arc;
use tokio::sync::Mutex;

use std::sync::atomic::{AtomicBool, Ordering};

pub struct TokenEconomyHook {
    enabled: Arc<AtomicBool>,
    threshold: f32,
    budget_limit: Arc<Mutex<u64>>,
    cumulative_usage: Arc<Mutex<u64>>,
}

impl TokenEconomyHook {
    pub fn new(threshold: f32, budget_limit: u64) -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(true)),
            threshold,
            budget_limit: Arc::new(Mutex::new(budget_limit)),
            cumulative_usage: Arc::new(Mutex::new(0)),
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
    }

    pub async fn set_budget(&self, budget: u64) {
        let mut b = self.budget_limit.lock().await;
        *b = budget;
    }
}

#[async_trait]
impl Hook for TokenEconomyHook {
    fn name(&self) -> &str {
        "token_economy_v2"
    }

    async fn on_event(&self, ctx: &mut HookContext, event: &Event) -> anyhow::Result<()> {
        if !self.enabled.load(Ordering::SeqCst) {
            return Ok(());
        }

        match event {
            Event::InteractionFinished { response, .. } => {
                let usage = response.usage.as_ref();
                if let Some(u) = usage {
                    let mut total = self.cumulative_usage.lock().await;
                    *total += u.total_tokens as u64;

                    let limit = *self.budget_limit.lock().await;
                    log::info!(
                        "💰 Token Usage: +{} (Total: {} / Budget: {})",
                        u.total_tokens,
                        *total,
                        limit
                    );

                    if *total > limit {
                        log::warn!(
                            "🚨 TOKEN BUDGET EXCEEDED! ({} > {})",
                            *total,
                            limit
                        );
                        // Inject a strong warning for the next turn
                        let mut history = ctx.agent.history.lock().await;
                        history.push(Message {
                            role: "system".to_string(),
                            content: Some(MessageContent::Text(format!(
                                "CRITICAL: You have exceeded your token budget ({} / {}). \
                                From now on, you MUST be extremely concise. No explanations, just direct answers or essential tool calls. \
                                Failure to comply will result in task termination.", 
                                *total, limit
                            ))),
                            ..Default::default()
                        });
                    } else if u.total_tokens > 2000 {
                        // Single message was expensive
                        let mut history = ctx.agent.history.lock().await;
                        history.push(Message {
                            role: "system".to_string(),
                            content: Some(MessageContent::Text(
                                "WARNING: Your last response was quite large. Please prioritize precision and brevity in future turns to save tokens."
                                .to_string()
                            )),
                            ..Default::default()
                        });
                    }
                }

                // Also trigger compaction if history is long
                let len = ctx.agent.history.lock().await.len();
                if len > 15 {
                    let agent = ctx.agent.clone();
                    tokio::spawn(async move {
                        let mut history_lock = agent.history.lock().await;
                        let compactor = agent.compactor.lock().await;
                        let _ = compactor.compact(history_lock.clone()).await.map(|new_h| {
                            *history_lock = new_h;
                        });
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }
}
