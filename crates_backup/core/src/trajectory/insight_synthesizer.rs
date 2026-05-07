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

        let messages = vec![
            Message {
                role: "system".to_string(),
                content: Some(MessageContent::Text("You are an Meta-Cognitive Analyst. Your job is to extract wisdom from experience.".to_string())),
                ..Default::default()
            },
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
        };

        let response = model.complete(req).await?;
        let insight = response
            .content
            .map(|c| c.to_string())
            .unwrap_or_else(|| "Failed to generate insights.".to_string());

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

        Ok(insight)
    }
}
