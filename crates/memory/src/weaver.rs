use arrow::array::{
    Array, ArrayRef, FixedSizeListArray, Float32Array, RecordBatch, StringArray, UInt32Array,
};
use arrow::datatypes::{DataType, Field, Schema};
use async_trait::async_trait;
use fastembed::{InitOptions, TextEmbedding};
use futures::StreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{Connection, connect};
use pharmakon_common::{AgentError, AgentResult, EmbeddingModel};
use std::sync::Arc;
use std::collections::HashMap;

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

use tokio::sync::Mutex;

pub struct EmbeddingJob {
    pub id: String,
    pub text: String,
}

pub struct KnowledgeNexus {
    embedding_model: Arc<LocalEmbeddingModel>,
    conn: Arc<Mutex<Connection>>,
    table_name: String,
    pub graph: Arc<crate::graph::GraphStore>,
    // Asynchronous embedding queue sender
    pub tx: tokio::sync::mpsc::Sender<EmbeddingJob>,
    // Isolated delta buffers
    local_nodes: Arc<Mutex<Vec<crate::graph::Node>>>,
    local_edges: Arc<Mutex<Vec<crate::graph::Edge>>>,
    // Base state for 3-way merge
    base_node_ids: Arc<Vec<String>>,
    is_isolated: bool,
}

impl KnowledgeNexus {
    pub async fn new(db_path: &str, graph_db_path: &str) -> anyhow::Result<Self> {
        let conn = Arc::new(Mutex::new(connect(db_path).execute().await?));
        let embedding_model = Arc::new(LocalEmbeddingModel::new()?);
        let graph = Arc::new(crate::graph::GraphStore::new(graph_db_path).await?);

        let table_name = "knowledge_units".to_string();

        // Ensure table exists in LanceDB
        let table_names = conn.lock().await.table_names().execute().await?;
        if !table_names.contains(&table_name) {
            let schema = Arc::new(Schema::new(vec![
                Field::new("id", DataType::Utf8, false),
                Field::new("text", DataType::Utf8, false),
                Field::new("decay_score", DataType::Float32, false),
                Field::new("access_count", DataType::UInt32, false),
                Field::new("node_type", DataType::Utf8, false),
                Field::new(
                    "vector",
                    DataType::FixedSizeList(
                        Arc::new(Field::new("item", DataType::Float32, true)),
                        384,
                    ),
                    false,
                ),
            ]));

            let batch = RecordBatch::new_empty(schema);
            conn.lock()
                .await
                .create_table(&table_name, vec![batch])
                .execute()
                .await?;
        }

        // Initialize high-performance, bounded non-blocking embedding queue with backpressure control
        let (tx, mut rx) = tokio::sync::mpsc::channel::<EmbeddingJob>(1000);

        let conn_clone = conn.clone();
        let embedding_model_clone = embedding_model.clone();
        let graph_clone = graph.clone();
        let table_name_clone = table_name.clone();

        tokio::spawn(async move {
            log::info!("🚀 KnowledgeNexus: Embedding Queue Worker Started.");
            let mut deduplication_cache: HashMap<String, Vec<f32>> = HashMap::new();

            let schema = Arc::new(Schema::new(vec![
                Field::new("id", DataType::Utf8, false),
                Field::new("text", DataType::Utf8, false),
                Field::new("decay_score", DataType::Float32, false),
                Field::new("access_count", DataType::UInt32, false),
                Field::new("node_type", DataType::Utf8, false),
                Field::new(
                    "vector",
                    DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), 384),
                    false,
                ),
            ]));

            while let Some(job) = rx.recv().await {
                let node_info = match graph_clone.get_node(&job.id).await {
                    Ok(Some(n)) => n,
                    _ => continue, // Skip if node is missing in SQLite (e.g. race condition/delete)
                };

                // Check text-level deduplication cache to completely bypass expensive neural inference
                let vector = match deduplication_cache.get(&job.text) {
                    Some(v) => {
                        log::debug!("⚡ EmbeddingQueue Deduplication Hit: Avoided neural inference for node: {}", job.id);
                        v.clone()
                    }
                    None => {
                        match embedding_model_clone.generate_embedding(&job.text).await {
                            Ok(v) => {
                                deduplication_cache.insert(job.text.clone(), v.clone());
                                v
                            }
                            Err(e) => {
                                log::warn!("Failed to generate embedding for {}: {}. Marking as FAILED.", job.id, e);
                                let _ = graph_clone.update_embedding_status(&job.id, "FAILED", None).await;
                                continue;
                            }
                        }
                    }
                };

                let id_array = StringArray::from(vec![job.id.clone()]);
                let text_array = StringArray::from(vec![job.text.clone()]);
                let decay_array = Float32Array::from(vec![1.0]);
                let access_array = UInt32Array::from(vec![node_info.access_count]);
                let node_type_array = StringArray::from(vec![node_info.node_type]);
                let vector_data = Float32Array::from(vector);
                
                let vector_array = match FixedSizeListArray::try_new(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    384,
                    Arc::new(vector_data),
                    None,
                ) {
                    Ok(arr) => arr,
                    Err(e) => {
                        log::error!("Failed to construct FixedSizeListArray for {}: {}", job.id, e);
                        continue;
                    }
                };

                let batch = match RecordBatch::try_new(
                    schema.clone(),
                    vec![
                        Arc::new(id_array) as ArrayRef,
                        Arc::new(text_array) as ArrayRef,
                        Arc::new(decay_array) as ArrayRef,
                        Arc::new(access_array) as ArrayRef,
                        Arc::new(node_type_array) as ArrayRef,
                        Arc::new(vector_array) as ArrayRef,
                    ],
                ) {
                    Ok(b) => b,
                    Err(e) => {
                        log::error!("Failed to build RecordBatch for {}: {}", job.id, e);
                        continue;
                    }
                };

                let table_opt = {
                    let lock = conn_clone.lock().await;
                    lock.open_table(&table_name_clone).execute().await
                };

                let table = match table_opt {
                    Ok(t) => t,
                    Err(e) => {
                        log::error!("Failed to open table for embedding write of node {}: {}", job.id, e);
                        continue;
                    }
                };

                if let Err(e) = table.add(vec![batch]).execute().await {
                    log::error!("Failed to insert batch into LanceDB for node {}: {}", job.id, e);
                    continue;
                }

                // sqlite status complete
                let _ = graph_clone.update_embedding_status(&job.id, "COMPLETED", Some(&job.id)).await;
            }
        });

        Ok(Self {
            embedding_model,
            conn,
            table_name,
            graph,
            tx,
            local_nodes: Arc::new(Mutex::new(Vec::new())),
            local_edges: Arc::new(Mutex::new(Vec::new())),
            base_node_ids: Arc::new(Vec::new()),
            is_isolated: false,
        })
    }

    /// Create an isolated clone of the KnowledgeNexus that buffers writes locally.
    pub fn isolated(&self) -> Self {
        Self {
            embedding_model: self.embedding_model.clone(),
            conn: self.conn.clone(),
            table_name: self.table_name.clone(),
            graph: self.graph.clone(),
            tx: self.tx.clone(),
            local_nodes: Arc::new(Mutex::new(Vec::new())),
            local_edges: Arc::new(Mutex::new(Vec::new())),
            base_node_ids: self.base_node_ids.clone(), // Simplified base state
            is_isolated: true,
        }
    }

    /// Commit local changes to the global KnowledgeNexus with conflict detection.
    pub async fn commit(&self) -> anyhow::Result<()> {
        if !self.is_isolated {
            return Ok(());
        }

        let nodes = {
            let mut guard = self.local_nodes.lock().await;
            std::mem::take(&mut *guard)
        };
        let edges = {
            let mut guard = self.local_edges.lock().await;
            std::mem::take(&mut *guard)
        };

        for node in nodes {
            // Check for conflict: Did someone else modify this node since we branched?
            if let Ok(Some(_existing)) = self.graph.get_node(&node.id).await {
                // If the node already exists and we didn't start with it, or it changed
                // For now: prioritize local but log conflict
                log::info!(
                    "Conflict detected for node {}. Applying local version.",
                    node.id
                );
            }
            let id = node.id.clone();
            let text = node.content.clone();
            self.graph.add_node(node).await?;

            // Queue for async background embedding
            let _ = self.tx.send(EmbeddingJob { id, text }).await;
        }
        for edge in edges {
            self.graph.add_edge(edge).await?;
        }

        Ok(())
    }

    pub async fn remember_batch(&self, entries: Vec<(String, String)>) -> anyhow::Result<()> {
        let mut new_nodes = Vec::new();
        for (id, text) in entries {
            let node = crate::graph::Node {
                id,
                label: text.chars().take(50).collect(),
                node_type: "generic".to_string(),
                content: text,
                summary: None,
                embedding_id: None,
                embedding_status: "PENDING".to_string(),
                access_count: 0,
                last_access_time: chrono::Utc::now().timestamp(),
                decay_score: 1.0,
                properties: serde_json::json!({}),
            };
            new_nodes.push(node);
        }

        if self.is_isolated {
            let mut guard = self.local_nodes.lock().await;
            guard.extend(new_nodes);
        } else {
            for node in new_nodes {
                let id = node.id.clone();
                let text = node.content.clone();
                self.graph.add_node(node).await?;

                // Queue for async background embedding
                let _ = self.tx.send(EmbeddingJob { id, text }).await;
            }
        }

        Ok(())
    }

    pub async fn add_edge(&self, edge: crate::graph::Edge) -> anyhow::Result<()> {
        if self.is_isolated {
            let mut guard = self.local_edges.lock().await;
            guard.push(edge);
        } else {
            self.graph.add_edge(edge).await?;
        }
        Ok(())
    }

    /// Record a causal edge between two knowledge nodes.
    ///
    /// Causal edges track why things happened:
    /// - `CAUSED_BY`: error was caused by a code pattern (RLFC failure)
    /// - `FIXED_BY`: error was fixed by a specific change (RLFC success)
    /// - `INVALIDATED_BY`: knowledge was superseded by newer information
    ///
    /// Weight indicates confidence (0.0–1.0). Default is 1.0 for verified causal links.
    pub async fn record_causal_edge(
        &self,
        from_id: &str,
        to_id: &str,
        relation: &str,
        weight: f32,
    ) -> anyhow::Result<()> {
        let edge = crate::graph::Edge {
            from_id: from_id.to_string(),
            to_id: to_id.to_string(),
            relation: relation.to_string(),
            weight: weight.clamp(0.0, 1.0),
            metadata: serde_json::json!({ "timestamp": chrono::Utc::now().to_rfc3339() }),
        };
        self.add_edge(edge).await
    }

    pub async fn sync_embeddings(&self) -> anyhow::Result<()> {
        let pending = self.graph.get_pending_embeddings().await?;
        for (id, text) in pending {
            let _ = self.tx.send(EmbeddingJob { id, text }).await;
        }
        Ok(())
    }

    pub async fn smart_search(&self, query: &str, limit: usize) -> anyhow::Result<Vec<String>> {
        let vector = self
            .embedding_model
            .generate_embedding(query)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to generate embedding: {}", e))?;

        let table = self.conn.lock().await.open_table(&self.table_name).execute().await?;
        // Request more than limit to allow for re-ranking
        let mut results = table
            .vector_search(vector)?
            .limit(limit * 2)
            .execute()
            .await?;

        let mut candidates = Vec::new();

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
            let distance_col = batch
                .column(batch.num_columns() - 1)
                .as_any()
                .downcast_ref::<Float32Array>()
                .unwrap();

            for i in 0..id_col.len() {
                let id = id_col.value(i).to_string();
                let text = text_col.value(i).to_string();
                let distance = distance_col.value(i);
                candidates.push((id, text, distance));
            }
        }

        let mut ranked_results = Vec::new();
        let now = chrono::Utc::now().timestamp();
        let query_lower = query.to_lowercase();
        let query_tokens: std::collections::HashSet<_> = query_lower.split_whitespace().collect();

        for (id, text, distance) in candidates {
            if let Ok(Some(node)) = self.graph.get_node(&id).await {
                // 1. Relevance (Hybrid: Vector Similarity + Keyword Boost)
                let vector_sim = 1.0 / (1.0 + distance);

                // Lightweight Keyword Score (approx BM25 without full index)
                let text_lower = text.to_lowercase();
                let match_count = query_tokens
                    .iter()
                    .filter(|&&t| text_lower.contains(t))
                    .count();
                let keyword_score =
                    (match_count as f32 / query_tokens.len().max(1) as f32).min(1.0);

                // Hybrid mix (weighted)
                let relevance = (vector_sim * 0.7) + (keyword_score * 0.3);

                // 2. Freshness (Smart Decay)
                // score = e^(-λt) * log(1 + access_count)
                let t_delta = (now - node.last_access_time).max(0) as f32 / 86400.0; // in days
                let lambda = match node.node_type.as_str() {
                    "code_struct" | "code_trait" => 0.01, // slow decay for core structures
                    _ => 0.05,                            // standard decay
                };
                let freshness = (-lambda * t_delta).exp() * (1.0 + (node.access_count as f32).ln());

                // 3. Structural Boost (based on graph edges)
                let relations = self.graph.query_relations(&id).await.unwrap_or_default();
                let structural_boost = 1.0 + (relations.len() as f32 * 0.1).min(1.0);

                let final_score = relevance * freshness * structural_boost;

                ranked_results.push((node, final_score));
            }
        }

        // Sort by final_score descending
        ranked_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut documents = Vec::new();
        for (node, _) in ranked_results.into_iter().take(limit) {
            let _ = self.graph.record_access(&node.id).await;
            documents.push(node.content.clone());

            // Graph Expansion (Delayed/Tiered)
            if let Ok(relations) = self.graph.query_relations(&node.id).await {
                for (rel_node, edge) in relations.into_iter().take(3) {
                    if edge.weight > 0.9 {
                        documents.push(format!(
                            "[Related: {}] Full Content:\n{}",
                            edge.relation, rel_node.content
                        ));
                    } else if edge.weight > 0.5 {
                        let summary = rel_node.summary.clone().unwrap_or_else(|| {
                            let preview = rel_node
                                .content
                                .chars()
                                .take(120)
                                .collect::<String>()
                                .replace('\n', " ");
                            format!("{}...", preview)
                        });
                        documents
                            .push(format!("[Related: {}] Summary: {}", edge.relation, summary));
                    }
                }
            }
        }

        Ok(documents)
    }

    pub async fn decay_memories(&self, factor: f32) -> anyhow::Result<()> {
        let node_ids = self.graph.get_all_node_ids().await?;
        let table = self.conn.lock().await.open_table(&self.table_name).execute().await?;

        for id in node_ids {
            if let Some(node) = self.graph.get_node(&id).await? {
                // Decay suppression for high-access nodes
                let suppression = (node.access_count as f32 / 100.0).min(0.95);
                let bounded_factor = factor.clamp(0.90, 1.0);
                let actual_factor = 1.0 - (1.0 - bounded_factor) * (1.0 - suppression);
                
                let new_score = (node.decay_score * actual_factor).max(0.01);
                
                // Update SQLite
                self.graph.update_decay_score(&id, new_score).await?;
                
                // Update LanceDB
                table
                    .update()
                    .only_if(format!("id = '{}'", id))
                    .column("decay_score", new_score.to_string())
                    .execute()
                    .await?;
            }
        }

        Ok(())
    }
}