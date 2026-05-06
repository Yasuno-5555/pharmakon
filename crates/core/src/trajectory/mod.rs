use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum TrajectoryStep {
    Thought {
        content: String,
        timestamp: DateTime<Utc>,
    },
    Action {
        tool: String,
        args: serde_json::Value,
        timestamp: DateTime<Utc>,
    },
    Observation {
        result: String,
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

        for step in &self.steps {
            match step {
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
                    timestamp,
                } => {
                    md.push_str(&format!(
                        "#### 🛠️ Action ({})\nTool: `{}`\nArgs: ```json\n{}\n```\n\n",
                        timestamp.format("%H:%M:%S"),
                        tool,
                        serde_json::to_string_pretty(args).unwrap_or_default()
                    ));
                }
                TrajectoryStep::Observation { result, timestamp } => {
                    md.push_str(&format!(
                        "#### 👁️ Observation ({})\n```\n{}\n```\n\n",
                        timestamp.format("%H:%M:%S"),
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
