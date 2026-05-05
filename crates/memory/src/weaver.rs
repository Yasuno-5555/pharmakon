use std::sync::Arc;
use async_trait::async_trait;
use fastembed::{TextEmbedding, InitOptions};
use lancedb::{connect, Connection};
use pharmakon_common::{EmbeddingModel, AgentResult, AgentError};
use arrow::array::{Array, ArrayRef, FixedSizeListArray, Float32Array, RecordBatch, StringArray, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use futures::StreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};

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
        let embeddings = self.model.embed(vec![text], None)
            .map_err(|e| AgentError(format!("Embedding error: {}", e)))?;
        
        Ok(embeddings[0].clone())
    }
}

pub struct MemoryWeaver {
    embedding_model: Arc<LocalEmbeddingModel>,
    conn: Connection,
    table_name: String,
}

impl MemoryWeaver {
    pub async fn new(db_path: &str) -> anyhow::Result<Self> {
        let conn = connect(db_path).execute().await?;
        let embedding_model = Arc::new(LocalEmbeddingModel::new()?);
        
        let table_name = "memories".to_string();
        
        // Ensure table exists
        let table_names = conn.table_names().execute().await?;
        if !table_names.contains(&table_name) {
            let schema = Arc::new(Schema::new(vec![
                Field::new("id", DataType::UInt64, false),
                Field::new("text", DataType::Utf8, false),
                Field::new("vector", DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), 384), false),
            ]));
            
            // Create an empty record batch for initialization
            let batch = RecordBatch::new_empty(schema);
            conn.create_table(&table_name, vec![batch]).execute().await?;
        }

        Ok(Self {
            embedding_model,
            conn,
            table_name,
        })
    }

    pub async fn remember(&self, text: &str) -> anyhow::Result<()> {
        self.remember_batch(vec![text.to_string()]).await
    }

    pub async fn remember_batch(&self, texts: Vec<String>) -> anyhow::Result<()> {
        if texts.is_empty() { return Ok(()); }
        
        let mut meaningful_texts = Vec::new();
        for text in texts {
            if text.trim().is_empty() { continue; }
            
            // Deduplication check
            if let Ok(existing) = self.search(&text, 1).await {
                if let Some(top) = existing.first() {
                    if top.trim() == text.trim() {
                        log::debug!("MemoryWeaver: Skipping duplicate.");
                        continue;
                    }
                }
            }
            meaningful_texts.push(text);
        }

        if meaningful_texts.is_empty() { return Ok(()); }

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::UInt64, false),
            Field::new("text", DataType::Utf8, false),
            Field::new("vector", DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), 384), false),
        ]));

        let mut batches = Vec::new();
        for text in meaningful_texts {
            let model = self.embedding_model.clone();
            let text_clone = text.clone();
            
            let vector = tokio::task::spawn_blocking(move || {
                let rt = tokio::runtime::Handle::current();
                rt.block_on(async move {
                    model.generate_embedding(&text_clone).await
                })
            }).await.map_err(|e| anyhow::anyhow!("Join error: {}", e))?
            .map_err(|e| anyhow::anyhow!("Failed to generate embedding: {}", e))?;
            
            let id = rand::random::<u64>();
            let id_array = UInt64Array::from(vec![id]);
            let text_array = StringArray::from(vec![text]);
            let vector_data = Float32Array::from(vector);
            let vector_array = FixedSizeListArray::try_new(
                Arc::new(Field::new("item", DataType::Float32, true)),
                384,
                Arc::new(vector_data),
                None,
            )?;
            
            batches.push(RecordBatch::try_new(schema.clone(), vec![
                Arc::new(id_array) as ArrayRef,
                Arc::new(text_array) as ArrayRef,
                Arc::new(vector_array) as ArrayRef,
            ])?);
        }

        let table = self.conn.open_table(&self.table_name).execute().await?;
        table.add(batches).execute().await?;
        
        Ok(())
    }

    pub async fn search(&self, query: &str, limit: usize) -> anyhow::Result<Vec<String>> {
        let model = self.embedding_model.clone();
        let query_clone = query.to_string();
        
        let vector = tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async move {
                model.generate_embedding(&query_clone).await
            })
        }).await.map_err(|e| anyhow::anyhow!("Join error: {}", e))?
        .map_err(|e| anyhow::anyhow!("Failed to generate embedding: {}", e))?;
        
        let table = self.conn.open_table(&self.table_name).execute().await?;
        let mut results = table.vector_search(vector)?
            .limit(limit)
            .execute()
            .await?;
        
        let mut documents = Vec::new();
        while let Some(batch_result) = results.next().await {
            let batch: RecordBatch = batch_result?;
            let text_col = batch.column(1).as_any().downcast_ref::<StringArray>()
                .ok_or_else(|| anyhow::anyhow!("Failed to downcast text column"))?;
            
            for i in 0..text_col.len() {
                documents.push(text_col.value(i).to_string());
            }
        }
        
        Ok(documents)
    }
}
