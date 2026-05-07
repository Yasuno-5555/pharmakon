use crate::vector_store::VectorStore;
use anyhow::Result;
use pharmakon_common::EmbeddingModel;

/// SemanticSearch provides long-term memory via embedding-based vector search.
pub struct SemanticSearch {
    vector_store: Box<dyn VectorStore>,
    embedding_model: Box<dyn EmbeddingModel>,
}

impl SemanticSearch {
    pub fn new(
        vector_store: Box<dyn VectorStore>,
        embedding_model: Box<dyn EmbeddingModel>,
    ) -> Self {
        Self {
            vector_store,
            embedding_model,
        }
    }

    /// Remember a piece of text by generating its embedding and storing it.
    pub async fn remember(&self, text: &str) -> Result<()> {
        let vector = self
            .embedding_model
            .generate_embedding(text)
            .await
            .map_err(anyhow::Error::new)?;
        let id = rand::random::<u64>();
        self.vector_store.add_memory(id, vector, text).await
    }

    /// Search for similar memories using embedding-based similarity.
    pub async fn search(&self, query: &str) -> Result<Vec<String>> {
        self.search_with_limit(query, 5).await
    }

    /// Search for similar memories with a specific limit.
    pub async fn search_with_limit(&self, query: &str, limit: u64) -> Result<Vec<String>> {
        let vector = self
            .embedding_model
            .generate_embedding(query)
            .await
            .map_err(anyhow::Error::new)?;
        self.vector_store.search_memory(vector, limit).await
    }

    /// Store a full interaction (user message and assistant response).
    pub async fn store_interaction(&self, user_msg: &str, assistant_msg: &str) -> Result<()> {
        let combined = format!("User: {}\nAssistant: {}", user_msg, assistant_msg);
        self.remember(&combined).await
    }
}
