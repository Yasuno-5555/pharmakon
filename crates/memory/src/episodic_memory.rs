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
        let chunk_size = 2000;
        if content.len() <= chunk_size {
            let id = format!("episode_{}", trajectory_id);
            self.nexus
                .remember_batch(vec![(id, content.to_string())])
                .await?;
        } else {
            let mut chunks = Vec::new();
            let mut remaining = content;
            let mut index = 0;
            while !remaining.is_empty() {
                // Find the closest char boundary at or before chunk_size
                let char_boundary = remaining
                    .char_indices()
                    .take_while(|(i, _)| *i <= chunk_size)
                    .last()
                    .map(|(i, _)| i)
                    .unwrap_or(0);

                if char_boundary == 0 {
                    let next_char_len = remaining.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                    let (chunk, rest) = remaining.split_at(next_char_len);
                    chunks.push((
                        format!("episode_{}_{}", trajectory_id, index),
                        chunk.to_string(),
                    ));
                    remaining = rest.trim_start();
                    index += 1;
                    continue;
                }

                if remaining.len() <= char_boundary {
                    chunks.push((
                        format!("episode_{}_{}", trajectory_id, index),
                        remaining.to_string(),
                    ));
                    break;
                }

                let sub_str = &remaining[..char_boundary];
                let split_pos = sub_str.rfind('\n').unwrap_or(char_boundary);
                let (chunk, rest) = remaining.split_at(split_pos);
                chunks.push((
                    format!("episode_{}_{}", trajectory_id, index),
                    chunk.to_string(),
                ));
                remaining = rest.trim_start();
                index += 1;
            }
            if !chunks.is_empty() {
                self.nexus.remember_batch(chunks).await?;
            }
        }
        Ok(())
    }

    pub async fn query_episodes(&self, query: &str, limit: usize) -> Result<Vec<String>> {
        // We use smart_search which already ranks by relevance, freshness, etc.
        self.nexus.smart_search(query, limit).await
    }
}
