//! Permission policy engine for the Odin harness.
//!
//! Implements [`PermissionEngine`] from odin-core, providing:
//! - Tool call allow/deny rules based on configured rules
//! - Shell command safety checking via regex patterns
//! - Rate limiting per agent/tool
//! - Path boundary validation for filesystem access

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use odin_core::error::{OdinError, OdinResult};
use odin_core::types::{AgentId, PathBoundary, PermissionAction, PermissionRule};
use regex::Regex;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, trace, warn};

/// A rate-limit tracker for a single agent/tool pair.
#[derive(Debug, Clone)]
struct RateTracker {
    /// Timestamps of recent calls within the window.
    timestamps: Vec<DateTime<Utc>>,
    /// Maximum calls allowed per minute.
    max_per_minute: u32,
}

impl RateTracker {
    fn new(max_per_minute: u32) -> Self {
        Self {
            timestamps: Vec::new(),
            max_per_minute,
        }
    }

    /// Check if a call is allowed and record it if so.
    fn check_and_record(&mut self, now: &DateTime<Utc>) -> bool {
        // Remove timestamps older than 1 minute
        let cutoff = *now - chrono::TimeDelta::minutes(1);
        self.timestamps.retain(|t| *t > cutoff);

        if self.timestamps.len() < self.max_per_minute as usize {
            self.timestamps.push(*now);
            true
        } else {
            false
        }
    }
}

/// The permission policy engine.
///
/// Evaluates tool calls and commands against configured allow/deny rules,
/// rate limits, and path boundaries.
pub struct PolicyEngine {
    /// Permission rules indexed by tool name.
    rules: HashMap<String, PermissionRule>,
    /// Dangerous command patterns (regex).
    dangerous_patterns: Vec<Regex>,
    /// Path boundaries for filesystem operations.
    path_boundary: PathBoundary,
    /// Rate limit trackers: key = "agent_id:tool_name"
    rate_trackers: Arc<RwLock<HashMap<String, RateTracker>>>,
    /// Default max rate per minute.
    default_max_rate: u32,
    /// Whether to require approval by default.
    require_approval: bool,
}

impl PolicyEngine {
    /// Create a new policy engine.
    pub fn new(
        rules: Vec<PermissionRule>,
        dangerous_commands: &[String],
        path_boundary: PathBoundary,
        default_max_rate: u32,
        require_approval: bool,
    ) -> Self {
        let rules_map: HashMap<String, PermissionRule> = rules
            .into_iter()
            .map(|r| (r.tool_name.clone(), r))
            .collect();

        let dangerous_patterns: Vec<Regex> = dangerous_commands
            .iter()
            .filter_map(|pattern| {
                Regex::new(pattern)
                    .map_err(|e| {
                        warn!(
                            pattern = %pattern,
                            error = %e,
                            "Invalid dangerous command regex pattern"
                        );
                    })
                    .ok()
            })
            .collect();

        Self {
            rules: rules_map,
            dangerous_patterns,
            path_boundary,
            rate_trackers: Arc::new(RwLock::new(HashMap::new())),
            default_max_rate,
            require_approval,
        }
    }

    /// Create a policy engine with default safe settings.
    pub fn default() -> Self {
        Self::new(
            vec![],
            &[
                r"rm\s+-rf".into(),
                r"git\s+reset\s+--hard".into(),
                r"git\s+push\s+--force".into(),
                r"sudo\s+".into(),
                r"chmod\s+777".into(),
                r">\s*/dev/".into(),
                r"mkfs\.".into(),
                r"dd\s+if=".into(),
            ],
            PathBoundary::default(),
            60,
            true,
        )
    }

    /// Check if a shell command matches any dangerous pattern.
    pub fn is_dangerous_command(&self, command: &str) -> bool {
        self.dangerous_patterns
            .iter()
            .any(|re| re.is_match(command))
    }

    /// Validate that a path is within the allowed boundaries.
    pub fn check_path_boundary(&self, path: &std::path::Path, write: bool) -> OdinResult<()> {
        let path_str = path.to_string_lossy();

        // Check denied paths first
        for denied in &self.path_boundary.denied {
            if path_str.contains(denied) {
                return Err(OdinError::PermissionDenied(format!(
                    "Path '{}' is in the denied list (matches '{}')",
                    path_str, denied
                )));
            }
        }

        // Check allowed paths
        let allowed = if write {
            &self.path_boundary.allowed_write
        } else {
            &self.path_boundary.allowed_read
        };

        let is_allowed = allowed.iter().any(|allowed_prefix| {
            if allowed_prefix == "." {
                // "." means current directory and subdirectories
                // We accept any path that doesn't escape (no "../")
                !path_str.contains("..")
            } else {
                path_str.starts_with(allowed_prefix)
            }
        });

        if !is_allowed {
            return Err(OdinError::PermissionDenied(format!(
                "Path '{}' is outside allowed boundaries",
                path_str
            )));
        }

        Ok(())
    }
}

#[async_trait]
impl odin_core::traits::PermissionEngine for PolicyEngine {
    /// Check if a tool call is allowed based on configured rules.
    async fn check_tool(
        &self,
        agent_id: AgentId,
        tool_name: &str,
        _args: &serde_json::Value,
    ) -> OdinResult<PermissionAction> {
        // Check for explicit rules
        if let Some(rule) = self.rules.get(tool_name) {
            debug!(
                agent_id = %agent_id,
                tool = %tool_name,
                action = %rule.action,
                "Tool permission check via explicit rule"
            );
            return Ok(rule.action);
        }

        // Default: allow but require approval if configured
        if self.require_approval {
            debug!(
                agent_id = %agent_id,
                tool = %tool_name,
                "Tool permission check defaults to AskUser"
            );
            Ok(PermissionAction::AskUser)
        } else {
            debug!(
                agent_id = %agent_id,
                tool = %tool_name,
                "Tool permission check defaults to Allow"
            );
            Ok(PermissionAction::Allow)
        }
    }

    /// Check if a shell command is allowed.
    async fn check_command(
        &self,
        agent_id: AgentId,
        command: &str,
    ) -> OdinResult<PermissionAction> {
        if self.is_dangerous_command(command) {
            warn!(
                agent_id = %agent_id,
                command = %command,
                "Dangerous command detected"
            );
            return Ok(PermissionAction::AskUser);
        }

        debug!(
            agent_id = %agent_id,
            command = %command,
            "Command allowed"
        );
        Ok(PermissionAction::Allow)
    }

    /// Check rate limits for a specific agent and tool.
    async fn check_rate_limit(&self, agent_id: AgentId, tool_name: &str) -> OdinResult<bool> {
        let key = format!("{}:{}", agent_id, tool_name);

        // Determine max rate for this tool
        let max_rate = self
            .rules
            .get(tool_name)
            .and_then(|r| r.max_rate_per_minute)
            .unwrap_or(self.default_max_rate);

        let mut trackers = self.rate_trackers.write().await;
        let tracker = trackers
            .entry(key.clone())
            .or_insert_with(|| RateTracker::new(max_rate));

        let allowed = tracker.check_and_record(&Utc::now());
        if allowed {
            trace!(
                agent_id = %agent_id,
                tool = %tool_name,
                "Rate limit check passed"
            );
        } else {
            warn!(
                agent_id = %agent_id,
                tool = %tool_name,
                max_rate = max_rate,
                "Rate limit exceeded"
            );
        }

        Ok(allowed)
    }

    /// Request user approval for an action.
    async fn request_approval(
        &self,
        _agent_id: AgentId,
        action: &str,
        details: &str,
    ) -> OdinResult<bool> {
        // In the current implementation, we log and deny by default.
        // A real implementation would prompt via the gateway.
        warn!(
            action = %action,
            details = %details,
            "Approval requested but no interactive approval mechanism available — denying"
        );
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use odin_core::traits::PermissionEngine;
    use uuid::Uuid;

    fn test_agent() -> AgentId {
        Uuid::new_v4()
    }

    #[tokio::test]
    async fn test_default_allow_tool() {
        let engine = PolicyEngine::default();
        let agent = test_agent();
        let result = engine
            .check_tool(agent, "file_read", &serde_json::json!({}))
            .await
            .unwrap();
        // Default with require_approval=true returns AskUser
        assert_eq!(result, PermissionAction::AskUser);
    }

    #[tokio::test]
    async fn test_explicit_rule_deny() {
        let rules = vec![PermissionRule {
            tool_name: "shell".to_string(),
            action: PermissionAction::Deny,
            require_approval: true,
            max_rate_per_minute: None,
        }];
        let engine = PolicyEngine::new(rules, &[], PathBoundary::default(), 60, true);
        let agent = test_agent();
        let result = engine
            .check_tool(agent, "shell", &serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(result, PermissionAction::Deny);
    }

    #[tokio::test]
    async fn test_explicit_rule_allow() {
        let rules = vec![PermissionRule {
            tool_name: "shell".to_string(),
            action: PermissionAction::Allow,
            require_approval: false,
            max_rate_per_minute: None,
        }];
        let engine = PolicyEngine::new(rules, &[], PathBoundary::default(), 60, false);
        let agent = test_agent();
        let result = engine
            .check_tool(agent, "shell", &serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(result, PermissionAction::Allow);
    }

    #[tokio::test]
    async fn test_dangerous_command_detection() {
        let engine = PolicyEngine::default();
        assert!(engine.is_dangerous_command("rm -rf /"));
        assert!(engine.is_dangerous_command("sudo apt install"));
        assert!(engine.is_dangerous_command("chmod 777 /etc"));
        assert!(engine.is_dangerous_command("dd if=/dev/zero of=/dev/sda"));
        assert!(!engine.is_dangerous_command("ls -la"));
        assert!(!engine.is_dangerous_command("echo hello"));
    }

    #[tokio::test]
    async fn test_command_permission() {
        let engine = PolicyEngine::default();
        let agent = test_agent();

        // Dangerous command should require approval
        let result = engine
            .check_command(agent, "rm -rf /important")
            .await
            .unwrap();
        assert_eq!(result, PermissionAction::AskUser);

        // Safe command should be allowed
        let result = engine.check_command(agent, "ls -la").await.unwrap();
        assert_eq!(result, PermissionAction::Allow);
    }

    #[tokio::test]
    async fn test_rate_limit() {
        let engine = PolicyEngine::new(
            vec![],
            &[],
            PathBoundary::default(),
            3, // max 3 per minute
            false,
        );
        let agent = test_agent();

        // First 3 calls should pass
        assert!(engine.check_rate_limit(agent, "shell").await.unwrap());
        assert!(engine.check_rate_limit(agent, "shell").await.unwrap());
        assert!(engine.check_rate_limit(agent, "shell").await.unwrap());

        // 4th should fail
        assert!(!engine.check_rate_limit(agent, "shell").await.unwrap());
    }

    #[test]
    fn test_path_boundary_allowed() {
        let boundary = PathBoundary {
            allowed_read: vec!["/home/user".to_string()],
            allowed_write: vec!["/home/user".to_string()],
            denied: vec!["/etc/passwd".to_string()],
        };
        let engine = PolicyEngine::new(vec![], &[], boundary, 60, true);

        assert!(
            engine
                .check_path_boundary(std::path::Path::new("/home/user/docs/file.txt"), false)
                .is_ok()
        );
        assert!(
            engine
                .check_path_boundary(std::path::Path::new("/home/user/docs/file.txt"), true)
                .is_ok()
        );
    }

    #[test]
    fn test_path_boundary_denied() {
        let boundary = PathBoundary {
            allowed_read: vec!["/".to_string()],
            allowed_write: vec!["/home/user".to_string()],
            denied: vec!["/etc/passwd".to_string()],
        };
        let engine = PolicyEngine::new(vec![], &[], boundary, 60, true);

        assert!(
            engine
                .check_path_boundary(std::path::Path::new("/etc/passwd"), false)
                .is_err()
        );
    }

    #[test]
    fn test_path_boundary_outside() {
        let boundary = PathBoundary {
            allowed_read: vec!["/home/user".to_string()],
            allowed_write: vec!["/home/user".to_string()],
            denied: vec![],
        };
        let engine = PolicyEngine::new(vec![], &[], boundary, 60, true);

        assert!(
            engine
                .check_path_boundary(std::path::Path::new("/etc/config"), false)
                .is_err()
        );
        assert!(
            engine
                .check_path_boundary(std::path::Path::new("/etc/config"), true)
                .is_err()
        );
    }

    #[test]
    fn test_path_boundary_current_dir() {
        let boundary = PathBoundary {
            allowed_read: vec![".".to_string()],
            allowed_write: vec![".".to_string()],
            denied: vec![],
        };
        let engine = PolicyEngine::new(vec![], &[], boundary, 60, true);

        assert!(
            engine
                .check_path_boundary(std::path::Path::new("./src/main.rs"), false)
                .is_ok()
        );
        assert!(
            engine
                .check_path_boundary(std::path::Path::new("../outside"), false)
                .is_err()
        );
    }
}
