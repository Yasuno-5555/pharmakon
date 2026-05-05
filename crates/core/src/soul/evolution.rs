use std::sync::Arc;
use tokio::sync::Mutex;
use crate::agent::Agent;
use pharmakon_common::{CompletionRequest, Message, MessageContent};

pub struct SoulEvolutionWorker {
    agent: Arc<Mutex<Agent>>,
}

impl SoulEvolutionWorker {
    pub fn new(agent: Arc<Mutex<Agent>>) -> Self {
        Self { agent }
    }

    pub async fn evolve_cycle(&self) -> anyhow::Result<()> {
        log::info!("Starting Soul Evolution cycle...");
        
        let (model, current_soul, recent_context) = {
            let agent_lock = self.agent.lock().await;
            let trajectory_summary = agent_lock.trajectory.steps.iter().filter_map(|step| {
                match step {
                    crate::trajectory::TrajectoryStep::Response { content, .. } => Some(content.clone()),
                    _ => None
                }
            }).collect::<Vec<_>>().join("\n");
            
            (agent_lock.model.clone(), agent_lock.prompt_manager.soul().clone(), trajectory_summary)
        };

        if recent_context.is_empty() {
            log::info!("No recent interactions for soul evolution. Skipping.");
            return Ok(());
        }

        let system_prompt = "You are the Soul Distiller. Your job is to analyze recent user interactions and extract lasting preferences, values, and traits to update the agent's core identity.";
        let user_prompt = format!(
            "Current Soul Traits: {:?}\n\nRecent Interactions:\n{}\n\nBased on this, suggest 2-3 new traits or updates to the system prompt to better align with the user. Output in YAML format with 'traits' and 'prompt_addition' fields.",
            current_soul.traits,
            recent_context
        );

        let request = CompletionRequest {
            messages: vec![
                Message { role: "system".to_string(), content: Some(MessageContent::Text(system_prompt.to_string())), ..Default::default() },
                Message { role: "user".to_string(), content: Some(MessageContent::Text(user_prompt)), ..Default::default() },
            ],
            temperature: Some(0.3),
            max_tokens: Some(500),
            tools: None,
        };

        let response = model.complete(request).await.map_err(|e| anyhow::anyhow!("Evolution LLM error: {}", e))?;
        
        if let Some(content) = response.content {
            let text = content.to_string();
            log::info!("Evolution suggestions received: {}", text);
            
            // Apply updates to the agent's soul
            let mut agent_lock = self.agent.lock().await;
            // Simple string append for now
            if text.contains("traits:") {
                agent_lock.prompt_manager.add_contribution(Box::new(crate::system_prompt::StaticContribution::new(
                    "Learned Preferences",
                    &text
                )));
            }
        }

        log::info!("Soul Evolution cycle completed.");
        Ok(())
    }
}
