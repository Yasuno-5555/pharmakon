use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub mod insight_synthesizer;
pub mod tool;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TrajectoryStep {
    Intent {
        goal: String,
        intent_type: String, // e.g., "fix_bug", "refactor", "explore"
        confidence: f32,
        timestamp: DateTime<Utc>,
    },
    Thought {
        content: String,
        timestamp: DateTime<Utc>,
    },
    Action {
        tool: String,
        args: serde_json::Value,
        intent_id: Option<usize>, // Link back to the intent
        timestamp: DateTime<Utc>,
    },
    Observation {
        result: String,
        action_id: Option<usize>, // Link back to the action
        timestamp: DateTime<Utc>,
    },
    Response {
        content: String,
        timestamp: DateTime<Utc>,
    },
}

#[derive(Serialize, Deserialize)]
pub struct Trajectory {
    pub session_id: String,
    pub steps: Vec<TrajectoryStep>,
    pub metadata: TrajectoryMetadata,
}

#[derive(Serialize, Deserialize)]
pub struct TrajectoryMetadata {
    pub model: String,
    pub created_at: DateTime<Utc>,
}

impl Trajectory {
    pub fn new(session_id: String, model: String) -> Self {
        Self {
            session_id,
            steps: Vec::new(),
            metadata: TrajectoryMetadata {
                model,
                created_at: Utc::now(),
            },
        }
    }

    pub fn add_step(&mut self, step: TrajectoryStep) {
        self.steps.push(step);
    }

    pub fn to_markdown(&self) -> String {
        let mut md = format!("# Trajectory for session: {}\n", self.session_id);
        md.push_str(&format!("- **Model**: {}\n", self.metadata.model));
        md.push_str(&format!("- **Date**: {}\n\n", self.metadata.created_at));

        for (i, step) in self.steps.iter().enumerate() {
            match step {
                TrajectoryStep::Intent { goal, intent_type, confidence, timestamp } => {
                    md.push_str(&format!(
                        "#### 🎯 Intent #{} ({})\n**Type**: `{}` (Confidence: {:.2})\n**Goal**: {}\n\n",
                        i,
                        timestamp.format("%H:%M:%S"),
                        intent_type,
                        confidence,
                        goal
                    ));
                }
                TrajectoryStep::Thought { content, timestamp } => {
                    md.push_str(&format!(
                        "#### 💭 Thought ({})\n{}\n\n",
                        timestamp.format("%H:%M:%S"),
                        content
                    ));
                }
                TrajectoryStep::Action {
                    tool,
                    args,
                    intent_id,
                    timestamp,
                } => {
                    let link = intent_id.map(|id| format!(" (from Intent #{})", id)).unwrap_or_default();
                    md.push_str(&format!(
                        "#### 🛠️ Action #{} ({}){})\nTool: `{}`\nArgs: ```json\n{}\n```\n\n",
                        i,
                        timestamp.format("%H:%M:%S"),
                        link,
                        tool,
                        serde_json::to_string_pretty(args).unwrap_or_default()
                    ));
                }
                TrajectoryStep::Observation { result, action_id, timestamp } => {
                    let link = action_id.map(|id| format!(" (from Action #{})", id)).unwrap_or_default();
                    md.push_str(&format!(
                        "#### 👁️ Observation ({}){})\n```\n{}\n```\n\n",
                        timestamp.format("%H:%M:%S"),
                        link,
                        result
                    ));
                }
                TrajectoryStep::Response { content, timestamp } => {
                    md.push_str(&format!(
                        "#### ✅ Response ({})\n{}\n\n",
                        timestamp.format("%H:%M:%S"),
                        content
                    ));
                }
            }
        }
        md
    }

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}
