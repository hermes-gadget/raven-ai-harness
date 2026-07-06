//! DB-specific model extensions for memory storage.
//!
//! Re-exports core types and adds serialization helpers
//! used by the SQLite store.

use chrono::{DateTime, Utc};
use odin_core::{MemoryCategory, MemoryEntry};
use serde::{Deserialize, Serialize};

/// Intermediate row type used to deserialize from SQLite query results.
///
/// SQLite stores timestamps and tags as TEXT, so we need an
/// intermediate step before converting to [`MemoryEntry`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MemoryRow {
    pub id: String,
    pub content: String,
    pub category: String,
    pub created_at: String,
    pub updated_at: String,
    pub tags: String,
    pub importance: f64,
}

impl TryFrom<MemoryRow> for MemoryEntry {
    type Error = String;

    fn try_from(row: MemoryRow) -> Result<Self, Self::Error> {
        let created_at: DateTime<Utc> =
            DateTime::parse_from_rfc3339(&row.created_at)
                .map_err(|e| format!("Invalid created_at: {e}"))?
                .with_timezone(&Utc);

        let updated_at: DateTime<Utc> =
            DateTime::parse_from_rfc3339(&row.updated_at)
                .map_err(|e| format!("Invalid updated_at: {e}"))?
                .with_timezone(&Utc);

        let tags: Vec<String> =
            serde_json::from_str(&row.tags)
                .map_err(|e| format!("Invalid tags JSON: {e}"))?;

        let category: MemoryCategory =
            serde_json::from_value(serde_json::Value::String(row.category))
                .map_err(|e| format!("Invalid category: {e}"))?;

        Ok(MemoryEntry {
            id: row.id,
            content: row.content,
            category,
            created_at,
            updated_at,
            tags,
            importance: row.importance as f32,
        })
    }
}

impl MemoryRow {
    /// Build a `MemoryRow` from a `MemoryEntry` for insertion.
    pub fn from_entry(entry: &MemoryEntry) -> Self {
        Self {
            id: entry.id.clone(),
            content: entry.content.clone(),
            category: serde_json::to_value(&entry.category)
                .and_then(|v| Ok(v.as_str().unwrap_or("").to_string()))
                .unwrap_or_else(|_| "fact".to_string()),
            created_at: entry.created_at.to_rfc3339(),
            updated_at: entry.updated_at.to_rfc3339(),
            tags: serde_json::to_string(&entry.tags)
                .unwrap_or_else(|_| "[]".to_string()),
            importance: entry.importance as f64,
        }
    }
}
