use crate::orchestration::research::notebook::ResearchNotebook;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct DeepResearchPlanner {
    pub notebook: Arc<Mutex<ResearchNotebook>>,
}

impl DeepResearchPlanner {
    pub fn new(notebook: Arc<Mutex<ResearchNotebook>>) -> Self {
        Self { notebook }
    }

    pub async fn plan_next_step(&self) -> anyhow::Result<String> {
        let notebook = self.notebook.lock().await;
        if notebook.pending_questions.is_empty() && notebook.verified_facts.is_empty() {
            Ok(format!("Initiating research on: {}", notebook.current_goal))
        } else {
            Ok(format!(
                "Continuing research. Found {} facts, {} pending questions.",
                notebook.verified_facts.len(),
                notebook.pending_questions.len()
            ))
        }
    }
}
