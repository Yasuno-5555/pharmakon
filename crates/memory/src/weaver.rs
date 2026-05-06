use arrow::array::{
    Array, ArrayRef, FixedSizeListArray, Float32Array, RecordBatch, StringArray, UInt32Array,
    UInt64Array,
};
use arrow::datatypes::{DataType, Field, Schema};
use async_trait::async_trait;
use fastembed::{InitOptions, TextEmbedding};
use futures::StreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{Connection, connect};
use pharmakon_common::{AgentError, AgentResult, EmbeddingModel};
use std::sync::Arc;

pub struct LocalEmbeddingModel {
    model: TextEmbedding,
}

impl LocalEmbeddingModel {
    pub fn new() -> anyhow::Result<Self> {
        let model = TextEmbedding::try_new(InitOptions::default())?;
        Ok(Self { model })
    }
}

#[async_trait]
impl EmbeddingModel for LocalEmbeddingModel {
    async fn generate_embedding(&self, text: &str) -> AgentResult<Vec<f32>> {
        let embeddings = self
            .model
            .embed(vec![text], None)
            .map_err(|e| AgentError(format!("Embedding error: {}", e)))?;

        Ok(embeddings[0].clone())
    }
}

pub struct KnowledgeNexus {
    embedding_model: Arc<LocalEmbeddingModel>,
    conn: Connection,
    table_name: String,
    pub graph: Arc<crate::graph::GraphStore>,
}

impl KnowledgeNexus {
    pub async fn new(db_path: &str, graph_db_path: &str) -> anyhow::Result<Self> {
        let conn = connect(db_path).execute().await?;
        let embedding_model = Arc::new(LocalEmbeddingModel::new()?);
        let graph = Arc::new(crate::graph::GraphStore::new(graph_db_path).await?);

        let table_name = "knowledge_units".to_string();

        // Ensure table exists
        let table_names = conn.table_names().execute().await?;
        if !table_names.contains(&table_name) {
            let schema = Arc::new(Schema::new(vec![
                Field::new("id", DataType::Utf8, false),
                Field::new("text", DataType::Utf8, false),
                Field::new("decay_score", DataType::Float32, false),
                Field::new("access_count", DataType::UInt32, false),
                Field::new(
                    "vector",
                    DataType::FixedSizeList(
                        Arc::new(Field::new("item", DataType::Float32, true)),
                        384,
                    ),
                    false,
                ),
            ]));

            // Create an empty record batch for initialization
            let batch = RecordBatch::new_empty(schema);
            conn.create_table(&table_name, vec![batch])
                .execute()
                .await?;
        }

        Ok(Self {
            embedding_model,
            conn,
            table_name,
            graph,
        })
    }

    pub async fn remember_batch(&self, entries: Vec<(String, String)>) -> anyhow::Result<()> {
        if entries.is_empty() {
            return Ok(());
        }

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("text", DataType::Utf8, false),
            Field::new("decay_score", DataType::Float32, false),
            Field::new("access_count", DataType::UInt32, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), 384),
                false,
            ),
        ]));

        let mut batches = Vec::new();
        for (id, text) in entries {
            let model = self.embedding_model.clone();
            let text_clone = text.clone();

            let vector = tokio::task::spawn_blocking(move || {
                let rt = tokio::runtime::Handle::current();
                rt.block_on(async move { model.generate_embedding(&text_clone).await })
            })
            .await
            .map_err(|e| anyhow::anyhow!("Join error: {}", e))?
            .map_err(|e| anyhow::anyhow!("Failed to generate embedding: {}", e))?;

            let id_array = StringArray::from(vec![id]);
            let text_array = StringArray::from(vec![text]);
            let decay_array = Float32Array::from(vec![1.0]);
            let access_array = UInt32Array::from(vec![0]);
            let vector_data = Float32Array::from(vector);
            let vector_array = FixedSizeListArray::try_new(
                Arc::new(Field::new("item", DataType::Float32, true)),
                384,
                Arc::new(vector_data),
                None,
            )?;

            batches.push(RecordBatch::try_new(
                schema.clone(),
                vec![
                    Arc::new(id_array) as ArrayRef,
                    Arc::new(text_array) as ArrayRef,
                    Arc::new(decay_array) as ArrayRef,
                    Arc::new(access_array) as ArrayRef,
                    Arc::new(vector_array) as ArrayRef,
                ],
            )?);
        }

        let table = self.conn.open_table(&self.table_name).execute().await?;
        table.add(batches).execute().await?;

        Ok(())
    }

    pub async fn smart_search(&self, query: &str, limit: usize) -> anyhow::Result<Vec<String>> {
        // 1. Vector Search
        let model = self.embedding_model.clone();
        let query_clone = query.to_string();

        let vector = tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async move { model.generate_embedding(&query_clone).await })
        })
        .await
        .map_err(|e| anyhow::anyhow!("Join error: {}", e))?
        .map_err(|e| anyhow::anyhow!("Failed to generate embedding: {}", e))?;

        let table = self.conn.open_table(&self.table_name).execute().await?;
        let mut results = table.vector_search(vector)?.limit(limit).execute().await?;

        let mut documents = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        while let Some(batch_result) = results.next().await {
            let batch: RecordBatch = batch_result?;
            let id_col = batch
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            let text_col = batch
                .column(1)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();

            for i in 0..text_col.len() {
                let id = id_col.value(i).to_string();
                let text = text_col.value(i).to_string();
                documents.push(text);
                seen_ids.insert(id);
            }
        }

        // 2. Graph Augmentation
        let mut augmented_results = Vec::new();
        for id in seen_ids {
            if let Ok(relations) = self.graph.query_relations(&id).await {
                for rel in relations {
                    augmented_results.push(format!("[Related] {}", rel));
                }
            }
        }

        documents.extend(augmented_results.into_iter().take(5));

        Ok(documents)
    }

    pub async fn decay_memories(&self, factor: f32) -> anyhow::Result<()> {
        let table = self.conn.open_table(&self.table_name).execute().await?;

        // Update: decay_score = decay_score * factor
        table
            .update()
            .column("decay_score", format!("decay_score * {}", factor))
            .execute()
            .await?;

        Ok(())
    }
}
