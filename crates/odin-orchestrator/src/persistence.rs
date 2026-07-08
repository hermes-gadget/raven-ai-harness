//! SQLite persistence for orchestration state.
//!
//! Ensures task graphs, agent lifecycles, and file lock state survive restarts.
//! Uses SQLite via sqlx for durable storage.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::PathBuf;
use uuid::Uuid;

use crate::lifecycle::AgentLifecycle;
use crate::task_graph::TaskGraph;

/// Error type for orchestration storage operations.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Not found: {0}")]
    NotFound(String),
}

/// Trait for orchestration state persistence.
#[async_trait]
pub trait OrchestrationStore: Send + Sync {
    /// Save a task graph.
    async fn save_task_graph(&self, graph: &TaskGraph) -> Result<(), StoreError>;
    /// Load a task graph by its first node ID (or root).
    async fn load_task_graph(&self, root_id: &str) -> Result<TaskGraph, StoreError>;
    /// List all stored task graphs.
    async fn list_task_graphs(&self) -> Result<Vec<TaskGraphSummary>, StoreError>;

    /// Save an agent lifecycle.
    async fn save_agent_lifecycle(&self, lifecycle: &AgentLifecycle) -> Result<(), StoreError>;
    /// Load an agent lifecycle.
    async fn load_agent_lifecycle(&self, agent_id: Uuid) -> Result<AgentLifecycle, StoreError>;
    /// List all stored lifecycles.
    async fn list_agent_lifecycles(&self) -> Result<Vec<AgentLifecycleSummary>, StoreError>;

    /// Initialize the database (create tables if needed).
    async fn initialize(&self) -> Result<(), StoreError>;
}

/// Summary of a stored task graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskGraphSummary {
    pub root_goal: String,
    pub status: String,
    pub node_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Summary of a stored agent lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLifecycleSummary {
    pub agent_id: String,
    pub phase: String,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

/// SQLite-backed orchestration store.
pub struct SqliteOrchestrationStore {
    pool: SqlitePool,
}

impl SqliteOrchestrationStore {
    /// Create a new SQLite store at the given path.
    pub async fn new(path: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let options = SqliteConnectOptions::new()
            .filename(path.into())
            .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        Ok(Self { pool })
    }

    /// Create an in-memory store (for testing).
    pub async fn new_in_memory() -> Result<Self, StoreError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;

        Ok(Self { pool })
    }
}

#[async_trait]
impl OrchestrationStore for SqliteOrchestrationStore {
    async fn initialize(&self) -> Result<(), StoreError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS task_graphs (
                root_id TEXT PRIMARY KEY,
                root_goal TEXT NOT NULL,
                graph_json TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'running',
                node_count INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS agent_lifecycles (
                agent_id TEXT PRIMARY KEY,
                graph_root_id TEXT,
                phase TEXT NOT NULL DEFAULT 'queued',
                lifecycle_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                finished_at TEXT
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn save_task_graph(&self, graph: &TaskGraph) -> Result<(), StoreError> {
        // Use root_goal as the key
        let root_id = graph.root_goal.clone();
        let graph_json = serde_json::to_string(&graph)?;
        let now = Utc::now().to_rfc3339();
        let status = serde_json::to_string(&graph.status)?;
        let node_count = graph.nodes.len() as i64;

        sqlx::query(
            r#"
            INSERT INTO task_graphs (root_id, root_goal, graph_json, status, node_count, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(root_id) DO UPDATE SET
                graph_json = excluded.graph_json,
                status = excluded.status,
                node_count = excluded.node_count,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&root_id)
        .bind(&graph.root_goal)
        .bind(&graph_json)
        .bind(&status)
        .bind(node_count)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn load_task_graph(&self, root_id: &str) -> Result<TaskGraph, StoreError> {
        let row = sqlx::query_as::<_, (String,)>(
            "SELECT graph_json FROM task_graphs WHERE root_id = ?",
        )
        .bind(root_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StoreError::NotFound(format!("Task graph '{}' not found", root_id)))?;

        let graph: TaskGraph = serde_json::from_str(&row.0)?;
        Ok(graph)
    }

    async fn list_task_graphs(&self) -> Result<Vec<TaskGraphSummary>, StoreError> {
        let rows = sqlx::query_as::<_, (String, String, i64, String, String)>(
            "SELECT root_goal, status, node_count, created_at, updated_at FROM task_graphs ORDER BY updated_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(goal, status, count, created, updated)| TaskGraphSummary {
                root_goal: goal,
                status,
                node_count: count,
                created_at: DateTime::parse_from_rfc3339(&created)
                    .unwrap_or_default()
                    .with_timezone(&Utc),
                updated_at: DateTime::parse_from_rfc3339(&updated)
                    .unwrap_or_default()
                    .with_timezone(&Utc),
            })
            .collect())
    }

    async fn save_agent_lifecycle(&self, lifecycle: &AgentLifecycle) -> Result<(), StoreError> {
        let agent_id = lifecycle.agent_id.to_string();
        let lifecycle_json = serde_json::to_string(&lifecycle)?;
        let phase = lifecycle.phase.label().to_string();
        let now = Utc::now().to_rfc3339();
        let finished = lifecycle.finished_at.map(|t| t.to_rfc3339());

        sqlx::query(
            r#"
            INSERT INTO agent_lifecycles (agent_id, phase, lifecycle_json, created_at, finished_at)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(agent_id) DO UPDATE SET
                phase = excluded.phase,
                lifecycle_json = excluded.lifecycle_json,
                finished_at = excluded.finished_at
            "#,
        )
        .bind(&agent_id)
        .bind(&phase)
        .bind(&lifecycle_json)
        .bind(&now)
        .bind(&finished)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn load_agent_lifecycle(&self, agent_id: Uuid) -> Result<AgentLifecycle, StoreError> {
        let row = sqlx::query_as::<_, (String,)>(
            "SELECT lifecycle_json FROM agent_lifecycles WHERE agent_id = ?",
        )
        .bind(agent_id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| {
            StoreError::NotFound(format!("Agent lifecycle '{}' not found", agent_id))
        })?;

        let lifecycle: AgentLifecycle = serde_json::from_str(&row.0)?;
        Ok(lifecycle)
    }

    async fn list_agent_lifecycles(&self) -> Result<Vec<AgentLifecycleSummary>, StoreError> {
        let rows = sqlx::query_as::<_, (String, String, String, Option<String>)>(
            "SELECT agent_id, phase, created_at, finished_at FROM agent_lifecycles ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, phase, created, finished)| AgentLifecycleSummary {
                agent_id: id,
                phase,
                created_at: DateTime::parse_from_rfc3339(&created)
                    .unwrap_or_default()
                    .with_timezone(&Utc),
                finished_at: finished.map(|f| {
                    DateTime::parse_from_rfc3339(&f)
                        .unwrap_or_default()
                        .with_timezone(&Utc)
                }),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::AgentPhase;
    use crate::task_graph::{TaskGraphStatus, TaskNode};

    async fn setup_store() -> SqliteOrchestrationStore {
        let store = SqliteOrchestrationStore::new_in_memory().await.unwrap();
        store.initialize().await.unwrap();
        store
    }

    #[tokio::test]
    async fn test_save_and_load_task_graph() {
        let store = setup_store().await;
        let mut graph = TaskGraph::new("test-root");
        let node = TaskNode {
            id: Uuid::new_v4(),
            label: "n1".into(),
            goal: "do stuff".into(),
            read_files: vec![],
            write_files: vec![],
            required_capabilities: vec![],
            priority: 0,
            status: crate::task_graph::TaskNodeStatus::Pending,
            result: None,
            agent_id: None,
        };
        graph.add_node(node);
        graph.status = TaskGraphStatus::Running;

        store.save_task_graph(&graph).await.unwrap();

        let loaded = store.load_task_graph("test-root").await.unwrap();
        assert_eq!(loaded.root_goal, "test-root");
        assert_eq!(loaded.nodes.len(), 1);
    }

    #[tokio::test]
    async fn test_save_and_load_agent_lifecycle() {
        let store = setup_store().await;
        let agent_id = Uuid::new_v4();
        let mut lifecycle = AgentLifecycle::new(agent_id);
        lifecycle.start();
        lifecycle.complete();

        store.save_agent_lifecycle(&lifecycle).await.unwrap();

        let loaded = store.load_agent_lifecycle(agent_id).await.unwrap();
        assert_eq!(loaded.phase, AgentPhase::Done);
        assert_eq!(loaded.agent_id, agent_id);
    }

    #[tokio::test]
    async fn test_list_task_graphs() {
        let store = setup_store().await;
        let graph = TaskGraph::new("list-test");
        store.save_task_graph(&graph).await.unwrap();

        let graphs = store.list_task_graphs().await.unwrap();
        assert!(!graphs.is_empty());
        assert!(graphs.iter().any(|g| g.root_goal == "list-test"));
    }

    #[tokio::test]
    async fn test_list_agent_lifecycles() {
        let store = setup_store().await;
        let lifecycle = AgentLifecycle::new(Uuid::new_v4());
        store.save_agent_lifecycle(&lifecycle).await.unwrap();

        let lifecycles = store.list_agent_lifecycles().await.unwrap();
        assert!(!lifecycles.is_empty());
    }

    #[tokio::test]
    async fn test_not_found() {
        let store = setup_store().await;
        let result = store.load_task_graph("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_existing() {
        let store = setup_store().await;
        let mut graph = TaskGraph::new("update-test");
        graph.status = TaskGraphStatus::Running;
        store.save_task_graph(&graph).await.unwrap();

        // Update
        graph.status = TaskGraphStatus::Complete;
        store.save_task_graph(&graph).await.unwrap();

        let loaded = store.load_task_graph("update-test").await.unwrap();
        assert_eq!(loaded.status, TaskGraphStatus::Complete);
    }
}
