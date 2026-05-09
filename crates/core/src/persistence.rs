use crate::model::Message;
use crate::trajectory::Trajectory;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::Row;
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use std::str::FromStr;

pub struct DbSessionStore {
    pool: SqlitePool,
}

impl DbSessionStore {
    pub async fn new(database_url: &str) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(database_url)?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal);

        let pool = SqlitePool::connect_with(options).await?;

        // Initialize schema
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT,
                tool_calls TEXT,
                tool_call_id TEXT,
                name TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS usage_stats (
                id INTEGER PRIMARY KEY,
                session_id TEXT,
                provider TEXT,
                model TEXT,
                prompt_tokens INTEGER,
                completion_tokens INTEGER,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS traffic_capture (
                id INTEGER PRIMARY KEY,
                session_id TEXT,
                url TEXT,
                method TEXT,
                status INTEGER,
                request_body TEXT,
                response_body TEXT,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS delivery_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                payload TEXT NOT NULL,
                retry_count INTEGER DEFAULT 0,
                next_retry TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                status TEXT DEFAULT 'pending'
            )",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS trajectory_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                payload TEXT NOT NULL,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS trajectories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                steps_json TEXT NOT NULL,
                model TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS commitments (
                id TEXT PRIMARY KEY,
                description TEXT NOT NULL,
                deadline DATETIME,
                status TEXT NOT NULL,
                metadata TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS approved_users (
                channel_id TEXT PRIMARY KEY,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS agent_souls (
                name TEXT PRIMARY KEY,
                description TEXT,
                instruction TEXT NOT NULL,
                avatar_url TEXT,
                metadata TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS facts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                content TEXT NOT NULL,
                source TEXT,
                importance INTEGER DEFAULT 1,
                metadata TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS research_cache (
                url TEXT PRIMARY KEY,
                summary TEXT NOT NULL,
                depth TEXT NOT NULL,
                metadata TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS tool_metrics (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                tool_name TEXT NOT NULL,
                success BOOLEAN NOT NULL,
                latency_ms INTEGER NOT NULL,
                error TEXT,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&pool)
        .await?;

        // Migration: add name column if missing (pre-existing DBs)
        let _ = sqlx::query(
            "ALTER TABLE messages ADD COLUMN name TEXT",
        )
        .execute(&pool)
        .await;

        Ok(Self { pool })
    }

    pub async fn save_message(&self, session_id: &str, msg: &Message) -> Result<()> {
        let content_json = msg
            .content
            .as_ref()
            .map(|c| serde_json::to_string(c).unwrap());
        let tool_calls_json = msg
            .tool_calls
            .as_ref()
            .map(|tc| serde_json::to_string(tc).unwrap());

        sqlx::query(
            "INSERT INTO messages (session_id, role, content, tool_calls, tool_call_id, name)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(session_id)
        .bind(&msg.role)
        .bind(content_json)
        .bind(tool_calls_json)
        .bind(&msg.tool_call_id)
        .bind(&msg.name)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn load_history(&self, session_id: &str) -> Result<Vec<Message>> {
        let rows = sqlx::query_as::<_, MessageRow>(
            "SELECT role, content, tool_calls, tool_call_id, name FROM messages
             WHERE session_id = ? ORDER BY created_at ASC",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        let messages = rows
            .into_iter()
            .map(|row| {
                let content = row.content.map(|c| {
                    // Try to parse as JSON first, if it fails, it might be a raw quoted string from sqlite3 output or old data
                    if let Ok(parsed) = serde_json::from_str::<crate::model::MessageContent>(&c) {
                        parsed
                    } else if let Ok(s) = serde_json::from_str::<String>(&c) {
                        crate::model::MessageContent::Text(s)
                    } else {
                        crate::model::MessageContent::Text(c)
                    }
                });
                Message {
                    role: row.role,
                    content,
                    tool_calls: row.tool_calls.and_then(|tc| serde_json::from_str(&tc).ok()),
                    tool_call_id: row.tool_call_id,
                    name: row.name,
                    ..Default::default()
                }
            })
            .collect();

        Ok(messages)
    }

    /// Clean up orphaned one-shot sessions (≤2 messages, older than 1 hour).
    pub async fn cleanup_orphan_sessions(&self) -> Result<usize> {
        let result = sqlx::query(
            "DELETE FROM messages WHERE session_id IN (
                SELECT session_id FROM messages
                GROUP BY session_id
                HAVING COUNT(*) <= 2
                   AND MAX(created_at) < datetime('now', '-1 hour')
            )",
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() as usize)
    }

    pub async fn list_sessions(&self) -> Result<Vec<String>> {
        let rows = sqlx::query_as::<_, (String,)>(
            "SELECT DISTINCT session_id FROM messages ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    pub async fn search_sessions(&self, query: &str) -> Result<Vec<String>> {
        let sql_query = format!("%{}%", query);
        let rows = sqlx::query_as::<_, (String,)>(
            "SELECT DISTINCT session_id FROM messages
             WHERE session_id LIKE ? OR content LIKE ?
             ORDER BY created_at DESC",
        )
        .bind(&sql_query)
        .bind(&sql_query)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    pub async fn enqueue_delivery(&self, session_id: &str, payload: &str) -> Result<()> {
        sqlx::query("INSERT INTO delivery_queue (session_id, payload) VALUES (?, ?)")
            .bind(session_id)
            .bind(payload)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_pending_deliveries(&self) -> Result<Vec<(i64, String, String)>> {
        let rows = sqlx::query_as::<_, (i64, String, String)>(
            "SELECT id, session_id, payload FROM delivery_queue WHERE status = 'pending' AND next_retry <= CURRENT_TIMESTAMP"
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn mark_delivered(&self, id: i64) -> Result<()> {
        sqlx::query("DELETE FROM delivery_queue WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn log_usage(
        &self,
        session_id: &str,
        provider: &str,
        model: &str,
        prompt: u32,
        completion: u32,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO usage_stats (session_id, provider, model, prompt_tokens, completion_tokens) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(session_id)
        .bind(provider)
        .bind(model)
        .bind(prompt)
        .bind(completion)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn log_traffic(
        &self,
        session_id: &str,
        url: &str,
        method: &str,
        status: u16,
        req: &str,
        res: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO traffic_capture (session_id, url, method, status, request_body, response_body) VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(session_id)
        .bind(url)
        .bind(method)
        .bind(status as i32)
        .bind(req)
        .bind(res)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn save_commitment(
        &self,
        id: &str,
        description: &str,
        deadline: Option<DateTime<Utc>>,
        status: &str,
        metadata: &Value,
    ) -> Result<()> {
        let metadata_json = serde_json::to_string(metadata)?;
        sqlx::query(
            "INSERT OR REPLACE INTO commitments (id, description, deadline, status, metadata) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(id)
        .bind(description)
        .bind(deadline)
        .bind(status)
        .bind(metadata_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn load_commitments(&self) -> Result<Vec<Value>> {
        let rows = sqlx::query_as::<_, (String, String, Option<DateTime<Utc>>, String, String)>(
            "SELECT id, description, deadline, status, metadata FROM commitments ORDER BY created_at DESC"
        )
        .fetch_all(&self.pool)
        .await?;

        let commitments = rows
            .into_iter()
            .map(|(id, desc, deadline, status, meta)| {
                serde_json::json!({
                    "id": id,
                    "description": desc,
                    "deadline": deadline,
                    "status": status,
                    "metadata": serde_json::from_str::<Value>(&meta).unwrap_or_default()
                })
            })
            .collect();

        Ok(commitments)
    }

    pub async fn update_commitment_status(&self, id: &str, status: &str) -> Result<()> {
        sqlx::query("UPDATE commitments SET status = ? WHERE id = ?")
            .bind(status)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn approve_user(&self, channel_id: &str) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO approved_users (channel_id) VALUES (?)")
            .bind(channel_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn is_user_approved(&self, channel_id: &str) -> Result<bool> {
        let row = sqlx::query("SELECT 1 FROM approved_users WHERE channel_id = ?")
            .bind(channel_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.is_some())
    }

    pub async fn add_fact(
        &self,
        content: &str,
        source: Option<&str>,
        importance: i32,
        metadata: &Value,
    ) -> Result<()> {
        let metadata_json = serde_json::to_string(metadata)?;
        sqlx::query(
            "INSERT INTO facts (content, source, importance, metadata) VALUES (?, ?, ?, ?)",
        )
        .bind(content)
        .bind(source)
        .bind(importance)
        .bind(metadata_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn search_facts(&self, query: &str) -> Result<Vec<Value>> {
        // Simple keyword search for now, will be replaced by vector search
        let search_query = format!("%{}%", query);
        let rows = sqlx::query_as::<_, (String, Option<String>, i32, String)>(
            "SELECT content, source, importance, metadata FROM facts
             WHERE content LIKE ? OR source LIKE ? ORDER BY importance DESC, created_at DESC",
        )
        .bind(&search_query)
        .bind(&search_query)
        .fetch_all(&self.pool)
        .await?;

        let facts = rows
            .into_iter()
            .map(|(content, source, importance, meta)| {
                serde_json::json!({
                    "content": content,
                    "source": source,
                    "importance": importance,
                    "metadata": serde_json::from_str::<Value>(&meta).unwrap_or_default()
                })
            })
            .collect();

        Ok(facts)
    }
}

#[async_trait]
impl pharmakon_common::CommitmentPersistence for DbSessionStore {
    async fn save_commitment(
        &self,
        id: &str,
        description: &str,
        deadline: Option<DateTime<Utc>>,
        status: &str,
        metadata: &Value,
    ) -> Result<()> {
        self.save_commitment(id, description, deadline, status, metadata)
            .await
    }

    async fn load_commitments(&self) -> Result<Vec<Value>> {
        self.load_commitments().await
    }

    async fn update_commitment_status(&self, id: &str, status: &str) -> Result<()> {
        self.update_commitment_status(id, status).await
    }
}

use serde_json::Value;

#[derive(sqlx::FromRow)]
struct MessageRow {
    role: String,
    content: Option<String>,
    tool_calls: Option<String>,
    tool_call_id: Option<String>,
    name: Option<String>,
}

impl DbSessionStore {
    pub async fn save_trajectory_event(
        &self,
        session_id: &str,
        event_type: &str,
        payload: &serde_json::Value,
    ) -> Result<()> {
        let payload_json = serde_json::to_string(payload)?;
        sqlx::query(
            "INSERT INTO trajectory_events (session_id, event_type, payload) VALUES (?, ?, ?)",
        )
        .bind(session_id)
        .bind(event_type)
        .bind(payload_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn save_tool_metric(
        &self,
        tool_name: &str,
        success: bool,
        latency_ms: u64,
        error: Option<String>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO tool_metrics (tool_name, success, latency_ms, error) VALUES (?, ?, ?, ?)",
        )
        .bind(tool_name)
        .bind(success)
        .bind(latency_ms as i64)
        .bind(error)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_tool_metrics(&self) -> Result<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            "SELECT tool_name, 
                    COUNT(*) as calls,
                    SUM(CASE WHEN success THEN 1 ELSE 0 END) as successes,
                    AVG(latency_ms) as avg_latency
             FROM tool_metrics
             GROUP BY tool_name"
        )
        .fetch_all(&self.pool)
        .await?;

        let mut stats = Vec::new();
        for r in rows {
            stats.push(serde_json::json!({
                "tool": r.get::<String, _>("tool_name"),
                "calls": r.get::<i64, _>("calls"),
                "successes": r.get::<i64, _>("successes"),
                "avg_latency_ms": r.get::<f64, _>("avg_latency"),
            }));
        }
        Ok(stats)
    }


    pub async fn load_trajectory_events(
        &self,
        session_id: &str,
    ) -> Result<Vec<crate::trajectory::TrajectoryStep>> {
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT event_type, payload FROM trajectory_events
             WHERE session_id = ? ORDER BY timestamp ASC",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        let mut steps = Vec::new();
        for (_event_type, payload) in rows {
            // We need to reconstruct TrajectoryStep.
            // Since TrajectoryStep is an enum with #[serde(tag = "type")],
            // the payload must match that structure.
            let step: crate::trajectory::TrajectoryStep = serde_json::from_str(&payload)?;
            steps.push(step);
        }
        Ok(steps)
    }

    pub async fn save_trajectory(&self, trajectory: &Trajectory) -> Result<()> {
        let steps_json = serde_json::to_string(&trajectory.steps)?;
        sqlx::query("INSERT INTO trajectories (session_id, steps_json, model) VALUES (?, ?, ?)")
            .bind(&trajectory.session_id)
            .bind(&steps_json)
            .bind(&trajectory.metadata.model)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn load_trajectory(&self, session_id: &str) -> Result<Option<Trajectory>> {
        #[derive(sqlx::FromRow)]
        struct TrajectoryRow {
            steps_json: String,
            model: Option<String>,
            created_at: Option<DateTime<Utc>>,
        }

        let row = sqlx::query_as::<_, TrajectoryRow>(
            "SELECT steps_json, model, created_at FROM trajectories WHERE session_id = ? ORDER BY created_at DESC LIMIT 1"
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(r) = row {
            let steps: Vec<crate::trajectory::TrajectoryStep> =
                serde_json::from_str(&r.steps_json)?;
            Ok(Some(Trajectory {
                session_id: session_id.to_string(),
                steps,
                metadata: crate::trajectory::TrajectoryMetadata {
                    model: r.model.unwrap_or_default(),
                    created_at: r.created_at.unwrap_or_else(Utc::now),
                },
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn list_trajectories(&self) -> Result<Vec<(String, String, String)>> {
        let rows = sqlx::query_as::<_, (String, String, String)>(
            "SELECT session_id, model, created_at FROM trajectories ORDER BY created_at DESC LIMIT 50"
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn load_all_trajectories(&self, limit: usize) -> Result<Vec<Trajectory>> {
        #[derive(sqlx::FromRow)]
        struct TrajectoryRow {
            session_id: String,
            steps_json: String,
            model: Option<String>,
            created_at: Option<DateTime<Utc>>,
        }

        let rows = sqlx::query_as::<_, TrajectoryRow>(
            "SELECT session_id, steps_json, model, created_at FROM trajectories ORDER BY created_at DESC LIMIT ?"
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        let mut trajectories = Vec::new();
        for r in rows {
            if let Ok(steps) = serde_json::from_str::<Vec<crate::trajectory::TrajectoryStep>>(&r.steps_json) {
                trajectories.push(Trajectory {
                    session_id: r.session_id,
                    steps,
                    metadata: crate::trajectory::TrajectoryMetadata {
                        model: r.model.unwrap_or_default(),
                        created_at: r.created_at.unwrap_or_else(Utc::now),
                    },
                });
            }
        }
        Ok(trajectories)
    }

    pub async fn save_research_cache(
        &self,
        url: &str,
        summary: &str,
        depth: &str,
        metadata: &Value,
    ) -> Result<()> {
        let metadata_json = serde_json::to_string(metadata)?;
        sqlx::query(
            "INSERT OR REPLACE INTO research_cache (url, summary, depth, metadata) VALUES (?, ?, ?, ?)"
        )
        .bind(url)
        .bind(summary)
        .bind(depth)
        .bind(metadata_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_research_cache(&self, url: &str) -> Result<Option<(String, String, Value)>> {
        let row = sqlx::query_as::<_, (String, String, String)>(
            "SELECT summary, depth, metadata FROM research_cache WHERE url = ?",
        )
        .bind(url)
        .fetch_optional(&self.pool)
        .await?;

        if let Some((summary, depth, meta)) = row {
            let metadata = serde_json::from_str::<Value>(&meta).unwrap_or_default();
            Ok(Some((summary, depth, metadata)))
        } else {
            Ok(None)
        }
    }
}
#[async_trait::async_trait]
impl pharmakon_common::ResearchPersistence for DbSessionStore {
    async fn get_research_cache(
        &self,
        url: &str,
    ) -> anyhow::Result<Option<(String, String, Value)>> {
        self.get_research_cache(url).await
    }

    async fn save_research_cache(
        &self,
        url: &str,
        content: &str,
        depth: &str,
        metadata: &Value,
    ) -> anyhow::Result<()> {
        self.save_research_cache(url, content, depth, metadata)
            .await
    }
}
