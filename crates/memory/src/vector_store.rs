use anyhow::Result;
use async_trait::async_trait;
use std::sync::Mutex;
use std::collections::HashMap;

#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn add_memory(&self, id: u64, vector: Vec<f32>, text: &str) -> Result<()>;
    async fn search_memory(&self, vector: Vec<f32>, limit: u64) -> Result<Vec<String>>;
}

pub struct InMemoryVectorStore {
    memories: Mutex<HashMap<u64, (Vec<f32>, String)>>,
}

impl InMemoryVectorStore {
    pub fn new() -> Self {
        Self {
            memories: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryVectorStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VectorStore for InMemoryVectorStore {
    async fn add_memory(&self, id: u64, vector: Vec<f32>, text: &str) -> Result<()> {
        let mut mems = self.memories.lock().unwrap();
        mems.insert(id, (vector, text.to_string()));
        Ok(())
    }

    async fn search_memory(&self, vector: Vec<f32>, limit: u64) -> Result<Vec<String>> {
        let mems = self.memories.lock().unwrap();
        let mut scored_mems: Vec<(f32, String)> = mems
            .values()
            .map(|(v, text)| {
                // Calculate cosine similarity manually
                let dot: f32 = v.iter().zip(vector.iter()).map(|(a, b)| a * b).sum();
                let mag_a: f32 = v.iter().map(|a| a * a).sum::<f32>().sqrt();
                let mag_b: f32 = vector.iter().map(|b| b * b).sum::<f32>().sqrt();
                let similarity = if mag_a > 0.0 && mag_b > 0.0 {
                    dot / (mag_a * mag_b)
                } else {
                    0.0
                };
                (similarity, text.clone())
            })
            .collect();

        // Sort by similarity descending
        scored_mems.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        let results = scored_mems
            .into_iter()
            .take(limit as usize)
            .map(|(_, text)| text)
            .collect();

        Ok(results)
    }
}
