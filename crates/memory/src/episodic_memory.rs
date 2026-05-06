use crate::weaver::KnowledgeNexus;
use anyhow::Result;
use std::sync::Arc;

pub struct EpisodicMemory {
    nexus: Arc<KnowledgeNexus>,
}

impl EpisodicMemory {
    pub fn new(nexus: Arc<KnowledgeNexus>) -> Self {
        Self { nexus }
    }

    pub async fn ingest_trajectory(&self, trajectory_id: &str, content: &str) -> Result<()> {
        // Break trajectory into logical segments if too long, or just index the whole thing
        // For now, index as a single unit or by steps if possible.
        let id = format!("episode_{}", trajectory_id);
        self.nexus
            .remember_batch(vec![(id, content.to_string())])
            .await?;
        Ok(())
    }

    pub async fn query_episodes(&self, query: &str, limit: usize) -> Result<Vec<String>> {
        // We use smart_search which already ranks by relevance, freshness, etc.
        self.nexus.smart_search(query, limit).await
    }
}
