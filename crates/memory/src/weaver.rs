use arrow::array::{
    Array, ArrayRef, FixedSizeListArray, Float32Array, RecordBatch, StringArray, UInt32Array,
};
use arrow::datatypes::{DataType, Field, Schema};
use async_trait::async_trait;
use fastembed::{InitOptions, TextEmbedding};
use futures::StreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{Connection, connect};
use lancedb::table::OptimizeAction;
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

/// A topic cluster — group of semantically related knowledge nodes.
#[derive(Debug, Clone)]
pub struct TopicCluster {
    pub id: usize,
    pub centroid: Vec<f32>,
    pub member_ids: Vec<String>,
    pub access_count: u64,
    pub last_accessed: i64,
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
    /// Topic clusters for cross-session knowledge sharing (from objeta L3 cache pattern).
    topic_clusters: Arc<Mutex<Vec<TopicCluster>>>,
    /// Node ID → cluster index mapping.
    cluster_map: Arc<Mutex<HashMap<String, usize>>>,
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
            topic_clusters: Arc::new(Mutex::new(Vec::new())),
            cluster_map: Arc::new(Mutex::new(HashMap::new())),
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
            base_node_ids: self.base_node_ids.clone(),
            is_isolated: true,
            topic_clusters: self.topic_clusters.clone(),
            cluster_map: self.cluster_map.clone(),
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
        if self.is_isolated {
            let mut documents = Vec::new();
            let query_lower = query.to_lowercase();
            let query_tokens: std::collections::HashSet<_> = query_lower.split_whitespace().collect();
            
            let nodes = self.local_nodes.lock().await.clone();
            let mut ranked = Vec::new();
            
            // Try to generate a query embedding for cosine approximation if needed
            let query_vector = self.embedding_model.generate_embedding(query).await.ok();
            
            for node in nodes {
                let text_lower = node.content.to_lowercase();
                
                // Keyword match score (weight 0.3)
                let match_count = query_tokens.iter().filter(|&&t| text_lower.contains(t)).count();
                let keyword_score = match_count as f32 / query_tokens.len().max(1) as f32;
                
                // Cosine similarity approximation (weight 0.7)
                let mut similarity = 0.0f32;
                if let Some(ref q_vec) = query_vector {
                    if let Ok(node_vec) = self.embedding_model.generate_embedding(&node.content).await {
                        let dot: f32 = q_vec.iter().zip(node_vec.iter()).map(|(a, b)| a * b).sum();
                        similarity = dot;
                    }
                }
                
                let score = (similarity * 0.7) + (keyword_score * 0.3);
                let relevance_threshold = std::env::var("PHARMAKON_RELEVANCE_THRESHOLD")
                    .ok()
                    .and_then(|v| v.parse::<f32>().ok())
                    .unwrap_or(0.20);
                if score >= relevance_threshold {
                    ranked.push((node, score));
                }
            }
            
            ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            for (node, _) in ranked.into_iter().take(limit) {
                documents.push(node.content);
            }
            return Ok(documents);
        }

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

                let relevance_threshold = std::env::var("PHARMAKON_RELEVANCE_THRESHOLD")
                    .ok()
                    .and_then(|v| v.parse::<f32>().ok())
                    .unwrap_or(0.20);
                if relevance >= relevance_threshold {
                    ranked_results.push((node, final_score));
                }
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

    /// Search with topic cluster boost for cross-session knowledge sharing.
    ///
    /// After the standard smart_search scoring, applies a cluster affinity boost:
    /// nodes in the same topic cluster as the top result get a +15% score boost.
    /// This enables cross-session knowledge discovery (from objeta L3 cache pattern).
    pub async fn search_with_topic_boost(&self, query: &str, limit: usize) -> anyhow::Result<Vec<String>> {
        let results = self.smart_search(query, limit * 2).await?;
        if results.len() <= 1 {
            return Ok(results.into_iter().take(limit).collect());
        }

        // Check if topic clusters are available
        let clusters = self.topic_clusters.lock().await;
        if clusters.is_empty() {
            return Ok(results.into_iter().take(limit).collect());
        }

        // Try to find which cluster the top result belongs to
        let top_result = &results[0];
        let mut boost_cluster_id: Option<usize> = None;

        for cluster in clusters.iter() {
            for member_id in &cluster.member_ids {
                if let Ok(Some(node)) = self.graph.get_node(member_id).await {
                    if top_result.contains(&node.content)
                        || node.content.contains(top_result.as_str())
                    {
                        boost_cluster_id = Some(cluster.id);
                        break;
                    }
                }
            }
            if boost_cluster_id.is_some() {
                break;
            }
        }

        // If a cluster was found, boost other results from the same cluster
        if let Some(cluster_id) = boost_cluster_id {
            let mut boosted: Vec<(String, f32)> = Vec::with_capacity(results.len());
            // Re-acquire lock for scoring pass
            let clusters = self.topic_clusters.lock().await;

            for (i, text) in results.into_iter().enumerate() {
                let score = if i == 0 {
                    2.0
                } else {
                    let in_cluster = clusters.iter()
                        .filter(|c| c.id == cluster_id)
                        .any(|c| {
                            c.member_ids.iter().any(|mid| {
                                text.contains(mid) // simple id-based match; content overlap is heavier but async
                            })
                        });
                    if in_cluster { 1.15 } else { 1.0 }
                };
                boosted.push((text, score));
            }

            boosted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            return Ok(boosted.into_iter().take(limit).map(|(t, _)| t).collect());
        }

        Ok(results.into_iter().take(limit).collect())
    }

    /// Build topic clusters from stored embeddings using simplified centroid-based clustering.
    ///
    /// Reads all nodes with completed embeddings from the graph store,
    /// fetches their vectors from LanceDB, and groups them into k clusters.
    /// Clusters are used by `search_with_topic_boost` for cross-session knowledge sharing.
    pub async fn build_topic_clusters(&self, k: usize) -> anyhow::Result<()> {
        if self.is_isolated {
            return Ok(());
        }

        let all_ids = self.graph.get_all_node_ids().await?;
        if all_ids.len() < k {
            return Ok(());
        }

        let table = self.conn.lock().await.open_table(&self.table_name).execute().await?;
        let mut node_vectors: Vec<(String, Vec<f32>)> = Vec::new();

        // Fetch all vectors from LanceDB via full table scan
        if let Ok(mut stream) = table.query().execute().await {
            while let Some(Ok(batch)) = stream.next().await {
                let id_col = batch.column(0).as_any().downcast_ref::<StringArray>().unwrap();
                let vector_col = batch.column_by_name("vector");
                if let Some(arr) = vector_col {
                    if let Some(list_arr) = arr.as_any().downcast_ref::<FixedSizeListArray>() {
                        for row in 0..list_arr.len() {
                            let id = id_col.value(row).to_string();
                            if let Ok(Some(node)) = self.graph.get_node(&id).await {
                                if node.embedding_status == "COMPLETED" {
                                    if let Some(float_arr) = list_arr.value(row).as_any().downcast_ref::<Float32Array>() {
                                        let vec: Vec<f32> = (0..float_arr.len()).map(|i| float_arr.value(i)).collect();
                                        node_vectors.push((id, vec));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if node_vectors.len() < k {
            return Ok(());
        }

        // Simple centroid initialization: spread k centroids across the dataset
        let mut clusters: Vec<TopicCluster> = Vec::new();
        let n = node_vectors.len();
        let actual_k = k.min(n);

        for i in 0..actual_k {
            let idx = (i * n / actual_k) % n; // deterministic spread
            clusters.push(TopicCluster {
                id: i,
                centroid: node_vectors[idx].1.clone(),
                member_ids: Vec::new(),
                access_count: 0,
                last_accessed: 0,
            });
        }

        // Single-pass assignment: assign each node to nearest centroid
        for (node_id, vector) in &node_vectors {
            let mut best_cluster = 0;
            let mut best_sim = f32::NEG_INFINITY;

            for (ci, cluster) in clusters.iter().enumerate() {
                let dot: f32 = vector.iter().zip(cluster.centroid.iter()).map(|(a, b)| a * b).sum();
                if dot > best_sim {
                    best_sim = dot;
                    best_cluster = ci;
                }
            }

            clusters[best_cluster].member_ids.push(node_id.clone());
        }

        // Update centroids as mean of members
        for cluster in &mut clusters {
            if cluster.member_ids.is_empty() {
                continue;
            }
            let dim = cluster.centroid.len();
            let mut new_centroid = vec![0.0f32; dim];
            let mut count = 0;

            for member_id in &cluster.member_ids {
                if let Some((_, vec)) = node_vectors.iter().find(|(id, _)| id == member_id) {
                    for (i, v) in vec.iter().enumerate() {
                        new_centroid[i] += v;
                    }
                    count += 1;
                }
            }

            if count > 0 {
                for v in new_centroid.iter_mut() {
                    *v /= count as f32;
                }
                cluster.centroid = new_centroid;
            }
        }

        // Update cluster map
        let mut cmap = HashMap::new();
        for cluster in &clusters {
            for mid in &cluster.member_ids {
                cmap.insert(mid.clone(), cluster.id);
            }
        }

        *self.topic_clusters.lock().await = clusters;
        *self.cluster_map.lock().await = cmap;

        log::info!(
            "KnowledgeNexus: Built {} topic clusters covering {} nodes.",
            actual_k,
            node_vectors.len()
        );
        Ok(())
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

    pub async fn delete_by_session(&self, session_id: &str) -> anyhow::Result<()> {
        // 1. Clear isolated delta buffers if active
        {
            let mut nodes = self.local_nodes.lock().await;
            let mut edges = self.local_edges.lock().await;
            
            let pattern = format!("\"session_id\":\"{}\"", session_id);
            nodes.retain(|n| {
                let props_str = serde_json::to_string(&n.properties).unwrap_or_default();
                !props_str.contains(&pattern)
            });
            let remaining_ids: std::collections::HashSet<String> = nodes.iter().map(|n| n.id.clone()).collect();
            edges.retain(|e| remaining_ids.contains(&e.from_id) && remaining_ids.contains(&e.to_id));
        }

        // 2. Identify the node IDs to delete from LanceDB BEFORE we delete them from SQLite
        let node_ids_to_delete = self.graph.get_session_node_ids(session_id).await?;

        // 3. Delete from LanceDB & Compact table
        if !node_ids_to_delete.is_empty() {
            let table = self.conn.lock().await.open_table(&self.table_name).execute().await?;
            for id in node_ids_to_delete {
                if let Err(e) = table.delete(&format!("id = '{}'", id)).await {
                    log::warn!("Failed to delete node '{}' from LanceDB: {}", id, e);
                }
            }
            // Trigger explicit compaction/optimization to reclaim disk space
            if let Err(e) = table.optimize(OptimizeAction::All).await {
                log::warn!("LanceDB: Failed to optimize table after deletions: {}", e);
            }
        }

        // 4. Delete from SQLite GraphStore
        self.graph.delete_by_session(session_id).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Node;

    #[tokio::test]
    async fn test_weaver_isolated_and_delete() -> anyhow::Result<()> {
        let r_id = uuid::Uuid::new_v4().to_string();
        let db_path = format!("target/test_lancedb_{}", r_id);
        let graph_db_path = format!("target/test_graph_{}.db", r_id);

        let nexus = KnowledgeNexus::new(
            &db_path,
            &graph_db_path,
        ).await?;

        // Test delete_by_session with isolated/delta nodes
        let isolated = nexus.isolated();
        
        let props = serde_json::json!({
            "session_id": "test-session-123"
        });

        let node = Node {
            id: "node-1".to_string(),
            label: "test-node".to_string(),
            node_type: "generic".to_string(),
            content: "Rust is a systems programming language".to_string(),
            summary: None,
            embedding_id: None,
            embedding_status: "PENDING".to_string(),
            access_count: 0,
            last_access_time: 0,
            decay_score: 1.0,
            properties: props,
        };

        {
            let mut nodes: tokio::sync::MutexGuard<'_, Vec<Node>> = isolated.local_nodes.lock().await;
            nodes.push(node);
        }

        // Verify it was added to local nodes
        {
            let nodes: tokio::sync::MutexGuard<'_, Vec<Node>> = isolated.local_nodes.lock().await;
            assert_eq!(nodes.len(), 1);
        }

        // Delete by session
        isolated.delete_by_session("test-session-123").await?;

        // Verify local nodes is empty
        {
            let nodes: tokio::sync::MutexGuard<'_, Vec<Node>> = isolated.local_nodes.lock().await;
            assert_eq!(nodes.len(), 0);
        }

        // Clean up temp files
        let _ = std::fs::remove_dir_all(&db_path);
        let _ = std::fs::remove_file(&graph_db_path);

        Ok(())
    }
}