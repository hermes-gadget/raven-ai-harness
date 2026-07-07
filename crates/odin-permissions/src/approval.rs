//! Interactive approval gate for potentially dangerous operations.
//!
//! The [`ApprovalGate`] manages the lifecycle of an approval request,
//! allowing an operator (human or automated) to approve or deny
//! specific actions before they execute.

use odin_core::error::OdinResult;
use odin_core::types::AgentId;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// A single pending approval request.
#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    /// Unique request ID.
    pub id: String,
    /// The agent requesting approval.
    pub agent_id: AgentId,
    /// Description of the action.
    pub action: String,
    /// Detailed context about the action.
    pub details: String,
    /// Current status of the request.
    pub status: ApprovalStatus,
    /// Number of times this has been re-requested.
    pub retry_count: u32,
}

/// Status of an approval request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalStatus {
    /// Waiting for a response.
    Pending,
    /// Action was approved.
    Approved,
    /// Action was denied.
    Denied,
    /// Request expired.
    Expired,
}

/// Manages interactive command/tool approval.
///
/// Implements a simple queue-based approval system where actions
/// can be approved or denied asynchronously.
pub struct ApprovalGate {
    /// Pending approval requests.
    pending: Arc<RwLock<HashMap<String, ApprovalRequest>>>,
    /// Whether auto-approval is enabled (bypasses the gate).
    auto_approve: Arc<RwLock<bool>>,
    /// Timeout for pending requests in seconds.
    timeout_secs: u64,
}

impl ApprovalGate {
    /// Create a new approval gate.
    pub fn new(auto_approve: bool, timeout_secs: u64) -> Self {
        Self {
            pending: Arc::new(RwLock::new(HashMap::new())),
            auto_approve: Arc::new(RwLock::new(auto_approve)),
            timeout_secs,
        }
    }

    /// Submit a new approval request.
    ///
    /// Returns the request ID.
    pub async fn submit_request(
        &self,
        agent_id: AgentId,
        action: String,
        details: String,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let request = ApprovalRequest {
            id: id.clone(),
            agent_id,
            action,
            details,
            status: ApprovalStatus::Pending,
            retry_count: 0,
        };

        self.pending.write().await.insert(id.clone(), request);
        info!(request_id = %id, "Approval request submitted");
        id
    }

    /// Wait for an approval decision on a request.
    ///
    /// Blocks until the request is approved, denied, or expired.
    pub async fn await_decision(&self, request_id: &str) -> ApprovalStatus {
        // If auto-approve is on, immediately approve
        if *self.auto_approve.read().await {
            debug!(request_id = %request_id, "Auto-approving request");
            return ApprovalStatus::Approved;
        }

        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(self.timeout_secs);

        loop {
            if start.elapsed() > timeout {
                warn!(request_id = %request_id, "Approval request timed out");
                let mut pending = self.pending.write().await;
                if let Some(req) = pending.get_mut(request_id) {
                    req.status = ApprovalStatus::Expired;
                }
                return ApprovalStatus::Expired;
            }

            {
                let pending = self.pending.read().await;
                if let Some(req) = pending.get(request_id) {
                    match req.status {
                        ApprovalStatus::Approved => return ApprovalStatus::Approved,
                        ApprovalStatus::Denied => return ApprovalStatus::Denied,
                        ApprovalStatus::Expired => return ApprovalStatus::Expired,
                        ApprovalStatus::Pending => {}
                    }
                } else {
                    return ApprovalStatus::Expired;
                }
            }

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    /// Approve a pending request.
    pub async fn approve(&self, request_id: &str) -> OdinResult<bool> {
        let mut pending = self.pending.write().await;
        if let Some(req) = pending.get_mut(request_id) {
            if req.status == ApprovalStatus::Pending {
                req.status = ApprovalStatus::Approved;
                info!(request_id = %request_id, "Request approved");
                return Ok(true);
            }
            warn!(
                request_id = %request_id,
                status = ?req.status,
                "Request is not in pending state"
            );
            Ok(false)
        } else {
            warn!(request_id = %request_id, "Request not found");
            Ok(false)
        }
    }

    /// Deny a pending request.
    pub async fn deny(&self, request_id: &str) -> OdinResult<bool> {
        let mut pending = self.pending.write().await;
        if let Some(req) = pending.get_mut(request_id) {
            if req.status == ApprovalStatus::Pending {
                req.status = ApprovalStatus::Denied;
                info!(request_id = %request_id, "Request denied");
                return Ok(true);
            }
            Ok(false)
        } else {
            warn!(request_id = %request_id, "Request not found");
            Ok(false)
        }
    }

    /// Get all pending requests.
    pub async fn pending_requests(&self) -> Vec<ApprovalRequest> {
        let pending = self.pending.read().await;
        pending
            .values()
            .filter(|r| r.status == ApprovalStatus::Pending)
            .cloned()
            .collect()
    }

    /// Get a specific request by ID.
    pub async fn get_request(&self, request_id: &str) -> Option<ApprovalRequest> {
        self.pending.read().await.get(request_id).cloned()
    }

    /// Set auto-approve mode.
    pub async fn set_auto_approve(&self, enabled: bool) {
        *self.auto_approve.write().await = enabled;
        info!(auto_approve = enabled, "Auto-approve mode changed");
    }

    /// Check if auto-approve is enabled.
    pub async fn is_auto_approve(&self) -> bool {
        *self.auto_approve.read().await
    }

    /// Clean up expired requests.
    pub async fn cleanup_expired(&self) -> usize {
        let mut pending = self.pending.write().await;
        let before = pending.len();
        pending.retain(|_, req| req.status == ApprovalStatus::Pending);
        let cleaned = before - pending.len();
        if cleaned > 0 {
            debug!(count = cleaned, "Cleaned up expired approval requests");
        }
        cleaned
    }
}

impl Default for ApprovalGate {
    fn default() -> Self {
        Self::new(false, 30)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_submit_and_approve() {
        let gate = ApprovalGate::new(false, 30);
        let agent_id = Uuid::new_v4();

        let id = gate
            .submit_request(agent_id, "shell:rm -rf /".into(), "Remove root".into())
            .await;

        assert!(gate.approve(&id).await.unwrap());

        let status = gate.await_decision(&id).await;
        assert_eq!(status, ApprovalStatus::Approved);
    }

    #[tokio::test]
    async fn test_submit_and_deny() {
        let gate = ApprovalGate::new(false, 30);
        let agent_id = Uuid::new_v4();

        let id = gate
            .submit_request(agent_id, "shell:rm -rf /".into(), "Remove root".into())
            .await;

        assert!(gate.deny(&id).await.unwrap());

        let status = gate.await_decision(&id).await;
        assert_eq!(status, ApprovalStatus::Denied);
    }

    #[tokio::test]
    async fn test_auto_approve() {
        let gate = ApprovalGate::new(true, 30);
        let agent_id = Uuid::new_v4();

        let id = gate
            .submit_request(agent_id, "test".into(), "test".into())
            .await;

        // With auto-approve, await_decision should return Approved immediately
        let status = gate.await_decision(&id).await;
        assert_eq!(status, ApprovalStatus::Approved);
    }

    #[tokio::test]
    async fn test_timeout() {
        let gate = ApprovalGate::new(false, 1); // 1 second timeout
        let agent_id = Uuid::new_v4();

        let id = gate
            .submit_request(agent_id, "test".into(), "test".into())
            .await;

        let status = gate.await_decision(&id).await;
        assert_eq!(status, ApprovalStatus::Expired);
    }

    #[tokio::test]
    async fn test_pending_requests() {
        let gate = ApprovalGate::new(false, 30);
        let agent_id = Uuid::new_v4();

        gate.submit_request(agent_id, "action1".into(), "desc1".into())
            .await;
        gate.submit_request(agent_id, "action2".into(), "desc2".into())
            .await;

        let pending = gate.pending_requests().await;
        assert_eq!(pending.len(), 2);
    }

    #[tokio::test]
    async fn test_cleanup() {
        let gate = ApprovalGate::new(false, 30);
        let agent_id = Uuid::new_v4();

        let id = gate
            .submit_request(agent_id, "test".into(), "test".into())
            .await;
        gate.approve(&id).await.unwrap();

        // After approval, cleanup removes the completed request
        let cleaned = gate.cleanup_expired().await;
        assert_eq!(cleaned, 1);
    }
}
