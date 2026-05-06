use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub properties: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Edge {
    pub from_id: String,
    pub to_id: String,
    pub relation: String,
}

pub struct GraphStore {
    pool: SqlitePool,
}

impl GraphStore {
    pub async fn new(db_path: &str) -> Result<Self> {
        let pool = SqlitePool::connect(&format!("sqlite:{}", db_path)).await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS graph_nodes (
                id TEXT PRIMARY KEY,
                label TEXT NOT NULL,
                properties TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS graph_edges (
                from_id TEXT NOT NULL,
                to_id TEXT NOT NULL,
                relation TEXT NOT NULL,
                PRIMARY KEY (from_id, to_id, relation),
                FOREIGN KEY (from_id) REFERENCES graph_nodes(id),
                FOREIGN KEY (to_id) REFERENCES graph_nodes(id)
            )",
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool })
    }

    pub async fn add_node(&self, node: Node) -> Result<()> {
        let props = serde_json::to_string(&node.properties)?;
        sqlx::query("INSERT OR REPLACE INTO graph_nodes (id, label, properties) VALUES (?, ?, ?)")
            .bind(node.id)
            .bind(node.label)
            .bind(props)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn add_edge(&self, edge: Edge) -> Result<()> {
        sqlx::query(
            "INSERT OR IGNORE INTO graph_edges (from_id, to_id, relation) VALUES (?, ?, ?)",
        )
        .bind(edge.from_id)
        .bind(edge.to_id)
        .bind(edge.relation)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn query_relations(&self, node_id: &str) -> Result<Vec<String>> {
        let rows = sqlx::query(
            "SELECT n.label, e.relation FROM graph_nodes n 
             JOIN graph_edges e ON n.id = e.to_id 
             WHERE e.from_id = ?",
        )
        .bind(node_id)
        .fetch_all(&self.pool)
        .await?;

        let mut results = Vec::new();
        for row in rows {
            let label: String = row.get(0);
            let relation: String = row.get(1);
            results.push(format!("{} is linked via {}", label, relation));
        }
        Ok(results)
    }
}
