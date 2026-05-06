use crate::orchestration::research::notebook::ResearchNotebook;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct DeepResearchPlanner {
    pub notebook: Arc<Mutex<ResearchNotebook>>,
    pub max_depth: u8,
    pub beam_width: usize,
}

impl DeepResearchPlanner {
    pub fn new(notebook: Arc<Mutex<ResearchNotebook>>, max_depth: u8, beam_width: usize) -> Self {
        Self { notebook, max_depth, beam_width }
    }

    pub async fn plan_next_step(&self) -> anyhow::Result<String> {
        let mut notebook = self.notebook.lock().await;
        
        if notebook.should_stop() {
            return Ok("RESEARCH_COMPLETE: Goal achieved or information gain saturated.".to_string());
        }

        notebook.step_count += 1;

        if notebook.pending_questions.is_empty() && notebook.verified_facts.is_empty() {
            Ok(format!("INITIAL_SEARCH: {}", notebook.current_goal))
        } else if !notebook.pending_questions.is_empty() {
            // Beam search logic: pick the most promising pending question
            let next_q = notebook.pending_questions.remove(0);
            Ok(format!("EXPLORE_BRANCH: {}", next_q))
        } else {
            Ok("CONSOLIDATE: Summarizing findings.".to_string())
        }
    }
}
