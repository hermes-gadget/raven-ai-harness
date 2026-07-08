//! Sub-agent — a scoped agent with restricted tools, files, and permissions.
//!
//! Each sub-agent gets only the resources it needs for its specific task.
//! This minimizes risk and keeps context windows small.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Configuration for a sub-agent — scoped capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentConfig {
    /// Human-readable name (e.g., "fix-cli-bug").
    pub name: String,
    /// The specific goal this agent should accomplish.
    pub goal: String,
    /// Files this agent can READ (relative paths).
    pub read_files: Vec<String>,
    /// Files this agent can WRITE (relative paths).
    pub write_files: Vec<String>,
    /// Tool names this agent can use (e.g., ["file_read", "shell", "git"]).
    pub allowed_tools: Vec<String>,
    /// Capability tags required (e.g., ["filesystem", "git", "rust"]).
    pub required_capabilities: Vec<String>,
    /// Max iterations for the loop engine.
    pub max_iterations: u32,
    /// Priority (lower = higher).
    pub priority: u32,
    /// Optional parent task graph node ID.
    pub task_node_id: Option<Uuid>,
    /// Context/summary to inject (from upstream tasks).
    pub injected_context: Option<String>,
}

/// A sub-agent instance with lifecycle tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgent {
    /// Unique identifier.
    pub id: Uuid,
    /// Configuration.
    pub config: SubAgentConfig,
    /// Current lifecycle phase.
    pub phase: crate::lifecycle::AgentPhase,
    /// The runtime agent ID (from odin-runtime), once spawned.
    pub runtime_agent_id: Option<Uuid>,
    /// Result summary, if completed.
    pub result: Option<String>,
    /// Error message, if failed.
    pub error: Option<String>,
    /// When this sub-agent was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl SubAgent {
    /// Create a new sub-agent from config.
    pub fn new(config: SubAgentConfig) -> Self {
        Self {
            id: Uuid::new_v4(),
            config,
            phase: crate::lifecycle::AgentPhase::Queued,
            runtime_agent_id: None,
            result: None,
            error: None,
            created_at: chrono::Utc::now(),
        }
    }

    /// Check if this sub-agent needs a file write lock.
    pub fn needs_write_locks(&self) -> bool {
        !self.config.write_files.is_empty()
    }

    /// Get all files this agent touches.
    pub fn all_files(&self) -> Vec<String> {
        let mut files = self.config.read_files.clone();
        files.extend(self.config.write_files.clone());
        files.sort();
        files.dedup();
        files
    }

    /// Check if this agent overlaps with another on any files.
    pub fn file_overlap(&self, other: &SubAgent) -> Vec<String> {
        let self_files: std::collections::HashSet<_> = self.all_files().into_iter().collect();
        let other_files: std::collections::HashSet<_> = other.all_files().into_iter().collect();
        self_files.intersection(&other_files).cloned().collect()
    }

    /// Check if this agent has write overlap with another.
    pub fn write_conflict(&self, other: &SubAgent) -> Vec<String> {
        let self_writes: std::collections::HashSet<_> =
            self.config.write_files.iter().cloned().collect();
        let other_writes: std::collections::HashSet<_> =
            other.config.write_files.iter().cloned().collect();
        self_writes.intersection(&other_writes).cloned().collect()
    }
}

/// Builder for SubAgentConfig.
pub struct SubAgentConfigBuilder {
    config: SubAgentConfig,
}

impl SubAgentConfigBuilder {
    /// Start building a sub-agent config for a goal.
    pub fn new(name: impl Into<String>, goal: impl Into<String>) -> Self {
        Self {
            config: SubAgentConfig {
                name: name.into(),
                goal: goal.into(),
                read_files: vec![],
                write_files: vec![],
                allowed_tools: vec![],
                required_capabilities: vec![],
                max_iterations: 50,
                priority: 0,
                task_node_id: None,
                injected_context: None,
            },
        }
    }

    pub fn read_files(mut self, files: Vec<String>) -> Self {
        self.config.read_files = files;
        self
    }

    pub fn write_files(mut self, files: Vec<String>) -> Self {
        self.config.write_files = files;
        self
    }

    pub fn allowed_tools(mut self, tools: Vec<String>) -> Self {
        self.config.allowed_tools = tools;
        self
    }

    pub fn capabilities(mut self, caps: Vec<String>) -> Self {
        self.config.required_capabilities = caps;
        self
    }

    pub fn max_iterations(mut self, max: u32) -> Self {
        self.config.max_iterations = max;
        self
    }

    pub fn priority(mut self, prio: u32) -> Self {
        self.config.priority = prio;
        self
    }

    pub fn task_node(mut self, node_id: Uuid) -> Self {
        self.config.task_node_id = Some(node_id);
        self
    }

    pub fn context(mut self, ctx: impl Into<String>) -> Self {
        self.config.injected_context = Some(ctx.into());
        self
    }

    pub fn build(self) -> SubAgentConfig {
        self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sub_agent_creation() {
        let config = SubAgentConfigBuilder::new("test", "do something")
            .read_files(vec!["README.md".into()])
            .write_files(vec!["src/main.rs".into()])
            .allowed_tools(vec!["file_read".into(), "shell".into()])
            .build();

        let agent = SubAgent::new(config);
        assert_eq!(agent.config.name, "test");
        assert_eq!(agent.phase, crate::lifecycle::AgentPhase::Queued);
        assert!(agent.needs_write_locks());
    }

    #[test]
    fn test_file_overlap() {
        let a = SubAgent::new(
            SubAgentConfigBuilder::new("a", "task a")
                .read_files(vec!["shared.txt".into(), "a_only.txt".into()])
                .write_files(vec!["output.txt".into()])
                .build(),
        );

        let b = SubAgent::new(
            SubAgentConfigBuilder::new("b", "task b")
                .read_files(vec!["shared.txt".into()])
                .write_files(vec!["output.txt".into(), "b_only.txt".into()])
                .build(),
        );

        let overlap = a.file_overlap(&b);
        assert!(overlap.contains(&"shared.txt".to_string()));
        assert!(overlap.contains(&"output.txt".to_string()));
    }

    #[test]
    fn test_write_conflict() {
        let a = SubAgent::new(
            SubAgentConfigBuilder::new("a", "a")
                .write_files(vec!["main.rs".into()])
                .build(),
        );
        let b = SubAgent::new(
            SubAgentConfigBuilder::new("b", "b")
                .write_files(vec!["main.rs".into(), "lib.rs".into()])
                .build(),
        );

        let conflicts = a.write_conflict(&b);
        assert_eq!(conflicts, vec!["main.rs"]);
    }

    #[test]
    fn test_no_write_conflict() {
        let a = SubAgent::new(
            SubAgentConfigBuilder::new("a", "a")
                .write_files(vec!["a.rs".into()])
                .build(),
        );
        let b = SubAgent::new(
            SubAgentConfigBuilder::new("b", "b")
                .write_files(vec!["b.rs".into()])
                .build(),
        );

        assert!(a.write_conflict(&b).is_empty());
    }

    #[test]
    fn test_builder_full() {
        let node_id = Uuid::new_v4();
        let config = SubAgentConfigBuilder::new("full", "full task")
            .read_files(vec!["readme.md".into()])
            .write_files(vec!["src/main.rs".into()])
            .allowed_tools(vec!["file_read".into(), "shell".into(), "git".into()])
            .capabilities(vec!["filesystem".into(), "git".into()])
            .max_iterations(100)
            .priority(5)
            .task_node(node_id)
            .context("upstream summary")
            .build();

        assert_eq!(config.name, "full");
        assert_eq!(config.goal, "full task");
        assert_eq!(config.max_iterations, 100);
        assert_eq!(config.priority, 5);
        assert_eq!(config.task_node_id, Some(node_id));
        assert_eq!(config.injected_context, Some("upstream summary".into()));
    }
}
