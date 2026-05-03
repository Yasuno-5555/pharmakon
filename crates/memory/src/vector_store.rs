use qdrant_client::Qdrant;
use qdrant_client::qdrant::{PointStruct, UpsertPointsBuilder, SearchPointsBuilder, CreateCollectionBuilder, VectorParamsBuilder, Distance};
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn add_memory(&self, id: u64, vector: Vec<f32>, text: &str) -> Result<()>;
    async fn search_memory(&self, vector: Vec<f32>, limit: u64) -> Result<Vec<String>>;
}

pub struct QdrantVectorStore {
    client: Qdrant,
    collection_name: String,
}

impl QdrantVectorStore {
    pub async fn new(url: &str) -> Result<Self> {
        let client = Qdrant::from_url(url).build()?;
        let collection_name = "pharmakon_memory".to_string();
        
        // Ensure collection exists
        if !client.collection_exists(&collection_name).await? {
            client.create_collection(
                CreateCollectionBuilder::new(&collection_name)
                    .vectors_config(VectorParamsBuilder::new(1536, Distance::Cosine))
            ).await?;
        }

        Ok(Self { client, collection_name })
    }
}

#[async_trait]
impl VectorStore for QdrantVectorStore {
    async fn add_memory(&self, id: u64, vector: Vec<f32>, text: &str) -> Result<()> {
        let payload: qdrant_client::Payload = std::collections::HashMap::from([
            ("text".to_string(), qdrant_client::qdrant::Value::from(text.to_string()))
        ]).into();

        self.client.upsert_points(
            UpsertPointsBuilder::new(
                &self.collection_name,
                vec![PointStruct::new(id, vector, payload)]
            )
        ).await?;

        Ok(())
    }

    async fn search_memory(&self, vector: Vec<f32>, limit: u64) -> Result<Vec<String>> {
        let response = self.client.search_points(
            SearchPointsBuilder::new(&self.collection_name, vector, limit)
                .with_payload(true)
        ).await?;

        let mut results = Vec::new();
        for result in response.result {
            if let Some(text) = result.payload.get("text").and_then(|v| v.as_str()) {
                results.push(text.to_string());
            }
        }

        Ok(results)
    }
}
