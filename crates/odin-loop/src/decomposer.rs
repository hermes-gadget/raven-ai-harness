//! Goal decomposition for complex tasks.
//!
//! Smaller models struggle with complex multi-step goals. The decomposer
//! breaks goals into atomic sub-tasks that can be tackled one at a time.

use odin_core::types::*;
use serde::{Deserialize, Serialize};

/// Decomposes complex goals into bite-sized sub-tasks.
#[derive(Debug, Clone)]
pub struct GoalDecomposer {
    /// Maximum sub-tasks to generate
    pub max_sub_tasks: usize,
    /// Maximum depth for recursive decomposition
    pub max_depth: u32,
}

impl Default for GoalDecomposer {
    fn default() -> Self {
        Self {
            max_sub_tasks: 3,
            max_depth: 2,
        }
    }
}

/// A decomposed plan with ordered sub-tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecomposedPlan {
    pub goal: String,
    pub sub_tasks: Vec<SubTask>,
    pub dependencies: Vec<Dependency>,
}

/// A dependency between sub-tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub from: String,
    pub to: String,
    pub reason: String,
}

impl GoalDecomposer {
    /// Decompose a goal using simple heuristics.
    ///
    /// For production use, this would call a lightweight model (e.g., a local
    /// 1B-7B model) to do the actual decomposition. The heuristic version here
    /// provides a baseline that works for common patterns.
    pub fn decompose_heuristic(&self, goal: &str) -> DecomposedPlan {
        let subtask_descriptions = self.break_down_goal(goal);
        let sub_tasks: Vec<SubTask> = subtask_descriptions
            .into_iter()
            .enumerate()
            .map(|(i, desc)| SubTask {
                id: format!("task_{}", i + 1),
                description: desc,
                status: SubTaskStatus::Pending,
                result: None,
            })
            .collect();

        // Default: sequential dependencies
        let dependencies: Vec<Dependency> = sub_tasks
            .windows(2)
            .map(|pair| Dependency {
                from: pair[0].id.clone(),
                to: pair[1].id.clone(),
                reason: "Sequential order".into(),
            })
            .collect();

        DecomposedPlan {
            goal: goal.to_string(),
            sub_tasks,
            dependencies,
        }
    }

    /// Simple heuristic decomposition based on common patterns.
    fn break_down_goal(&self, goal: &str) -> Vec<String> {
        let mut tasks = Vec::new();
        let lower = goal.to_lowercase();

        // Pattern 1: "Create/Write/Build X that does Y"
        if lower.contains("create") || lower.contains("build") || lower.contains("write") {
            tasks.push(format!("Plan the structure for: {}", goal));
            tasks.push(format!("Set up the project/files for: {}", goal));
            tasks.push(format!("Implement the core logic for: {}", goal));
            tasks.push(format!("Add error handling and edge cases for: {}", goal));
            tasks.push(format!("Test and verify: {}", goal));
        }
        // Pattern 2: "Fix/Debug/Resolve X"
        else if lower.contains("fix") || lower.contains("debug") || lower.contains("resolve") {
            tasks.push(format!("Reproduce and understand the issue: {}", goal));
            tasks.push(format!("Identify the root cause of: {}", goal));
            tasks.push(format!("Implement the fix for: {}", goal));
            tasks.push(format!("Verify the fix resolves: {}", goal));
        }
        // Pattern 3: "Analyze/Research/Investigate X"
        else if lower.contains("analyze") || lower.contains("research") || lower.contains("investigate") {
            tasks.push(format!("Gather information about: {}", goal));
            tasks.push(format!("Analyze findings for: {}", goal));
            tasks.push(format!("Summarize and report on: {}", goal));
        }
        // Pattern 4: Generic — break into prepare, execute, verify
        else {
            tasks.push(format!("Prepare and gather context for: {}", goal));
            tasks.push(format!("Execute: {}", goal));
            tasks.push(format!("Verify and validate results for: {}", goal));
        }

        // Cap at max_sub_tasks
        tasks.truncate(self.max_sub_tasks);

        // Ensure we have at least 2 tasks
        if tasks.len() < 2 {
            tasks.clear();
            tasks.push(format!("Step 1: {}", goal));
            tasks.push("Step 2: Verify completion".to_string());
        }

        tasks
    }

    /// Get the next pending sub-task.
    pub fn next_pending<'a>(&self, plan: &'a DecomposedPlan) -> Option<&'a SubTask> {
        plan.sub_tasks
            .iter()
            .find(|st| st.status == SubTaskStatus::Pending)
    }

    /// Mark a sub-task as completed.
    pub fn complete_task(plan: &mut DecomposedPlan, task_id: &str, result: Option<String>) {
        if let Some(task) = plan.sub_tasks.iter_mut().find(|st| st.id == task_id) {
            task.status = SubTaskStatus::Completed;
            task.result = result;
        }
    }

    /// Mark a sub-task as failed.
    pub fn fail_task(plan: &mut DecomposedPlan, task_id: &str, error: Option<String>) {
        if let Some(task) = plan.sub_tasks.iter_mut().find(|st| st.id == task_id) {
            task.status = SubTaskStatus::Failed;
            task.result = error;
        }
    }

    /// Check if all sub-tasks are complete.
    pub fn all_complete(&self, plan: &DecomposedPlan) -> bool {
        plan.sub_tasks
            .iter()
            .all(|st| st.status == SubTaskStatus::Completed || st.status == SubTaskStatus::Skipped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decompose_create_goal() {
        let decomposer = GoalDecomposer::default();
        let plan = decomposer.decompose_heuristic("Create a web server that serves static files");
        assert!(!plan.sub_tasks.is_empty());
        assert!(plan.sub_tasks.len() >= 2);
        // Should include planning, implementation, testing
        let descs: Vec<&str> = plan.sub_tasks.iter().map(|s| s.description.as_str()).collect();
        let combined = descs.join(" ");
        assert!(combined.contains("Plan") || combined.contains("plan"));
        assert!(combined.contains("Implement") || combined.contains("implement") || combined.contains("core"));
    }

    #[test]
    fn test_decompose_fix_goal() {
        let decomposer = GoalDecomposer::default();
        let plan = decomposer.decompose_heuristic("Fix the database connection leak");
        let descs: Vec<&str> = plan.sub_tasks.iter().map(|s| s.description.as_str()).collect();
        let combined = descs.join(" ");
        assert!(combined.contains("Reproduce") || combined.contains("Identify") || combined.contains("root cause"));
    }

    #[test]
    fn test_decompose_generic_goal() {
        let decomposer = GoalDecomposer::default();
        let plan = decomposer.decompose_heuristic("Review the latest PR");
        assert!(!plan.sub_tasks.is_empty());
        assert!(plan.sub_tasks.len() <= 10);
    }

    #[test]
    fn test_complete_and_check() {
        let decomposer = GoalDecomposer::default();
        let mut plan = decomposer.decompose_heuristic("Test goal");
        assert!(!decomposer.all_complete(&plan));

        let ids: Vec<String> = plan.sub_tasks.iter().map(|t| t.id.clone()).collect();
        for id in ids {
            GoalDecomposer::complete_task(&mut plan, &id, Some("Done".into()));
        }
        assert!(decomposer.all_complete(&plan));
    }

    #[test]
    fn test_dependencies_exist() {
        let decomposer = GoalDecomposer::default();
        let plan = decomposer.decompose_heuristic("Build a REST API");
        if plan.sub_tasks.len() >= 2 {
            assert!(!plan.dependencies.is_empty());
        }
    }
}
