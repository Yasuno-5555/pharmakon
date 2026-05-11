use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub node_type: String, // e.g., "code_struct", "code_fn", "research_summary"
    pub content: String,   // The raw text to be embedded
    pub summary: Option<String>,
    pub embedding_id: Option<String>,
    pub embedding_status: String, // PENDING, COMPLETED, FAILED
    pub access_count: u32,
    pub last_access_time: i64,
    pub decay_score: f32,
    pub properties: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Edge {
    pub from_id: String,
    pub to_id: String,
    pub relation: String,
    pub weight: f32,
    pub metadata: serde_json::Value,
}

/// Well-known causal edge relation types for KnowledgeNexus.
/// These enable causal memory chains for debugging and reasoning.
impl Edge {
    pub const CAUSED_BY: &'static str = "caused_by";
    pub const FIXED_BY: &'static str = "fixed_by";
    pub const INVALIDATED_BY: &'static str = "invalidated_by";
}

pub struct GraphStore {
    pool: SqlitePool,
}

impl GraphStore {
    pub async fn new(db_path: &str) -> Result<Self> {
        let pool = SqlitePool::connect(&format!("sqlite:{}?mode=rwc", db_path)).await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS graph_nodes (
                id TEXT PRIMARY KEY,
                label TEXT NOT NULL,
                node_type TEXT NOT NULL DEFAULT 'generic',
                content TEXT NOT NULL,
                summary TEXT,
                embedding_id TEXT,
                embedding_status TEXT DEFAULT 'PENDING',
                access_count INTEGER DEFAULT 0,
                last_access_time INTEGER DEFAULT 0,
                decay_score REAL DEFAULT 1.0,
                properties TEXT NOT NULL DEFAULT '{}'
            )",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS graph_edges (
                from_id TEXT NOT NULL,
                to_id TEXT NOT NULL,
                relation TEXT NOT NULL,
                weight REAL DEFAULT 1.0,
                metadata TEXT NOT NULL DEFAULT '{}',
                PRIMARY KEY (from_id, to_id, relation),
                FOREIGN KEY (from_id) REFERENCES graph_nodes(id),
                FOREIGN KEY (to_id) REFERENCES graph_nodes(id)
            )",
        )
        .execute(&pool)
        .await?;

        // Migration: add any columns that might be missing from old databases.
        // ALTER TABLE ADD COLUMN errors if column already exists, so we ignore errors.
        let migrations = [
            "ALTER TABLE graph_nodes ADD COLUMN node_type TEXT NOT NULL DEFAULT 'generic'",
            "ALTER TABLE graph_nodes ADD COLUMN content TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE graph_nodes ADD COLUMN summary TEXT",
            "ALTER TABLE graph_nodes ADD COLUMN embedding_id TEXT",
            "ALTER TABLE graph_nodes ADD COLUMN embedding_status TEXT DEFAULT 'PENDING'",
            "ALTER TABLE graph_nodes ADD COLUMN access_count INTEGER DEFAULT 0",
            "ALTER TABLE graph_nodes ADD COLUMN last_access_time INTEGER DEFAULT 0",
            "ALTER TABLE graph_nodes ADD COLUMN decay_score REAL DEFAULT 1.0",
            "ALTER TABLE graph_nodes ADD COLUMN properties TEXT NOT NULL DEFAULT '{}'",
            "ALTER TABLE graph_edges ADD COLUMN metadata TEXT NOT NULL DEFAULT '{}'",
        ];
        for sql in migrations {
            let _ = sqlx::query(sql).execute(&pool).await;
        }

        Ok(Self { pool })
    }

    pub async fn add_node(&self, node: Node) -> Result<()> {
        let props = serde_json::to_string(&node.properties)?;
        sqlx::query("INSERT OR REPLACE INTO graph_nodes (id, label, node_type, content, summary, embedding_id, embedding_status, access_count, last_access_time, decay_score, properties) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(node.id)
            .bind(node.label)
            .bind(node.node_type)
            .bind(node.content)
            .bind(node.summary)
            .bind(node.embedding_id)
            .bind(node.embedding_status)
            .bind(node.access_count)
            .bind(node.last_access_time)
            .bind(node.decay_score)
            .bind(props)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn add_edge(&self, edge: Edge) -> Result<()> {
        let metadata = serde_json::to_string(&edge.metadata)?;
        sqlx::query(
            "INSERT OR REPLACE INTO graph_edges (from_id, to_id, relation, weight, metadata) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(edge.from_id)
        .bind(edge.to_id)
        .bind(edge.relation)
        .bind(edge.weight)
        .bind(metadata)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_pending_embeddings(&self) -> Result<Vec<(String, String)>> {
        let rows =
            sqlx::query("SELECT id, content FROM graph_nodes WHERE embedding_status = 'PENDING'")
                .fetch_all(&self.pool)
                .await?;

        Ok(rows.into_iter().map(|r| (r.get(0), r.get(1))).collect())
    }

    pub async fn update_embedding_status(
        &self,
        id: &str,
        status: &str,
        embedding_id: Option<&str>,
    ) -> Result<()> {
        sqlx::query("UPDATE graph_nodes SET embedding_status = ?, embedding_id = ? WHERE id = ?")
            .bind(status)
            .bind(embedding_id)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_node(&self, id: &str) -> Result<Option<Node>> {
        let row = sqlx::query("SELECT * FROM graph_nodes WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        if let Some(r) = row {
            Ok(Some(Node {
                id: r.get("id"),
                label: r.get("label"),
                node_type: r.get("node_type"),
                content: r.get("content"),
                summary: r.get("summary"),
                embedding_id: r.get("embedding_id"),
                embedding_status: r.get("embedding_status"),
                access_count: r.get::<i64, _>("access_count") as u32,
                last_access_time: r.get("last_access_time"),
                decay_score: r.get("decay_score"),
                properties: serde_json::from_str(r.get("properties"))?,
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn record_access(&self, id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE graph_nodes SET access_count = access_count + 1, last_access_time = ?, decay_score = 1.0 WHERE id = ?",
        )
        .bind(chrono::Utc::now().timestamp())
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn query_relations(&self, node_id: &str) -> Result<Vec<(Node, Edge)>> {
        let rows = sqlx::query(
            "SELECT n.*, e.* FROM graph_nodes n
             JOIN graph_edges e ON n.id = e.to_id
             WHERE e.from_id = ?",
        )
        .bind(node_id)
        .fetch_all(&self.pool)
        .await?;

        let mut results = Vec::new();
        for r in rows {
            let node = Node {
                id: r.get("id"),
                label: r.get("label"),
                node_type: r.get("node_type"),
                content: r.get("content"),
                summary: r.get("summary"),
                embedding_id: r.get("embedding_id"),
                embedding_status: r.get("embedding_status"),
                access_count: r.get::<i64, _>("access_count") as u32,
                last_access_time: r.get("last_access_time"),
                decay_score: r.get("decay_score"),
                properties: serde_json::from_str(r.get("properties"))?,
            };
            let edge = Edge {
                from_id: r.get("from_id"),
                to_id: r.get("to_id"),
                relation: r.get("relation"),
                weight: r.get("weight"),
                metadata: serde_json::from_str(r.get("metadata"))?,
            };
            results.push((node, edge));
        }
        Ok(results)
    }

    pub async fn get_all_node_ids(&self) -> Result<Vec<String>> {
        let rows = sqlx::query("SELECT id FROM graph_nodes")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(|r| r.get(0)).collect())
    }

    pub async fn update_decay_score(&self, id: &str, score: f32) -> Result<()> {
        sqlx::query("UPDATE graph_nodes SET decay_score = ? WHERE id = ?")
            .bind(score)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}