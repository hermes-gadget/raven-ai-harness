//! Progress tracking for orchestration status.
//!
//! Tracks the state of each workstream (group of parallel sub-agents)
//! and provides status updates for CLI, API, Discord, and WebSocket.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Status of a single workstream (group of related sub-agents).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkstreamStatus {
    /// Unique workstream ID.
    pub id: Uuid,
    /// Human-readable label (e.g., "fix-cli-bug").
    pub label: String,
    /// Number of sub-agents in this workstream.
    pub total_agents: usize,
    /// Number of agents completed.
    pub completed: usize,
    /// Number of agents running.
    pub running: usize,
    /// Number of agents failed.
    pub failed: usize,
    /// Number of agents queued.
    pub queued: usize,
    /// When the workstream started.
    pub started_at: DateTime<Utc>,
    /// Estimated completion (if known).
    pub estimated_completion: Option<DateTime<Utc>>,
    /// Overall status message.
    pub status_message: String,
}

/// Overall progress tracker for all orchestration work.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressTracker {
    /// All active workstreams.
    pub workstreams: Vec<WorkstreamStatus>,
    /// Overall task graph progress.
    pub graph_progress: Option<(usize, usize)>, // (done, total)
    /// Last update timestamp.
    pub last_updated: DateTime<Utc>,
    /// Whether there are any active (non-terminal) workstreams.
    pub has_active_work: bool,
}

impl Default for ProgressTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressTracker {
    /// Create an empty progress tracker.
    pub fn new() -> Self {
        Self {
            workstreams: vec![],
            graph_progress: None,
            last_updated: Utc::now(),
            has_active_work: false,
        }
    }

    /// Add or update a workstream.
    pub fn update_workstream(&mut self, status: WorkstreamStatus) {
        if let Some(existing) = self.workstreams.iter_mut().find(|w| w.id == status.id) {
            *existing = status;
        } else {
            self.workstreams.push(status);
        }
        self.last_updated = Utc::now();
        self.recalculate_active();
    }

    /// Remove a completed workstream.
    pub fn remove_workstream(&mut self, id: Uuid) {
        self.workstreams.retain(|w| w.id != id);
        self.recalculate_active();
    }

    /// Set the overall task graph progress.
    pub fn set_graph_progress(&mut self, done: usize, total: usize) {
        self.graph_progress = Some((done, total));
    }

    /// Check if there are any active workstreams.
    fn recalculate_active(&mut self) {
        self.has_active_work = self
            .workstreams
            .iter()
            .any(|w| w.running > 0 || w.queued > 0);
    }

    /// Format a human-readable progress summary.
    pub fn format_summary(&self) -> String {
        let mut lines = vec![];

        if let Some((done, total)) = self.graph_progress {
            lines.push(format!("📊 Task Graph: {}/{} nodes complete", done, total));
        }

        for ws in &self.workstreams {
            let icon = if ws.failed > 0 {
                "⚠️"
            } else if ws.running > 0 {
                "🔄"
            } else if ws.completed == ws.total_agents {
                "✅"
            } else {
                "⏳"
            };
            lines.push(format!(
                "{} {}: {}/{}/{} (done/running/queued) [{} total]",
                icon, ws.label, ws.completed, ws.running, ws.queued, ws.total_agents
            ));
        }

        if lines.is_empty() {
            "No active workstreams.".to_string()
        } else {
            lines.join("\n")
        }
    }

    /// Format a compact one-line status.
    pub fn format_compact(&self) -> String {
        let (done, total) = self.graph_progress.unwrap_or((0, 0));
        let active = self.workstreams.iter().filter(|w| w.running > 0).count();
        format!(
            "📊 {}/{} nodes | {} active workstream(s)",
            done, total, active
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_workstream(
        label: &str,
        completed: usize,
        running: usize,
        queued: usize,
        total: usize,
    ) -> WorkstreamStatus {
        WorkstreamStatus {
            id: Uuid::new_v4(),
            label: label.into(),
            total_agents: total,
            completed,
            running,
            failed: 0,
            queued,
            started_at: Utc::now(),
            estimated_completion: None,
            status_message: "working".into(),
        }
    }

    #[test]
    fn test_progress_tracker_update() {
        let mut tracker = ProgressTracker::new();
        let ws = make_workstream("fix-bug", 1, 2, 1, 4);
        let id = ws.id;
        tracker.update_workstream(ws);

        assert_eq!(tracker.workstreams.len(), 1);
        assert!(tracker.has_active_work);

        // Update with completed state
        let updated = make_workstream("fix-bug", 4, 0, 0, 4);
        tracker.update_workstream(WorkstreamStatus { id, ..updated });
        assert_eq!(tracker.workstreams[0].completed, 4);
    }

    #[test]
    fn test_progress_tracker_no_active() {
        let mut tracker = ProgressTracker::new();
        let ws = make_workstream("done", 4, 0, 0, 4);
        tracker.update_workstream(ws);

        assert!(!tracker.has_active_work);
    }

    #[test]
    fn test_format_summary() {
        let mut tracker = ProgressTracker::new();
        tracker.set_graph_progress(2, 5);
        tracker.update_workstream(make_workstream("ws1", 1, 1, 0, 2));
        tracker.update_workstream(make_workstream("ws2", 0, 0, 3, 3));

        let summary = tracker.format_summary();
        assert!(summary.contains("2/5"));
        assert!(summary.contains("ws1"));
        assert!(summary.contains("ws2"));
    }

    #[test]
    fn test_format_compact() {
        let mut tracker = ProgressTracker::new();
        tracker.set_graph_progress(3, 6);
        tracker.update_workstream(make_workstream("ws1", 1, 1, 0, 2));

        let compact = tracker.format_compact();
        assert!(compact.contains("3/6"));
        assert!(compact.contains("active"));
    }

    #[test]
    fn test_remove_workstream() {
        let mut tracker = ProgressTracker::new();
        let ws = make_workstream("temp", 0, 0, 1, 1);
        let id = ws.id;
        tracker.update_workstream(ws);
        assert_eq!(tracker.workstreams.len(), 1);

        tracker.remove_workstream(id);
        assert_eq!(tracker.workstreams.len(), 0);
    }
}
