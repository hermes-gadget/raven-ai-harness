//! Merge resolver — combines parallel sub-agent results into one coherent response.
//!
//! When multiple sub-agents complete in parallel, their raw results need to be
//! merged into a single user-facing response. The merge resolver handles:
//! - Collecting results from all sub-agents
//! - Detecting conflicts (two agents changed the same file)
//! - Applying merge strategies (concatenate, pick-first, last-wins, manual)
//! - Producing a final summary for the user

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Strategy for merging sub-agent results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MergeStrategy {
    /// Concatenate all results in order.
    Concatenate,
    /// Pick the first non-error result.
    FirstWins,
    /// Pick the last non-error result.
    LastWins,
    /// Flag conflict for user resolution.
    Manual,
    /// Only merge if no conflicts; otherwise Manual.
    Auto,
}

/// A result from one sub-agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentResult {
    /// Sub-agent ID.
    pub agent_id: Uuid,
    /// Sub-agent name.
    pub name: String,
    /// Human-readable summary of what was done.
    pub summary: String,
    /// Full output (may be large).
    pub output: Option<String>,
    /// Files that were modified.
    pub modified_files: Vec<String>,
    /// Whether the sub-agent succeeded.
    pub success: bool,
    /// Error message if failed.
    pub error: Option<String>,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

/// The merged result from all sub-agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergedResult {
    /// Overall success (all succeeded).
    pub success: bool,
    /// Combined summary for the user.
    pub summary: String,
    /// Per-agent results.
    pub results: Vec<SubAgentResult>,
    /// Files modified across all agents.
    pub all_modified_files: Vec<String>,
    /// Detected conflicts (file → list of agent names that modified it).
    pub conflicts: Vec<FileConflict>,
    /// Merge strategy used.
    pub strategy: MergeStrategy,
    /// Whether manual resolution is needed.
    pub needs_user_input: bool,
}

/// A file conflict detected during merge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileConflict {
    /// The conflicted file.
    pub file: String,
    /// Agents that modified this file.
    pub agents: Vec<String>,
    /// Severity: "low" (text files, different sections) or "high" (same lines).
    pub severity: String,
}

/// Resolves merges between parallel sub-agent results.
pub struct MergeResolver;

impl Default for MergeResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl MergeResolver {
    /// Create a new merge resolver.
    pub fn new() -> Self {
        Self
    }

    /// Merge sub-agent results using the given strategy.
    pub fn merge(
        &self,
        results: Vec<SubAgentResult>,
        strategy: MergeStrategy,
    ) -> MergedResult {
        let conflicts = self.detect_conflicts(&results);

        // Determine if we need user input
        let needs_user_input = match strategy {
            MergeStrategy::Manual => !conflicts.is_empty(),
            MergeStrategy::Auto => !conflicts.is_empty(),
            _ => false,
        };

        // Build combined summary
        let summary = match strategy {
            MergeStrategy::Concatenate => self.concat_summaries(&results),
            MergeStrategy::FirstWins => self.first_wins_summary(&results),
            MergeStrategy::LastWins => self.last_wins_summary(&results),
            MergeStrategy::Manual | MergeStrategy::Auto => {
                if conflicts.is_empty() {
                    self.concat_summaries(&results)
                } else {
                    self.conflict_summary(&results, &conflicts)
                }
            }
        };

        let all_modified_files: Vec<String> = {
            let mut files: Vec<String> = results
                .iter()
                .flat_map(|r| r.modified_files.clone())
                .collect();
            files.sort();
            files.dedup();
            files
        };

        let all_success = results.iter().all(|r| r.success);

        MergedResult {
            success: all_success,
            summary,
            results,
            all_modified_files,
            conflicts,
            strategy,
            needs_user_input,
        }
    }

    /// Detect conflicts: files modified by more than one agent.
    fn detect_conflicts(&self, results: &[SubAgentResult]) -> Vec<FileConflict> {
        let mut file_agents: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();

        for result in results {
            for file in &result.modified_files {
                file_agents
                    .entry(file.clone())
                    .or_default()
                    .push(result.name.clone());
            }
        }

        file_agents
            .into_iter()
            .filter(|(_, agents)| agents.len() > 1)
            .map(|(file, agents)| FileConflict {
                file,
                agents,
                severity: "high".to_string(), // default to high — real detection needs diff analysis
            })
            .collect()
    }

    /// Concatenate all summaries.
    fn concat_summaries(&self, results: &[SubAgentResult]) -> String {
        let parts: Vec<String> = results
            .iter()
            .map(|r| {
                if r.success {
                    format!("✅ **{}**: {}", r.name, r.summary)
                } else {
                    format!(
                        "❌ **{}**: {} (error: {})",
                        r.name,
                        r.summary,
                        r.error.as_deref().unwrap_or("unknown")
                    )
                }
            })
            .collect();

        let success_count = results.iter().filter(|r| r.success).count();
        let total = results.len();
        let header = format!(
            "## Results ({}/{})\n\n",
            success_count, total
        );

        format!("{}{}", header, parts.join("\n\n"))
    }

    /// Pick first non-error result.
    fn first_wins_summary(&self, results: &[SubAgentResult]) -> String {
        for r in results {
            if r.success {
                return format!("✅ **{}**: {}", r.name, r.summary);
            }
        }
        "❌ All sub-agents failed".to_string()
    }

    /// Pick last non-error result.
    fn last_wins_summary(&self, results: &[SubAgentResult]) -> String {
        for r in results.iter().rev() {
            if r.success {
                return format!("✅ **{}**: {}", r.name, r.summary);
            }
        }
        "❌ All sub-agents failed".to_string()
    }

    /// Build a conflict summary.
    fn conflict_summary(
        &self,
        results: &[SubAgentResult],
        conflicts: &[FileConflict],
    ) -> String {
        let mut summary = String::from("⚠️ **Merge conflicts detected**\n\n");

        summary.push_str("### Conflicted files:\n");
        for conflict in conflicts {
            summary.push_str(&format!(
                "- `{}` (modified by: {})\n",
                conflict.file,
                conflict.agents.join(", ")
            ));
        }

        summary.push_str("\n### All results:\n");
        for r in results {
            if r.success {
                summary.push_str(&format!("- ✅ {}: {}\n", r.name, r.summary));
            } else {
                summary.push_str(&format!("- ❌ {}: {}\n", r.name, r.summary));
            }
        }

        summary.push_str("\n> Manual resolution required. Use `odin merge resolve` to handle conflicts.");
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result(name: &str, success: bool, files: Vec<&str>, summary: &str) -> SubAgentResult {
        SubAgentResult {
            agent_id: Uuid::new_v4(),
            name: name.into(),
            summary: summary.into(),
            output: None,
            modified_files: files.iter().map(|s| s.to_string()).collect(),
            success,
            error: if success { None } else { Some("failed".into()) },
            duration_ms: 100,
        }
    }

    #[test]
    fn test_merge_concatenate_all_success() {
        let resolver = MergeResolver::new();
        let results = vec![
            make_result("agent-a", true, vec!["a.txt"], "Fixed bug A"),
            make_result("agent-b", true, vec!["b.txt"], "Added feature B"),
        ];

        let merged = resolver.merge(results, MergeStrategy::Concatenate);
        assert!(merged.success);
        assert!(merged.summary.contains("agent-a"));
        assert!(merged.summary.contains("agent-b"));
        assert!(!merged.needs_user_input);
        assert_eq!(merged.all_modified_files.len(), 2);
    }

    #[test]
    fn test_merge_detect_conflicts() {
        let resolver = MergeResolver::new();
        let results = vec![
            make_result("agent-a", true, vec!["shared.rs"], "Changed X"),
            make_result("agent-b", true, vec!["shared.rs"], "Changed Y"),
        ];

        let merged = resolver.merge(results, MergeStrategy::Auto);
        assert!(!merged.conflicts.is_empty());
        assert_eq!(merged.conflicts[0].file, "shared.rs");
        assert_eq!(merged.conflicts[0].agents.len(), 2);
        assert!(merged.needs_user_input);
    }

    #[test]
    fn test_merge_partial_failure() {
        let resolver = MergeResolver::new();
        let results = vec![
            make_result("agent-a", true, vec!["a.txt"], "Done"),
            make_result("agent-b", false, vec![], "Failed"),
        ];

        let merged = resolver.merge(results, MergeStrategy::Concatenate);
        assert!(!merged.success);
        assert!(merged.summary.contains("❌"));
        assert!(merged.summary.contains("✅"));
    }

    #[test]
    fn test_merge_first_wins() {
        let resolver = MergeResolver::new();
        let results = vec![
            make_result("first", true, vec!["a.txt"], "First result"),
            make_result("second", true, vec!["b.txt"], "Second result"),
        ];

        let merged = resolver.merge(results, MergeStrategy::FirstWins);
        assert!(merged.summary.contains("First result"));
        assert!(!merged.summary.contains("Second result"));
    }

    #[test]
    fn test_merge_last_wins() {
        let resolver = MergeResolver::new();
        let results = vec![
            make_result("first", true, vec!["a.txt"], "First result"),
            make_result("last", true, vec!["b.txt"], "Last result"),
        ];

        let merged = resolver.merge(results, MergeStrategy::LastWins);
        assert!(merged.summary.contains("Last result"));
        assert!(!merged.summary.contains("First result"));
    }

    #[test]
    fn test_merge_no_conflicts_auto() {
        let resolver = MergeResolver::new();
        let results = vec![
            make_result("a", true, vec!["a.txt"], "Fixed A"),
            make_result("b", true, vec!["b.txt"], "Fixed B"),
        ];

        let merged = resolver.merge(results, MergeStrategy::Auto);
        assert!(merged.conflicts.is_empty());
        assert!(!merged.needs_user_input);
    }
}
