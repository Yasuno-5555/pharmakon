use crate::agent::Agent;
use crate::orchestration::swarm::SwarmManager;
use pharmakon_common::AgentSpawner;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct InitiativeEngineWorker {
    agent: Arc<Mutex<Agent>>,
    spawner: SwarmManager,
}

impl InitiativeEngineWorker {
    pub fn new(agent: Arc<Mutex<Agent>>) -> Self {
        let spawner = SwarmManager::new(agent.clone());
        Self { agent, spawner }
    }

    pub async fn run_initiative_cycle(&self) -> anyhow::Result<()> {
        log::info!("InitiativeEngine: Starting proactive evaluation cycle...");

        let (model, trajectory_context) = {
            let agent_lock = self.agent.lock().await;

            // Extract recent trajectory to find unresolved threads
            let trajectory = agent_lock.trajectory.lock().await;
            let recent_steps = trajectory
                .steps
                .iter()
                .rev()
                .take(10) // Look at the last 10 steps
                .collect::<Vec<_>>();

            let mut context_summary = String::new();
            for step in recent_steps.iter().rev() {
                // Reverse again to maintain chronological order
                match step {
                    crate::trajectory::TrajectoryStep::Thought { content, .. } => {
                        context_summary.push_str(&format!("Thought: {}\n", content));
                    }
                    crate::trajectory::TrajectoryStep::Action { tool, args, .. } => {
                        let arg_str = serde_json::to_string(&args).unwrap_or_default();
                        context_summary
                            .push_str(&format!("Action: {} with args {}\n", tool, arg_str));
                    }
                    crate::trajectory::TrajectoryStep::Response { content, .. } => {
                        context_summary.push_str(&format!("Agent: {}\n", content));
                    }
                    crate::trajectory::TrajectoryStep::Observation { result, .. } => {
                        // Truncate long observations
                        let truncated = if result.len() > 200 {
                            format!("{}...", &result[..200])
                        } else {
                            result.clone()
                        };
                        context_summary.push_str(&format!("Observation: {}\n", truncated));
                    }
                    crate::trajectory::TrajectoryStep::Intent { goal, .. } => {
                        context_summary.push_str(&format!("Intent: {}\n", goal));
                    }
                }
            }

            (agent_lock.model.clone(), context_summary)
        };

        if trajectory_context.is_empty() {
            log::info!("InitiativeEngine: No recent context to evaluate.");
            return Ok(());
        }

        let system_prompt = "You are the Initiative Engine for an autonomous agent. Your job is to analyze the recent conversation context and identify ONE implicit goal, unresolved problem, or logical next step that the user might want investigated but hasn't explicitly asked for yet. If you find one, describe it as an actionable research task. If there is nothing obvious to do, output exactly 'NO_INITIATIVE_NEEDED'.";
        let user_prompt = format!(
            "Recent Context:\n{}\n\nBased on this context, what is the most valuable proactive task you can start in the background?",
            trajectory_context
        );

        let messages = vec![
            pharmakon_common::Message {
                role: "system".to_string(),
                content: Some(pharmakon_common::MessageContent::Text(
                    system_prompt.to_string(),
                )),
                ..Default::default()
            },
            pharmakon_common::Message {
                role: "user".to_string(),
                content: Some(pharmakon_common::MessageContent::Text(user_prompt)),
                ..Default::default()
            },
        ];

        let request = pharmakon_common::CompletionRequest {
            messages,
            temperature: Some(0.4),
            max_tokens: Some(300),
            tools: None,
        };

        let response = model.lock().await.complete(request).await?;

        let task = match &response.content {
            Some(pharmakon_common::MessageContent::Text(t)) => t.trim().to_string(),
            _ => String::new(),
        };

        if task == "NO_INITIATIVE_NEEDED" || task.is_empty() {
            log::info!("InitiativeEngine: No proactive tasks identified.");
            return Ok(());
        }

        log::info!("InitiativeEngine: Identified proactive task: {}", task);

        // Autonomously spawn a researcher agent to tackle this task
        let role = Some("researcher".to_string());
        match self.spawner.spawn(&task, role, 1).await {
            Ok(msg) => log::info!("InitiativeEngine spawned background task: {}", msg),
            Err(e) => log::error!("InitiativeEngine failed to spawn task: {}", e),
        }

        Ok(())
    }
}
