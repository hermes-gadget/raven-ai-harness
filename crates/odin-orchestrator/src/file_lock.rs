//! File-level locking for concurrent sub-agent execution.
//!
//! The FileLockManager ensures that while multiple sub-agents can read files
//! concurrently, only one agent can write to a file at a time. Conflicting
//! writes are queued. When the lock is released, the next writer is dequeued.
//!
//! IMPORTANT: Writes always use exclusive locks. Reads are concurrent.

use dashmap::DashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

/// Mode for a file lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockMode {
    /// Shared read lock — multiple agents can read concurrently.
    Read,
    /// Exclusive write lock — only one agent at a time.
    Write,
}

/// A file lock held by an agent.
#[derive(Debug, Clone)]
pub struct FileLock {
    /// The file path being locked (relative to workspace root).
    pub path: String,
    /// Lock mode.
    pub mode: LockMode,
    /// Agent ID holding the lock.
    pub agent_id: Uuid,
    /// When the lock was acquired.
    pub acquired_at: Instant,
}

/// Manages file-level locks for parallel sub-agent execution.
///
/// Rules:
/// - Multiple agents can hold READ locks on the same file.
/// - Only ONE agent can hold a WRITE lock on a file.
/// - No READ locks are allowed while a WRITE lock is held.
/// - Writers are queued FIFO.
pub struct FileLockManager {
    /// Currently held locks: file_path → Vec<FileLock>
    locks: Arc<DashMap<String, Vec<FileLock>>>,
    /// Queue of agents waiting for a write lock: file_path → VecDeque<(agent_id, queued_at)>
    write_queue: Arc<DashMap<String, VecDeque<QueuedWriter>>>,
}

/// A writer waiting in queue.
#[derive(Debug, Clone)]
struct QueuedWriter {
    agent_id: Uuid,
    #[allow(dead_code)]
    queued_at: Instant,
}

impl Default for FileLockManager {
    fn default() -> Self {
        Self::new()
    }
}

impl FileLockManager {
    /// Create a new empty file lock manager.
    pub fn new() -> Self {
        Self {
            locks: Arc::new(DashMap::new()),
            write_queue: Arc::new(DashMap::new()),
        }
    }

    /// Try to acquire a read lock on a file.
    /// Returns `Ok(())` if acquired, or `Err(msg)` if a write lock is held.
    pub fn acquire_read(&self, path: &str, agent_id: Uuid) -> Result<(), String> {
        let mut entry = self.locks.entry(path.to_string()).or_default();

        // Check if any write lock is held
        if entry.iter().any(|l| l.mode == LockMode::Write) {
            return Err(format!(
                "Cannot acquire read lock on '{}': write lock held",
                path
            ));
        }

        entry.push(FileLock {
            path: path.to_string(),
            mode: LockMode::Read,
            agent_id,
            acquired_at: Instant::now(),
        });

        tracing::debug!(
            "[FILE_LOCK] Agent {} acquired READ lock on '{}' ({} holders)",
            agent_id,
            path,
            entry.len()
        );
        Ok(())
    }

    /// Try to acquire a write lock on a file.
    /// If a lock (read or write) is held, the writer is queued.
    /// Returns `Ok(())` if acquired immediately, or `Err(queued_message)` if queued.
    pub fn acquire_write(&self, path: &str, agent_id: Uuid) -> Result<(), String> {
        let mut entry = self.locks.entry(path.to_string()).or_default();

        if entry.is_empty() {
            // No locks held — acquire immediately
            entry.push(FileLock {
                path: path.to_string(),
                mode: LockMode::Write,
                agent_id,
                acquired_at: Instant::now(),
            });
            tracing::info!(
                "[FILE_LOCK] Agent {} acquired WRITE lock on '{}'",
                agent_id,
                path
            );
            Ok(())
        } else {
            // Locks held — queue this writer
            let mut queue = self.write_queue.entry(path.to_string()).or_default();
            queue.push_back(QueuedWriter {
                agent_id,
                queued_at: Instant::now(),
            });
            tracing::info!(
                "[FILE_LOCK] Agent {} QUEUED for WRITE lock on '{}' (position: {})",
                agent_id,
                path,
                queue.len()
            );
            Err(format!(
                "Queued for write lock on '{}' (position: {})",
                path,
                queue.len()
            ))
        }
    }

    /// Release all locks held by an agent.
    /// When a write lock is released, the next writer in the queue is granted the lock.
    pub fn release_all(&self, agent_id: Uuid) -> Vec<String> {
        let mut released_paths = Vec::new();

        // Iterate over all locked files
        let paths: Vec<String> = self.locks.iter().map(|entry| entry.key().clone()).collect();

        for path in &paths {
            if let Some(mut entry) = self.locks.get_mut(path) {
                let had_write = entry.iter().any(|l| l.mode == LockMode::Write && l.agent_id == agent_id);
                entry.retain(|l| l.agent_id != agent_id);

                if entry.is_empty() {
                    // All locks released — grant to next queued writer, if any
                    drop(entry); // release the dashmap lock
                    self.locks.remove(path);

                    if let Some(mut queue) = self.write_queue.get_mut(path) {
                        if let Some(next) = queue.pop_front() {
                            // Grant write lock to next queued agent
                            let mut new_entry = self.locks.entry(path.clone()).or_default();
                            new_entry.push(FileLock {
                                path: path.clone(),
                                mode: LockMode::Write,
                                agent_id: next.agent_id,
                                acquired_at: Instant::now(),
                            });
                            tracing::info!(
                                "[FILE_LOCK] Granted queued WRITE lock on '{}' to agent {}",
                                path,
                                next.agent_id
                            );
                        }
                        if queue.is_empty() {
                            drop(queue);
                            self.write_queue.remove(path);
                        }
                    }
                }

                released_paths.push(path.clone());
                if had_write {
                    tracing::info!(
                        "[FILE_LOCK] Agent {} released WRITE lock on '{}'",
                        agent_id,
                        path
                    );
                }
            }
        }

        released_paths
    }

    /// Release a specific lock.
    pub fn release(&self, path: &str, agent_id: Uuid) {
        if let Some(mut entry) = self.locks.get_mut(path) {
            entry.retain(|l| l.agent_id != agent_id);
            if entry.is_empty() {
                drop(entry);
                self.locks.remove(path);

                // Grant to next queued writer
                if let Some(mut queue) = self.write_queue.get_mut(path) {
                    if let Some(next) = queue.pop_front() {
                        let mut new_entry = self.locks.entry(path.to_string()).or_default();
                        new_entry.push(FileLock {
                            path: path.to_string(),
                            mode: LockMode::Write,
                            agent_id: next.agent_id,
                            acquired_at: Instant::now(),
                        });
                    }
                    if queue.is_empty() {
                        drop(queue);
                        self.write_queue.remove(path);
                    }
                }
            }
        }
    }

    /// Check if a file has any locks.
    pub fn is_locked(&self, path: &str) -> bool {
        self.locks.contains_key(path)
    }

    /// Check if a file has a write lock.
    pub fn has_write_lock(&self, path: &str) -> bool {
        self.locks
            .get(path)
            .map(|entry| entry.iter().any(|l| l.mode == LockMode::Write))
            .unwrap_or(false)
    }

    /// Get all agents holding locks on a file.
    pub fn lock_holders(&self, path: &str) -> Vec<Uuid> {
        self.locks
            .get(path)
            .map(|entry| entry.iter().map(|l| l.agent_id).collect())
            .unwrap_or_default()
    }

    /// Get the queue length for a file.
    pub fn queue_length(&self, path: &str) -> usize {
        self.write_queue
            .get(path)
            .map(|q| q.len())
            .unwrap_or(0)
    }

    /// List all currently locked files.
    pub fn locked_files(&self) -> Vec<String> {
        self.locks.iter().map(|e| e.key().clone()).collect()
    }

    /// Summary of the lock manager state.
    pub fn summary(&self) -> FileLockSummary {
        let locked_count = self.locks.len();
        let queued_writers: usize = self.write_queue.iter().map(|q| q.len()).sum();
        let write_locked_count = self
            .locks
            .iter()
            .filter(|e| e.iter().any(|l| l.mode == LockMode::Write))
            .count();

        FileLockSummary {
            total_locked_files: locked_count,
            write_locked_files: write_locked_count,
            queued_writers,
        }
    }
}

/// Summary of the file lock manager state.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FileLockSummary {
    pub total_locked_files: usize,
    pub write_locked_files: usize,
    pub queued_writers: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_lock_concurrent() {
        let mgr = FileLockManager::new();
        let a1 = Uuid::new_v4();
        let a2 = Uuid::new_v4();

        assert!(mgr.acquire_read("test.txt", a1).is_ok());
        assert!(mgr.acquire_read("test.txt", a2).is_ok());

        let holders = mgr.lock_holders("test.txt");
        assert_eq!(holders.len(), 2);
    }

    #[test]
    fn test_write_lock_exclusive() {
        let mgr = FileLockManager::new();
        let a1 = Uuid::new_v4();
        let a2 = Uuid::new_v4();

        // First write succeeds
        assert!(mgr.acquire_write("test.txt", a1).is_ok());

        // Second write is queued
        let result = mgr.acquire_write("test.txt", a2);
        assert!(result.is_err()); // queued
        assert!(mgr.queue_length("test.txt") > 0);
    }

    #[test]
    fn test_read_blocked_by_write() {
        let mgr = FileLockManager::new();
        let writer = Uuid::new_v4();
        let reader = Uuid::new_v4();

        mgr.acquire_write("test.txt", writer).unwrap();
        let result = mgr.acquire_read("test.txt", reader);
        assert!(result.is_err());
    }

    #[test]
    fn test_release_grants_next_writer() {
        let mgr = FileLockManager::new();
        let a1 = Uuid::new_v4();
        let a2 = Uuid::new_v4();

        mgr.acquire_write("test.txt", a1).unwrap();
        let r2 = mgr.acquire_write("test.txt", a2);
        assert!(r2.is_err()); // queued

        // Release a1's lock
        mgr.release_all(a1);

        // a2 should now have the lock
        assert!(mgr.has_write_lock("test.txt"));
        let holders = mgr.lock_holders("test.txt");
        assert_eq!(holders, vec![a2]);
    }

    #[test]
    fn test_release_all_clears_multiple() {
        let mgr = FileLockManager::new();
        let agent = Uuid::new_v4();

        mgr.acquire_read("a.txt", agent).unwrap();
        mgr.acquire_read("b.txt", agent).unwrap();

        let paths = mgr.release_all(agent);
        assert_eq!(paths.len(), 2);
        assert!(!mgr.is_locked("a.txt"));
        assert!(!mgr.is_locked("b.txt"));
    }

    #[test]
    fn test_summary() {
        let mgr = FileLockManager::new();
        let a1 = Uuid::new_v4();
        let a2 = Uuid::new_v4();

        mgr.acquire_write("file1.txt", a1).unwrap();
        mgr.acquire_read("file2.txt", a2).unwrap();
        let _ = mgr.acquire_write("file1.txt", a2); // queued

        let summary = mgr.summary();
        assert_eq!(summary.total_locked_files, 2);
        assert_eq!(summary.write_locked_files, 1);
        assert_eq!(summary.queued_writers, 1);
    }

    #[test]
    fn test_read_after_write_release() {
        let mgr = FileLockManager::new();
        let writer = Uuid::new_v4();
        let reader = Uuid::new_v4();

        mgr.acquire_write("test.txt", writer).unwrap();
        mgr.release_all(writer);

        // Now reader should be able to acquire
        assert!(mgr.acquire_read("test.txt", reader).is_ok());
    }
}
