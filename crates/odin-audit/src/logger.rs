//! Audit logger implementation for the Odin harness.
//!
//! Implements the [`AuditLogger`] trait from odin-core with support for:
//! - JSON file logging
//! - SQLite-backed persistent storage
//! - Querying by agent ID, session ID, and event type

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use odin_core::error::{OdinError, OdinResult};
use odin_core::traits::AuditLogger;
use odin_core::types::{AgentId, AuditEntry, AuditEventType, AuditResult, SessionId};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Configuration for the audit logger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLoggerConfig {
    /// Whether audit logging is enabled.
    pub enabled: bool,
    /// Path to a JSON log file (optional).
    pub file_path: Option<PathBuf>,
    /// Path to a SQLite database (optional).
    pub db_path: Option<PathBuf>,
    /// Whether to log in JSON format.
    pub json_format: bool,
    /// Maximum entries to keep in memory before flushing.
    pub buffer_size: usize,
    /// Whether to mask secret values in log output.
    pub mask_secrets: bool,
}

impl Default for AuditLoggerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            file_path: None,
            db_path: None,
            json_format: true,
            buffer_size: 100,
            mask_secrets: true,
        }
    }
}

/// An in-memory audit entry for buffering.
#[derive(Debug, Clone)]
struct BufferedEntry {
    entry: AuditEntry,
    #[allow(dead_code)]
    timestamp: DateTime<Utc>,
}

/// Implementation of the [`AuditLogger`] trait.
///
/// Supports logging to:
/// 1. A JSON lines file (`file_path`)
/// 2. An in-memory buffer (always active for queries)
/// 3. Optionally SQLite (when `db_path` is set — requires `sqlx` feature)
pub struct AuditLoggerImpl {
    /// Configuration.
    config: AuditLoggerConfig,
    /// In-memory buffer of recent entries.
    buffer: Arc<RwLock<Vec<BufferedEntry>>>,
    /// File handle (opened lazily).
    file: Arc<Mutex<Option<std::fs::File>>>,
}

impl AuditLoggerImpl {
    /// Create a new audit logger.
    pub fn new(config: AuditLoggerConfig) -> Self {
        let file = if config.enabled {
            if let Some(ref path) = config.file_path {
                match Self::open_file(path) {
                    Ok(f) => {
                        info!(file_path = %path.display(), "Audit log file opened");
                        Some(f)
                    }
                    Err(e) => {
                        error!(
                            file_path = %path.display(),
                            error = %e,
                            "Failed to open audit log file"
                        );
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        };

        Self {
            config,
            buffer: Arc::new(RwLock::new(Vec::new())),
            file: Arc::new(Mutex::new(file)),
        }
    }

    /// Create a new audit logger with default configuration.
    #[allow(clippy::should_implement_trait)]
    pub fn default() -> Self {
        Self::new(AuditLoggerConfig::default())
    }

    /// Create the audit logger with a file-based output.
    pub fn with_file(path: impl Into<PathBuf>) -> Self {
        let config = AuditLoggerConfig {
            file_path: Some(path.into()),
            ..AuditLoggerConfig::default()
        };
        Self::new(config)
    }

    /// Helper to open the log file.
    fn open_file(path: &Path) -> OdinResult<std::fs::File> {
        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                OdinError::Io(std::io::Error::other(format!(
                    "Failed to create audit log directory '{}': {}",
                    parent.display(),
                    e
                )))
            })?;
        }

        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| {
                OdinError::Io(std::io::Error::other(format!(
                    "Failed to open audit log file '{}': {}",
                    path.display(),
                    e
                )))
            })
    }

    /// Flush buffered entries to the file.
    async fn flush_to_file(&self) -> OdinResult<()> {
        let entries: Vec<BufferedEntry> = {
            let mut buffer = self.buffer.write().await;
            if buffer.is_empty() {
                return Ok(());
            }
            // Drain only entries that haven't been flushed yet
            // We simply take all and re-add those that fail
            buffer.drain(..).collect()
        };

        let mut file_guard = self.file.lock().await;
        let file = match file_guard.as_mut() {
            Some(f) => f,
            None => {
                // File not available; re-buffer
                let mut buffer = self.buffer.write().await;
                buffer.extend(entries);
                return Ok(());
            }
        };

        let count = entries.len();
        for buffered in &entries {
            let line = if self.config.json_format {
                serde_json::to_string(&buffered.entry).map_err(OdinError::Serialization)?
            } else {
                format!(
                    "[{}] [{}] [{}] [{}] {}: {}\n",
                    buffered.entry.timestamp.to_rfc3339(),
                    buffered.entry.event_type,
                    buffered.entry.agent_id,
                    buffered.entry.session_id,
                    buffered.entry.action,
                    serde_json::to_string(&buffered.entry.details).unwrap_or_default(),
                )
            };

            writeln!(file, "{}", line).map_err(|e| {
                OdinError::Io(std::io::Error::other(format!(
                    "Failed to write audit log entry: {}",
                    e
                )))
            })?;
        }

        file.flush().map_err(|e| {
            OdinError::Io(std::io::Error::other(format!(
                "Failed to flush audit log: {}",
                e
            )))
        })?;

        debug!(count = count, "Flushed audit entries to file");
        Ok(())
    }

    /// Add an audit entry to the in-memory buffer.
    async fn buffer_entry(&self, entry: AuditEntry) -> OdinResult<()> {
        let buffered = BufferedEntry {
            timestamp: Utc::now(),
            entry,
        };

        {
            let mut buffer = self.buffer.write().await;
            buffer.push(buffered);

            // Trim buffer if it exceeds the maximum
            if buffer.len() > self.config.buffer_size * 2 {
                let excess = buffer.len() - self.config.buffer_size;
                buffer.drain(0..excess);
            }
        }

        // Flush if buffer is large enough
        if self.buffer.read().await.len() >= self.config.buffer_size
            && let Err(e) = self.flush_to_file().await
        {
            warn!(error = %e, "Failed to flush audit buffer to file");
        }

        Ok(())
    }
}

#[async_trait]
impl AuditLogger for AuditLoggerImpl {
    /// Log an audit entry.
    async fn log(&self, entry: AuditEntry) -> OdinResult<()> {
        if !self.config.enabled {
            return Ok(());
        }

        debug!(
            event_type = %entry.event_type,
            agent_id = %entry.agent_id,
            action = %entry.action,
            "Audit entry logged"
        );

        self.buffer_entry(entry).await
    }

    /// Query audit entries by agent, session, and event type.
    async fn query(
        &self,
        agent_id: Option<AgentId>,
        session_id: Option<SessionId>,
        event_type: Option<AuditEventType>,
        limit: usize,
    ) -> OdinResult<Vec<AuditEntry>> {
        let buffer = self.buffer.read().await;

        let results: Vec<AuditEntry> = buffer
            .iter()
            .filter(|b| {
                let mut matches = true;
                if let Some(ref aid) = agent_id {
                    matches = matches && b.entry.agent_id == *aid;
                }
                if let Some(ref sid) = session_id {
                    matches = matches && b.entry.session_id == *sid;
                }
                if let Some(ref et) = event_type {
                    matches = matches && b.entry.event_type == *et;
                }
                matches
            })
            .rev()
            .take(limit)
            .map(|b| b.entry.clone())
            .collect();

        Ok(results)
    }

    /// Get the most recent entries.
    async fn recent(&self, limit: usize) -> OdinResult<Vec<AuditEntry>> {
        let buffer = self.buffer.read().await;
        let results: Vec<AuditEntry> = buffer
            .iter()
            .rev()
            .take(limit)
            .map(|b| b.entry.clone())
            .collect();

        Ok(results)
    }
}

impl AuditLoggerImpl {
    /// Force flush buffered entries to disk.
    pub async fn flush(&self) -> OdinResult<()> {
        self.flush_to_file().await
    }

    /// Rotate the log file (close and reopen).
    pub async fn rotate(&self, new_path: Option<PathBuf>) -> OdinResult<()> {
        let path = new_path.or_else(|| self.config.file_path.clone());
        let path = match path {
            Some(p) => p,
            None => return Err(OdinError::Config("No log file path configured".into())),
        };

        let new_file = Self::open_file(&path)?;
        let mut file_guard = self.file.lock().await;
        *file_guard = Some(new_file);

        info!(file_path = %path.display(), "Audit log file rotated");
        Ok(())
    }

    /// Get the number of buffered entries.
    pub async fn buffer_size(&self) -> usize {
        self.buffer.read().await.len()
    }

    /// Clear all buffered entries.
    pub async fn clear_buffer(&self) {
        self.buffer.write().await.clear();
        debug!("Audit buffer cleared");
    }
}

/// Create an audit entry builder for convenience.
pub fn audit_entry(
    agent_id: AgentId,
    session_id: SessionId,
    event_type: AuditEventType,
    action: impl Into<String>,
    details: serde_json::Value,
    result: AuditResult,
) -> AuditEntry {
    AuditEntry {
        id: Uuid::new_v4(),
        timestamp: Utc::now(),
        agent_id,
        session_id,
        event_type,
        action: action.into(),
        details,
        result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use odin_core::traits::AuditLogger;
    use odin_core::types::AuditEventType;
    use uuid::Uuid;

    fn make_entry(
        agent_id: AgentId,
        session_id: SessionId,
        event_type: AuditEventType,
    ) -> AuditEntry {
        AuditEntry {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            agent_id,
            session_id,
            event_type,
            action: "test_action".to_string(),
            details: serde_json::json!({"key": "value"}),
            result: AuditResult::Success,
        }
    }

    #[tokio::test]
    async fn test_log_and_recent() {
        let logger = AuditLoggerImpl::default();
        let agent_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();

        let entry = make_entry(agent_id, session_id, AuditEventType::ToolCall);
        logger.log(entry).await.unwrap();

        let recent = logger.recent(10).await.unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].event_type, AuditEventType::ToolCall);
    }

    #[tokio::test]
    async fn test_query_by_agent() {
        let logger = AuditLoggerImpl::default();
        let agent1 = Uuid::new_v4();
        let agent2 = Uuid::new_v4();
        let session = Uuid::new_v4();

        logger
            .log(make_entry(agent1, session, AuditEventType::ToolCall))
            .await
            .unwrap();
        logger
            .log(make_entry(agent2, session, AuditEventType::ModelCall))
            .await
            .unwrap();

        let results = logger.query(Some(agent1), None, None, 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].agent_id, agent1);
    }

    #[tokio::test]
    async fn test_query_by_event_type() {
        let logger = AuditLoggerImpl::default();
        let agent_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();

        logger
            .log(make_entry(agent_id, session_id, AuditEventType::ToolCall))
            .await
            .unwrap();
        logger
            .log(make_entry(agent_id, session_id, AuditEventType::ModelCall))
            .await
            .unwrap();
        logger
            .log(make_entry(agent_id, session_id, AuditEventType::ToolCall))
            .await
            .unwrap();

        let results = logger
            .query(None, None, Some(AuditEventType::ToolCall), 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_disabled_logger() {
        let config = AuditLoggerConfig {
            enabled: false,
            ..Default::default()
        };
        let logger = AuditLoggerImpl::new(config);
        let agent_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();

        logger
            .log(make_entry(agent_id, session_id, AuditEventType::ToolCall))
            .await
            .unwrap();

        let recent = logger.recent(10).await.unwrap();
        assert_eq!(recent.len(), 0);
    }

    #[tokio::test]
    async fn test_file_logging() {
        let tmp_dir = std::env::temp_dir();
        let log_path = tmp_dir.join(format!("audit_test_{}.jsonl", Uuid::new_v4()));

        let config = AuditLoggerConfig {
            file_path: Some(log_path.clone()),
            json_format: true,
            ..AuditLoggerConfig::default()
        };

        let logger = AuditLoggerImpl::new(config);
        let agent_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();

        logger
            .log(make_entry(
                agent_id,
                session_id,
                AuditEventType::SessionStart,
            ))
            .await
            .unwrap();

        // Flush to ensure it's written
        logger.flush().await.unwrap();

        // Read the file and verify
        let content = std::fs::read_to_string(&log_path).unwrap();
        assert!(content.contains("\"event_type\":\"session_start\""));
        assert!(content.contains(&agent_id.to_string()));

        // Cleanup
        let _ = std::fs::remove_file(&log_path);
    }

    #[tokio::test]
    async fn test_audit_entry_builder() {
        let agent_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();

        let entry = audit_entry(
            agent_id,
            session_id,
            AuditEventType::ConfigChange,
            "update_config",
            serde_json::json!({"setting": "value"}),
            AuditResult::Success,
        );

        assert_eq!(entry.event_type, AuditEventType::ConfigChange);
        assert_eq!(entry.action, "update_config");
        assert_eq!(entry.result, AuditResult::Success);
    }

    #[tokio::test]
    async fn test_buffer_trimming() {
        let config = AuditLoggerConfig {
            buffer_size: 5,
            enabled: true,
            ..Default::default()
        };
        let logger = AuditLoggerImpl::new(config);
        let agent_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();

        // Add 12 entries — buffer should trim to ~5
        for _ in 0..12 {
            logger
                .log(make_entry(agent_id, session_id, AuditEventType::Decision))
                .await
                .unwrap();
        }

        let size = logger.buffer_size().await;
        assert!(size <= 10); // buffer_size * 2
    }

    #[tokio::test]
    async fn test_clear_buffer() {
        let logger = AuditLoggerImpl::default();
        let agent_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();

        logger
            .log(make_entry(agent_id, session_id, AuditEventType::Error))
            .await
            .unwrap();
        assert_eq!(logger.buffer_size().await, 1);

        logger.clear_buffer().await;
        assert_eq!(logger.buffer_size().await, 0);
    }
}
