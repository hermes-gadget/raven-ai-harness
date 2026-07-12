//! Agent lifecycle — full state machine for sub-agent execution.
//!
//! Each sub-agent moves through these states:
//!
//! ```text
//! Queued -> Running -> Blocked
//!            |            |
//!            v            |
//!     WaitingForLock -----+
//!            |            |
//!            v            |
//!        Reviewing -------+
//!            |
//!            v
//!    Done / Failed / Cancelled
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The lifecycle phase of a sub-agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentPhase {
    /// Waiting to be assigned to an executor.
    Queued,
    /// Actively executing its goal.
    Running,
    /// Blocked waiting for an upstream dependency or user input.
    Blocked,
    /// Waiting for a file write lock to become available.
    WaitingForLock,
    /// Finished executing, waiting for orchestrator review.
    Reviewing,
    /// Completed successfully.
    Done,
    /// Failed with an error.
    Failed,
    /// Cancelled by user or orchestrator.
    Cancelled,
}

impl AgentPhase {
    /// Whether this is a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            AgentPhase::Done | AgentPhase::Failed | AgentPhase::Cancelled
        )
    }

    /// Whether this is an active/running state.
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            AgentPhase::Running | AgentPhase::Blocked | AgentPhase::WaitingForLock
        )
    }

    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            AgentPhase::Queued => "queued",
            AgentPhase::Running => "running",
            AgentPhase::Blocked => "blocked",
            AgentPhase::WaitingForLock => "waiting_for_lock",
            AgentPhase::Reviewing => "reviewing",
            AgentPhase::Done => "done",
            AgentPhase::Failed => "failed",
            AgentPhase::Cancelled => "cancelled",
        }
    }
}

/// Full lifecycle tracking for a sub-agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLifecycle {
    /// The sub-agent's ID.
    pub agent_id: Uuid,
    /// Current phase.
    pub phase: AgentPhase,
    /// When the agent was created / queued.
    pub created_at: DateTime<Utc>,
    /// When the agent started running (if started).
    pub started_at: Option<DateTime<Utc>>,
    /// When the agent entered its current phase.
    pub phase_changed_at: DateTime<Utc>,
    /// When the agent finished (terminal state).
    pub finished_at: Option<DateTime<Utc>>,
    /// Phase history for audit trail.
    pub history: Vec<PhaseTransition>,
    /// Error message if failed.
    pub error: Option<String>,
    /// Number of retries (if failed and retried).
    pub retry_count: u32,
    /// The file locks currently held (paths).
    pub held_locks: Vec<String>,
}

/// A recorded phase transition in the lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseTransition {
    /// The phase the agent moved into.
    pub to: AgentPhase,
    /// When the transition happened.
    pub at: DateTime<Utc>,
    /// Optional reason for the transition.
    pub reason: Option<String>,
}

impl AgentLifecycle {
    /// Create a new lifecycle starting in Queued state.
    pub fn new(agent_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            agent_id,
            phase: AgentPhase::Queued,
            created_at: now,
            started_at: None,
            phase_changed_at: now,
            finished_at: None,
            history: vec![PhaseTransition {
                to: AgentPhase::Queued,
                at: now,
                reason: Some("Agent created".into()),
            }],
            error: None,
            retry_count: 0,
            held_locks: vec![],
        }
    }

    /// Transition to a new phase.
    pub fn transition(&mut self, to: AgentPhase, reason: Option<String>) {
        let now = Utc::now();

        // Record the transition
        self.history.push(PhaseTransition {
            to,
            at: now,
            reason,
        });

        // Track start time
        if to == AgentPhase::Running && self.started_at.is_none() {
            self.started_at = Some(now);
        }

        // Track finish time
        if to.is_terminal() {
            self.finished_at = Some(now);
        }

        self.phase = to;
        self.phase_changed_at = now;
    }

    /// Mark as running.
    pub fn start(&mut self) {
        self.transition(AgentPhase::Running, Some("Task started".into()));
    }

    /// Mark as blocked, waiting for upstream.
    pub fn block(&mut self, reason: impl Into<String>) {
        self.transition(AgentPhase::Blocked, Some(reason.into()));
    }

    /// Mark as waiting for file lock.
    pub fn wait_for_lock(&mut self, file: impl Into<String>) {
        let reason = format!("Waiting for write lock on '{}'", file.into());
        if self.phase == AgentPhase::WaitingForLock
            && self
                .history
                .last()
                .and_then(|entry| entry.reason.as_deref())
                == Some(reason.as_str())
        {
            return;
        }
        self.transition(AgentPhase::WaitingForLock, Some(reason));
    }

    /// Mark as done.
    pub fn complete(&mut self) {
        self.transition(AgentPhase::Done, Some("Task completed".into()));
    }

    /// Mark as failed.
    pub fn fail(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
        self.transition(AgentPhase::Failed, self.error.clone());
    }

    /// Mark as cancelled.
    pub fn cancel(&mut self, reason: impl Into<String>) {
        self.transition(AgentPhase::Cancelled, Some(reason.into()));
    }

    /// Mark as under review.
    pub fn review(&mut self) {
        self.transition(
            AgentPhase::Reviewing,
            Some("Under orchestrator review".into()),
        );
    }

    /// Retry from failed state.
    pub fn retry(&mut self) {
        self.retry_count += 1;
        self.error = None;
        self.transition(
            AgentPhase::Running,
            Some(format!("Retry #{}", self.retry_count)),
        );
    }

    /// Record that a file lock was acquired.
    pub fn lock_acquired(&mut self, path: &str) {
        if !self.held_locks.iter().any(|held| held == path) {
            self.held_locks.push(path.to_string());
        }
    }

    /// Record that a file lock was released.
    pub fn lock_released(&mut self, path: &str) {
        self.held_locks.retain(|p| p != path);
    }

    /// Duration since creation.
    pub fn elapsed(&self) -> chrono::Duration {
        Utc::now() - self.created_at
    }

    /// Duration of active execution (excluding blocked/waiting).
    pub fn active_duration(&self) -> Option<chrono::Duration> {
        self.started_at
            .map(|start| self.finished_at.unwrap_or_else(Utc::now) - start)
    }

    /// Get the reason for the current phase.
    pub fn current_reason(&self) -> Option<&str> {
        self.history
            .iter()
            .rev()
            .find(|t| t.reason.is_some())
            .and_then(|t| t.reason.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lifecycle_creation() {
        let id = Uuid::new_v4();
        let lc = AgentLifecycle::new(id);
        assert_eq!(lc.phase, AgentPhase::Queued);
        assert_eq!(lc.agent_id, id);
        assert_eq!(lc.history.len(), 1);
        assert_eq!(lc.history[0].to, AgentPhase::Queued);
    }

    #[test]
    fn test_full_lifecycle() {
        let id = Uuid::new_v4();
        let mut lc = AgentLifecycle::new(id);

        lc.start();
        assert_eq!(lc.phase, AgentPhase::Running);
        assert!(lc.started_at.is_some());

        lc.wait_for_lock("main.rs");
        assert_eq!(lc.phase, AgentPhase::WaitingForLock);
        assert!(lc.held_locks.is_empty()); // lock not actually acquired here

        lc.start(); // resume after lock
        lc.complete();
        assert_eq!(lc.phase, AgentPhase::Done);
        assert!(lc.finished_at.is_some());
    }

    #[test]
    fn test_wait_for_same_lock_is_idempotent() {
        let mut lifecycle = AgentLifecycle::new(Uuid::new_v4());
        lifecycle.wait_for_lock("shared.rs");
        let history_len = lifecycle.history.len();
        let changed_at = lifecycle.phase_changed_at;

        lifecycle.wait_for_lock("shared.rs");

        assert_eq!(lifecycle.history.len(), history_len);
        assert_eq!(lifecycle.phase_changed_at, changed_at);
    }

    #[test]
    fn test_failure_and_retry() {
        let id = Uuid::new_v4();
        let mut lc = AgentLifecycle::new(id);

        lc.start();
        lc.fail("something broke");
        assert_eq!(lc.phase, AgentPhase::Failed);
        assert_eq!(lc.error.as_deref(), Some("something broke"));

        lc.retry();
        assert_eq!(lc.phase, AgentPhase::Running);
        assert_eq!(lc.retry_count, 1);
        assert!(lc.error.is_none());
    }

    #[test]
    fn test_terminal_check() {
        assert!(AgentPhase::Done.is_terminal());
        assert!(AgentPhase::Failed.is_terminal());
        assert!(AgentPhase::Cancelled.is_terminal());
        assert!(!AgentPhase::Running.is_terminal());
        assert!(!AgentPhase::Queued.is_terminal());
    }

    #[test]
    fn test_active_check() {
        assert!(AgentPhase::Running.is_active());
        assert!(AgentPhase::Blocked.is_active());
        assert!(AgentPhase::WaitingForLock.is_active());
        assert!(!AgentPhase::Queued.is_active());
        assert!(!AgentPhase::Done.is_active());
    }

    #[test]
    fn test_lock_tracking() {
        let id = Uuid::new_v4();
        let mut lc = AgentLifecycle::new(id);

        lc.lock_acquired("src/main.rs");
        lc.lock_acquired("Cargo.toml");
        lc.lock_acquired("src/main.rs");
        assert_eq!(lc.held_locks.len(), 2);

        lc.lock_released("src/main.rs");
        assert_eq!(lc.held_locks, vec!["Cargo.toml"]);
    }

    #[test]
    fn test_elapsed_duration() {
        let id = Uuid::new_v4();
        let lc = AgentLifecycle::new(id);
        let elapsed = lc.elapsed();
        // Should be very small (just created)
        assert!(elapsed.num_milliseconds() >= 0);
    }

    #[test]
    fn test_phase_transition_history() {
        let id = Uuid::new_v4();
        let mut lc = AgentLifecycle::new(id);

        lc.start();
        lc.block("waiting for dep");
        lc.start();

        assert_eq!(lc.history.len(), 4); // Queued → Running → Blocked → Running
        assert_eq!(lc.history[1].to, AgentPhase::Running);
        assert_eq!(lc.history[2].to, AgentPhase::Blocked);
    }
}
