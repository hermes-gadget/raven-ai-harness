//! Composer — the user-facing orchestrator agent.
//!
//! The Composer is the central intelligence of Raven Agent. It:
//! - Receives user intent (one or more requests in one message)
//! - Decomposes into a task graph
//! - Detects independent vs. dependent workstreams
//! - Spawns scoped sub-agents with appropriate tools/files/permissions
//! - Manages file lock acquisition and queueing
//! - Tracks progress and lifecycle of all sub-agents
//! - Handles interruptions (pause, cancel, redirect, reprioritize)
//! - Merges parallel results into one coherent response
//!
//! Default behavior: multi-agent orchestration. One user message → auto-split.

use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::file_lock::FileLockManager;
use crate::lifecycle::{AgentLifecycle, AgentPhase};
use crate::merge::{MergeResolver, MergeStrategy, SubAgentResult};
use crate::progress::{ProgressTracker, WorkstreamStatus};
use crate::sub_agent::{SubAgent, SubAgentConfig, SubAgentConfigBuilder};
use crate::task_graph::{TaskGraph, TaskGraphStatus, TaskNode, TaskNodeStatus};

/// Result from the composer — the merged output of all sub-agents.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ComposerResult {
    /// The original user goal.
    pub goal: String,
    /// Whether the overall goal succeeded.
    pub success: bool,
    /// Human-readable summary.
    pub summary: String,
    /// Per-agent results.
    pub agent_results: Vec<SubAgentResult>,
    /// Files modified.
    pub modified_files: Vec<String>,
    /// Any merge conflicts requiring user input.
    pub conflicts: Vec<crate::merge::FileConflict>,
    /// Total duration in milliseconds.
    pub duration_ms: u64,
    /// Number of sub-agents spawned.
    pub agent_count: usize,
}

/// Configuration for the Composer.
#[derive(Debug, Clone)]
pub struct ComposerConfig {
    /// Maximum sub-agents to run in parallel.
    pub max_parallel: usize,
    /// Default max iterations per sub-agent.
    pub default_max_iterations: u32,
    /// Whether to auto-merge results without user review.
    pub auto_merge: bool,
    /// Merge strategy for parallel results.
    pub merge_strategy: MergeStrategy,
    /// Workspace root for file operations.
    pub workspace_root: String,
    /// Whether to persist orchestration state to SQLite.
    pub persist_state: bool,
}

impl Default for ComposerConfig {
    fn default() -> Self {
        Self {
            max_parallel: 10,
            default_max_iterations: 50,
            auto_merge: true,
            merge_strategy: MergeStrategy::Auto,
            workspace_root: ".".to_string(),
            persist_state: true,
        }
    }
}

/// The Composer — main orchestration engine.
pub struct Composer {
    /// Configuration.
    config: ComposerConfig,
    /// File lock manager for concurrent access.
    file_locks: Arc<FileLockManager>,
    /// Merge resolver for combining results.
    merge_resolver: MergeResolver,
    /// Progress tracker for status reporting.
    progress: ProgressTracker,
    /// Active task graphs.
    graphs: HashMap<String, TaskGraph>,
    /// Active sub-agents: agent_id → (SubAgent, AgentLifecycle)
    agents: HashMap<Uuid, (SubAgent, AgentLifecycle)>,
    /// Workstreams: label → Vec<agent_id>
    workstreams: HashMap<String, Vec<Uuid>>,
}

impl Default for Composer {
    fn default() -> Self {
        Self::new(ComposerConfig::default())
    }
}

impl Composer {
    /// Create a new Composer.
    pub fn new(config: ComposerConfig) -> Self {
        Self {
            config,
            file_locks: Arc::new(FileLockManager::new()),
            merge_resolver: MergeResolver::new(),
            progress: ProgressTracker::new(),
            graphs: HashMap::new(),
            agents: HashMap::new(),
            workstreams: HashMap::new(),
        }
    }

    /// Create a Composer with a custom FileLockManager.
    pub fn with_file_locks(mut self, locks: Arc<FileLockManager>) -> Self {
        self.file_locks = locks;
        self
    }

    // ── Intent Intake & Decomposition ───────────────────────────

    /// Process a user message. This is the main entry point.
    ///
    /// The composer analyzes the message, detects if it contains multiple
    /// independent requests, creates a task graph, and schedules work.
    pub fn intake(&mut self, goal: &str) -> &TaskGraph {
        let graph = self.decompose(goal);
        let root = graph.root_goal.clone();
        self.graphs.insert(root.clone(), graph);
        self.graphs.get(&root).unwrap()
    }

    /// Decompose a goal into a task graph.
    ///
    /// In the full system, this uses an LLM to decompose. For now,
    /// we provide a heuristic decomposition that splits on common
    /// delimiters and detects independent workstreams.
    fn decompose(&self, goal: &str) -> TaskGraph {
        let mut graph = TaskGraph::new(goal.to_string());

        // Heuristic: split on "and", ";", or numbered items
        let sub_goals = self.split_goal(goal);

        if sub_goals.len() <= 1 {
            // Single task — create one node
            graph.add_node(TaskNode {
                id: Uuid::new_v4(),
                label: "main".to_string(),
                goal: goal.to_string(),
                read_files: vec![],
                write_files: vec![],
                required_capabilities: vec![],
                priority: 0,
                status: TaskNodeStatus::Pending,
                result: None,
                agent_id: None,
            });
        } else {
            // Multiple sub-goals — they're independent (no dependencies)
            for (i, sg) in sub_goals.iter().enumerate() {
                let label = format!("task-{}", i + 1);
                graph.add_node(TaskNode {
                    id: Uuid::new_v4(),
                    label,
                    goal: sg.clone(),
                    read_files: vec![],
                    write_files: vec![],
                    required_capabilities: vec![],
                    priority: i as u32,
                    status: TaskNodeStatus::Pending,
                    result: None,
                    agent_id: None,
                });
            }
        }

        graph.status = TaskGraphStatus::Running;
        graph
    }

    /// Heuristic goal splitting.
    fn split_goal(&self, goal: &str) -> Vec<String> {
        // Split on "and" at word boundaries (but not within "command" etc.)
        let parts: Vec<&str> = goal.split_inclusive(&[',', ';'][..]).collect();

        if parts.len() > 1 {
            parts
                .iter()
                .map(|s| s.trim().trim_matches(&[',', ';'][..]).trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            // Try splitting on " and " as a fallback
            let parts: Vec<&str> = goal.split(" and ").collect();
            if parts.len() > 1 {
                parts.iter().map(|s| s.trim().to_string()).collect()
            } else {
                vec![goal.to_string()]
            }
        }
    }

    // ── Sub-Agent Management ────────────────────────────────────

    /// Create a sub-agent config for a task graph node.
    pub fn create_sub_agent(&self, node: &TaskNode) -> SubAgentConfig {
        SubAgentConfigBuilder::new(&node.label, &node.goal)
            .read_files(node.read_files.clone())
            .write_files(node.write_files.clone())
            .capabilities(node.required_capabilities.clone())
            .max_iterations(self.config.default_max_iterations)
            .priority(node.priority)
            .task_node(node.id)
            .build()
    }

    /// Register a sub-agent and its lifecycle.
    pub fn register_agent(&mut self, config: SubAgentConfig) -> Uuid {
        let agent = SubAgent::new(config.clone());
        let agent_id = agent.id;
        let lifecycle = AgentLifecycle::new(agent_id);

        self.agents.insert(agent_id, (agent, lifecycle));

        // Track by workstream
        let ws_label = config.name.clone();
        self.workstreams.entry(ws_label).or_default().push(agent_id);

        agent_id
    }

    /// Get a sub-agent by ID.
    pub fn get_agent(&self, id: &Uuid) -> Option<&(SubAgent, AgentLifecycle)> {
        self.agents.get(id)
    }

    /// Get a mutable reference to a sub-agent and lifecycle.
    pub fn get_agent_mut(&mut self, id: &Uuid) -> Option<&mut (SubAgent, AgentLifecycle)> {
        self.agents.get_mut(id)
    }

    /// Start a sub-agent (transition from Queued to Running).
    pub fn start_agent(&mut self, id: Uuid) -> Result<(), String> {
        let (agent, lifecycle) = self
            .agents
            .get_mut(&id)
            .ok_or_else(|| format!("Agent {} not found", id))?;

        // Acquire file locks if needed
        if agent.needs_write_locks() {
            for file in &agent.config.write_files {
                match self.file_locks.acquire_write(file, id) {
                    Ok(()) => {
                        lifecycle.lock_acquired(file);
                    }
                    Err(msg) => {
                        lifecycle.wait_for_lock(file);
                        return Err(msg);
                    }
                }
            }
        }

        // Acquire read locks for files we just read
        for file in &agent.config.read_files {
            let _ = self.file_locks.acquire_read(file, id);
        }

        agent.phase = AgentPhase::Running;
        lifecycle.start();
        Ok(())
    }

    /// Complete a sub-agent.
    pub fn complete_agent(&mut self, id: Uuid, result: SubAgentResult) {
        if let Some((agent, lifecycle)) = self.agents.get_mut(&id) {
            agent.phase = AgentPhase::Done;
            agent.result = Some(result.summary.clone());
            lifecycle.complete();

            // Release all file locks
            self.file_locks.release_all(id);
        }
    }

    /// Fail a sub-agent.
    pub fn fail_agent(&mut self, id: Uuid, error: impl Into<String>) {
        let error = error.into();
        if let Some((agent, lifecycle)) = self.agents.get_mut(&id) {
            agent.phase = AgentPhase::Failed;
            agent.error = Some(error.clone());
            lifecycle.fail(error);

            // Release all file locks
            self.file_locks.release_all(id);
        }
    }

    /// Cancel a sub-agent.
    pub fn cancel_agent(&mut self, id: Uuid, reason: impl Into<String>) {
        if let Some((agent, lifecycle)) = self.agents.get_mut(&id) {
            agent.phase = AgentPhase::Cancelled;
            lifecycle.cancel(reason);

            // Release all file locks
            self.file_locks.release_all(id);
        }
    }

    /// Pause all running agents (mark as blocked, release file locks).
    pub fn pause_all(&mut self) {
        let ids: Vec<Uuid> = self
            .agents
            .iter()
            .filter(|(_, (a, _))| a.phase == AgentPhase::Running)
            .map(|(id, _)| *id)
            .collect();

        for id in &ids {
            // Release file locks for this agent so others can proceed
            self.file_locks.release_all(*id);
        }

        for (agent, lifecycle) in self.agents.values_mut() {
            if agent.phase == AgentPhase::Running {
                agent.phase = AgentPhase::Blocked;
                lifecycle.block("Paused by user");
            }
        }
    }

    /// Resume all blocked agents (re-acquire file locks if needed).
    pub fn resume_all(&mut self) -> Result<usize, String> {
        let mut resumed = 0;
        let ids: Vec<Uuid> = self
            .agents
            .iter()
            .filter(|(_, (a, _))| a.phase == AgentPhase::Blocked)
            .map(|(id, _)| *id)
            .collect();

        for id in ids {
            // Re-acquire file locks if needed, then start
            let needs_locks = {
                if let Some((agent, _)) = self.agents.get(&id) {
                    agent.needs_write_locks()
                } else {
                    false
                }
            };

            if needs_locks {
                // Try to acquire write locks
                let write_files: Vec<String> = self
                    .agents
                    .get(&id)
                    .map(|(a, _)| a.config.write_files.clone())
                    .unwrap_or_default();

                let mut all_acquired = true;
                for file in &write_files {
                    if self.file_locks.acquire_write(file, id).is_err() {
                        all_acquired = false;
                        break;
                    }
                }
                if !all_acquired {
                    continue; // could not re-acquire locks, skip
                }
            }

            // Also acquire read locks
            if let Some((agent, _)) = self.agents.get(&id) {
                for file in &agent.config.read_files {
                    let _ = self.file_locks.acquire_read(file, id);
                }
            }

            // Transition to running
            if let Some((agent, lifecycle)) = self.agents.get_mut(&id) {
                agent.phase = AgentPhase::Running;
                lifecycle.start();
            }
            resumed += 1;
        }
        Ok(resumed)
    }

    // ── Workstream Detection ──────────────────────────────────────

    /// Detect independent workstreams from a task graph.
    /// Returns groups of node IDs that can run in parallel.
    pub fn detect_workstreams(&self, graph: &TaskGraph) -> Vec<Vec<Uuid>> {
        graph.independent_groups()
    }

    /// Check if two sub-agents have overlapping file access.
    pub fn has_file_overlap(&self, a_id: Uuid, b_id: Uuid) -> bool {
        let a = self.agents.get(&a_id);
        let b = self.agents.get(&b_id);
        match (a, b) {
            (Some((a_agent, _)), Some((b_agent, _))) => !a_agent.file_overlap(b_agent).is_empty(),
            _ => false,
        }
    }

    // ── Merge & Results ───────────────────────────────────────────

    /// Collect results from completed agents and merge.
    pub fn collect_results(&self) -> Vec<SubAgentResult> {
        self.agents
            .values()
            .filter(|(a, _)| a.phase == AgentPhase::Done || a.phase == AgentPhase::Failed)
            .map(|(agent, lifecycle)| SubAgentResult {
                agent_id: agent.id,
                name: agent.config.name.clone(),
                summary: agent.result.clone().unwrap_or_else(|| "No summary".into()),
                output: None,
                modified_files: agent.config.write_files.clone(),
                success: agent.phase == AgentPhase::Done,
                error: agent.error.clone(),
                duration_ms: lifecycle
                    .active_duration()
                    .map(|d| d.num_milliseconds() as u64)
                    .unwrap_or(0),
            })
            .collect()
    }

    /// Merge all collected results into a final response.
    pub fn merge_results(&self, results: Vec<SubAgentResult>) -> ComposerResult {
        let merged = self
            .merge_resolver
            .merge(results, self.config.merge_strategy);

        ComposerResult {
            goal: self
                .graphs
                .values()
                .next()
                .map(|g| g.root_goal.clone())
                .unwrap_or_default(),
            success: merged.success,
            summary: merged.summary,
            agent_results: merged.results,
            modified_files: merged.all_modified_files,
            conflicts: merged.conflicts,
            duration_ms: 0, // set externally
            agent_count: self.agents.len(),
        }
    }

    // ── Progress & Status ─────────────────────────────────────────

    /// Update the progress tracker from current state.
    pub fn update_progress(&mut self) {
        for (ws_label, agent_ids) in &self.workstreams {
            let total = agent_ids.len();
            let completed = agent_ids
                .iter()
                .filter(|id| {
                    self.agents
                        .get(id)
                        .map(|(a, _)| a.phase == AgentPhase::Done)
                        .unwrap_or(false)
                })
                .count();
            let running = agent_ids
                .iter()
                .filter(|id| {
                    self.agents
                        .get(id)
                        .map(|(a, _)| a.phase.is_active())
                        .unwrap_or(false)
                })
                .count();
            let failed = agent_ids
                .iter()
                .filter(|id| {
                    self.agents
                        .get(id)
                        .map(|(a, _)| a.phase == AgentPhase::Failed)
                        .unwrap_or(false)
                })
                .count();
            let queued = agent_ids
                .iter()
                .filter(|id| {
                    self.agents
                        .get(id)
                        .map(|(a, _)| a.phase == AgentPhase::Queued)
                        .unwrap_or(false)
                })
                .count();

            self.progress.update_workstream(WorkstreamStatus {
                id: Uuid::new_v4(),
                label: ws_label.clone(),
                total_agents: total,
                completed,
                running,
                failed,
                queued,
                started_at: chrono::Utc::now(),
                estimated_completion: None,
                status_message: if failed > 0 {
                    format!("{} failed", failed)
                } else {
                    format!("{}/{} complete", completed, total)
                },
            });
        }
    }

    /// Get a progress summary string.
    pub fn progress_summary(&self) -> String {
        self.progress.format_summary()
    }

    /// Get a compact one-line status.
    pub fn compact_status(&self) -> String {
        self.progress.format_compact()
    }

    /// Check if all work is complete.
    pub fn is_all_done(&self) -> bool {
        self.agents.values().all(|(a, _)| a.phase.is_terminal())
    }

    /// Check if any agent has failed.
    pub fn has_failures(&self) -> bool {
        self.agents
            .values()
            .any(|(a, _)| a.phase == AgentPhase::Failed)
    }

    // ── Interruption Handling ─────────────────────────────────────

    /// Cancel all agents.
    pub fn cancel_all(&mut self, reason: &str) {
        let ids: Vec<Uuid> = self.agents.keys().copied().collect();
        for id in ids {
            self.cancel_agent(id, reason);
        }
    }

    /// Reprioritize an agent (set its priority).
    pub fn reprioritize(&mut self, id: Uuid, new_priority: u32) -> Result<(), String> {
        if let Some((agent, _)) = self.agents.get_mut(&id) {
            agent.config.priority = new_priority;
            Ok(())
        } else {
            Err(format!("Agent {} not found", id))
        }
    }

    // ── File Lock Access ──────────────────────────────────────────

    /// Get a reference to the file lock manager.
    pub fn file_locks(&self) -> &Arc<FileLockManager> {
        &self.file_locks
    }

    /// Get file lock summary.
    pub fn lock_summary(&self) -> crate::file_lock::FileLockSummary {
        self.file_locks.summary()
    }

    // ── Graph Access ──────────────────────────────────────────────

    /// Get the task graph for a root goal.
    pub fn get_graph(&self, root_goal: &str) -> Option<&TaskGraph> {
        self.graphs.get(root_goal)
    }

    /// Update a node status in the task graph.
    pub fn update_node_status(&mut self, root_goal: &str, node_id: Uuid, status: TaskNodeStatus) {
        if let Some(graph) = self.graphs.get_mut(root_goal) {
            graph.update_node_status(node_id, status);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decompose_single_goal() {
        let composer = Composer::default();
        let goal = "Fix the CLI bug";
        let graph = composer.decompose(goal);

        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.root_goal, goal);
    }

    #[test]
    fn test_decompose_multiple_goals() {
        let composer = Composer::default();
        let goal = "Fix the CLI bug, improve docs, add tests";
        let graph = composer.decompose(goal);

        assert_eq!(graph.nodes.len(), 3);
        assert!(graph.nodes.values().any(|n| n.goal.contains("Fix")));
        assert!(graph.nodes.values().any(|n| n.goal.contains("docs")));
        assert!(graph.nodes.values().any(|n| n.goal.contains("tests")));
    }

    #[test]
    fn test_split_goal_semicolons() {
        let composer = Composer::default();
        let parts = composer.split_goal("task a; task b; task c");
        assert_eq!(parts.len(), 3);
    }

    #[test]
    fn test_split_goal_and() {
        let composer = Composer::default();
        let parts = composer.split_goal("task a and task b and task c");
        assert_eq!(parts.len(), 3);
    }

    #[test]
    fn test_split_goal_single() {
        let composer = Composer::default();
        let parts = composer.split_goal("just one task");
        assert_eq!(parts.len(), 1);
    }

    #[test]
    fn test_agent_registration_and_lifecycle() {
        let mut composer = Composer::default();
        let config = SubAgentConfigBuilder::new("test", "do work").build();
        let id = composer.register_agent(config);

        assert!(composer.get_agent(&id).is_some());
        assert_eq!(composer.get_agent(&id).unwrap().1.phase, AgentPhase::Queued);

        composer.start_agent(id).unwrap();
        assert_eq!(
            composer.get_agent(&id).unwrap().1.phase,
            AgentPhase::Running
        );
    }

    #[test]
    fn test_agent_completion() {
        let mut composer = Composer::default();
        let config = SubAgentConfigBuilder::new("test", "do work").build();
        let id = composer.register_agent(config);
        composer.start_agent(id).unwrap();

        let result = SubAgentResult {
            agent_id: id,
            name: "test".into(),
            summary: "done!".into(),
            output: None,
            modified_files: vec![],
            success: true,
            error: None,
            duration_ms: 100,
        };
        composer.complete_agent(id, result);
        assert_eq!(composer.get_agent(&id).unwrap().1.phase, AgentPhase::Done);
    }

    #[test]
    fn test_agent_failure() {
        let mut composer = Composer::default();
        let config = SubAgentConfigBuilder::new("test", "do work").build();
        let id = composer.register_agent(config);
        composer.start_agent(id).unwrap();
        composer.fail_agent(id, "something broke");

        let (agent, lifecycle) = composer.get_agent(&id).unwrap();
        assert_eq!(lifecycle.phase, AgentPhase::Failed);
        assert_eq!(agent.error.as_deref(), Some("something broke"));
    }

    #[test]
    fn test_cancel_all() {
        let mut composer = Composer::default();
        let id1 = composer.register_agent(SubAgentConfigBuilder::new("a", "a").build());
        let id2 = composer.register_agent(SubAgentConfigBuilder::new("b", "b").build());
        composer.start_agent(id1).unwrap();
        composer.start_agent(id2).unwrap();

        composer.cancel_all("user interrupted");
        assert!(composer.is_all_done());
    }

    #[test]
    fn test_pause_and_resume() {
        let mut composer = Composer::default();
        let id = composer.register_agent(SubAgentConfigBuilder::new("a", "a").build());
        composer.start_agent(id).unwrap();

        composer.pause_all();
        assert_eq!(
            composer.get_agent(&id).unwrap().0.phase,
            AgentPhase::Blocked
        );

        let resumed = composer.resume_all().unwrap();
        assert_eq!(resumed, 1);
        assert_eq!(
            composer.get_agent(&id).unwrap().0.phase,
            AgentPhase::Running
        );
    }

    #[test]
    fn test_file_lock_acquire_on_start() {
        let mut composer = Composer::default();
        let config = SubAgentConfigBuilder::new("writer", "write stuff")
            .write_files(vec!["main.rs".into()])
            .build();
        let id = composer.register_agent(config);

        composer.start_agent(id).unwrap();
        assert!(composer.file_locks.has_write_lock("main.rs"));

        composer.complete_agent(
            id,
            SubAgentResult {
                agent_id: id,
                name: "writer".into(),
                summary: "done".into(),
                output: None,
                modified_files: vec!["main.rs".into()],
                success: true,
                error: None,
                duration_ms: 0,
            },
        );
        assert!(!composer.file_locks.has_write_lock("main.rs"));
    }

    #[test]
    fn test_write_lock_queueing() {
        let mut composer = Composer::default();

        // First writer gets the lock
        let config1 = SubAgentConfigBuilder::new("w1", "write 1")
            .write_files(vec!["shared.rs".into()])
            .build();
        let id1 = composer.register_agent(config1);
        composer.start_agent(id1).unwrap();

        // Second writer should queue
        let config2 = SubAgentConfigBuilder::new("w2", "write 2")
            .write_files(vec!["shared.rs".into()])
            .build();
        let id2 = composer.register_agent(config2);
        let result = composer.start_agent(id2);
        assert!(result.is_err()); // queued

        // Release first → second should get lock
        composer.complete_agent(
            id1,
            SubAgentResult {
                agent_id: id1,
                name: "w1".into(),
                summary: "done".into(),
                output: None,
                modified_files: vec!["shared.rs".into()],
                success: true,
                error: None,
                duration_ms: 0,
            },
        );

        assert!(composer.file_locks.has_write_lock("shared.rs"));
    }

    #[test]
    fn test_collect_and_merge_results() {
        let mut composer = Composer::default();
        let id1 = composer.register_agent(
            SubAgentConfigBuilder::new("a", "task a")
                .write_files(vec!["a.txt".into()])
                .build(),
        );
        let id2 = composer.register_agent(
            SubAgentConfigBuilder::new("b", "task b")
                .write_files(vec!["b.txt".into()])
                .build(),
        );

        composer.start_agent(id1).unwrap();
        composer.start_agent(id2).unwrap();

        composer.complete_agent(
            id1,
            SubAgentResult {
                agent_id: id1,
                name: "a".into(),
                summary: "Fixed A".into(),
                output: None,
                modified_files: vec!["a.txt".into()],
                success: true,
                error: None,
                duration_ms: 100,
            },
        );
        composer.complete_agent(
            id2,
            SubAgentResult {
                agent_id: id2,
                name: "b".into(),
                summary: "Fixed B".into(),
                output: None,
                modified_files: vec!["b.txt".into()],
                success: true,
                error: None,
                duration_ms: 200,
            },
        );

        let results = composer.collect_results();
        assert_eq!(results.len(), 2);

        let merged = composer.merge_results(results);
        assert!(merged.success);
        assert_eq!(merged.modified_files.len(), 2);
    }

    #[test]
    fn test_reprioritize() {
        let mut composer = Composer::default();
        let config = SubAgentConfigBuilder::new("test", "do work")
            .priority(10)
            .build();
        let id = composer.register_agent(config);

        composer.reprioritize(id, 1).unwrap();
        let (agent, _) = composer.get_agent(&id).unwrap();
        assert_eq!(agent.config.priority, 1);
    }

    #[test]
    fn test_is_all_done() {
        let mut composer = Composer::default();
        assert!(composer.is_all_done()); // no agents = done

        let id = composer.register_agent(SubAgentConfigBuilder::new("a", "a").build());
        assert!(!composer.is_all_done()); // queued

        composer.start_agent(id).unwrap();
        assert!(!composer.is_all_done()); // running

        composer.complete_agent(
            id,
            SubAgentResult {
                agent_id: id,
                name: "a".into(),
                summary: "done".into(),
                output: None,
                modified_files: vec![],
                success: true,
                error: None,
                duration_ms: 0,
            },
        );
        assert!(composer.is_all_done());
    }

    #[test]
    fn test_progress_update() {
        let mut composer = Composer::default();
        let id = composer.register_agent(SubAgentConfigBuilder::new("ws1", "task").build());
        composer.start_agent(id).unwrap();
        composer.complete_agent(
            id,
            SubAgentResult {
                agent_id: id,
                name: "ws1".into(),
                summary: "done".into(),
                output: None,
                modified_files: vec![],
                success: true,
                error: None,
                duration_ms: 0,
            },
        );

        composer.update_progress();
        let summary = composer.progress_summary();
        assert!(summary.contains("ws1"));
    }
}
