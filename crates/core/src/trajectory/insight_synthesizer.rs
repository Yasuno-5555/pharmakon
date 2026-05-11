use crate::agent::Agent;
use crate::model::{CompletionRequest, Message, MessageContent};
use anyhow::Result;
use std::sync::Arc;

pub struct InsightSynthesizer;

impl InsightSynthesizer {
    pub async fn synthesize(agent: Arc<Agent>) -> Result<String> {
        let trajectory = agent.trajectory.lock().await;
        if trajectory.steps.is_empty() {
            return Ok("Trajectory is empty. No insights to synthesize.".to_string());
        }

        let markdown = trajectory.to_markdown();
        let prompt = format!(
            "Analyze the following agent trajectory and synthesize 'Lessons Learned'. \
             Identify what worked, what failed, and provide actionable advice for future agents working on similar tasks. \
             Focus on architectural insights, tool usage patterns, and common pitfalls. \n\n\
             TRAJECTORY:\n{}",
            markdown
        );

        let system_prompt = "You are an Meta-Cognitive Analyst. Your job is to extract wisdom from experience.";
        let messages = vec![
            Message {
                role: "user".to_string(),
                content: Some(MessageContent::Text(prompt)),
                ..Default::default()
            },
        ];

        let model = {
            let m = agent.model.lock().await;
            (*m).clone()
        };

        let req = CompletionRequest {
            messages,
            temperature: Some(0.3),
            max_tokens: Some(2048),
            tools: None,
            complexity: None,
            system_instruction: Some(system_prompt.to_string()),
        };

        let response = model.complete(req).await?;
        let insight = response
            .content
            .map(|c| c.to_string())
            .unwrap_or_else(|| "Failed to generate insights.".to_string());

        // MANDATE VALIDATION: Prevent harmful self-correction
        let mandate_prompt = format!(
            "The following 'Lesson Learned' has been proposed for the project. \
             Should any part of this be elevated to an 'Engineering Mandate' in PHARMAKON.md? \
             Only elevate items that are fundamental architectural constraints or proven best practices for this codebase. \
             If yes, output only the valid Markdown snippet to append. If no, output 'NONE'. \n\n\
             PROPOSAL:\n{}",
            insight
        );

        let critic_system_prompt = "You are a Senior Architectural Critic. You are extremely conservative and only approve mandates that are universally true for this project.";
        let critic_req = CompletionRequest {
            messages: vec![
                Message {
                    role: "user".to_string(),
                    content: Some(MessageContent::Text(mandate_prompt)),
                    ..Default::default()
                },
            ],
            temperature: Some(0.1),
            max_tokens: Some(512),
            tools: None,
            complexity: None,
            system_instruction: Some(critic_system_prompt.to_string()),
        };

        if let Ok(critic_resp) = model.complete(critic_req).await {
            let mandate = critic_resp.content.map(|c| c.to_string()).unwrap_or_default();
            if !mandate.contains("NONE") && mandate.len() > 10 {
                let mandate_path = std::path::PathBuf::from("PHARMAKON.md");
                let mut existing = std::fs::read_to_string(&mandate_path).unwrap_or_default();
                existing.push_str("\n### [Automated Mandate] ");
                existing.push_str(&chrono::Utc::now().format("%Y-%m-%d").to_string());
                existing.push('\n');
                existing.push_str(&mandate);
                existing.push('\n');
                let _ = std::fs::write(&mandate_path, existing);
                log::info!("PHARMAKON.md updated with new mandate validated by critic.");
            }
        }

        // Save to file
        let path = std::path::PathBuf::from(".pharmakon/knowledge/lessons_learned.md");
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let new_content = format!(
            "## Session Insight ({})\n\n{}\n\n---\n\n{}",
            chrono::Utc::now().to_rfc3339(),
            insight,
            existing
        );
        std::fs::write(&path, new_content)?;

        // Trigger background Ollama distillation automatically on insight synthesis
        if let Some(store) = &agent.session_store {
            let store_clone = store.clone();
            tokio::spawn(async move {
                let distiller = crate::orchestration::ollama_distiller::OllamaDistiller::new(store_clone);
                // Distill from default llama3.2 to our target pharmakon-distilled
                if let Err(e) = distiller.distill("llama3.2", "pharmakon-distilled").await {
                    log::warn!("Auto background Ollama distillation skipped/failed: {}", e);
                }
            });
        }

        Ok(insight)
    }
}
