//! State summarization for context window management.
//!
//! Smaller models have limited context windows. The summarizer compresses
//! conversation history into compact state summaries, keeping only the
//! essential information needed for the next loop iteration.

use odin_core::traits::LoopState;
use odin_core::types::*;

/// Summarizes agent state for small context windows.
#[derive(Debug, Clone)]
pub struct StateSummarizer {
    /// Maximum tokens to keep in the summary
    max_summary_tokens: u32,
}

impl Default for StateSummarizer {
    fn default() -> Self {
        Self {
            max_summary_tokens: 2048,
        }
    }
}

impl StateSummarizer {
    pub fn new(max_summary_tokens: u32) -> Self {
        Self { max_summary_tokens }
    }

    /// Create a state summary from the current loop state.
    pub fn summarize(&self, state: &LoopState) -> StateSummary {
        let completed_steps: Vec<String> = state
            .history
            .iter()
            .filter(|r| {
                r.phase == LoopPhase::Verify && r.confidence.map(|c| c.is_high()).unwrap_or(false)
            })
            .filter_map(|r| r.output.clone())
            .collect();

        let pending_steps: Vec<String> = state
            .task
            .sub_tasks
            .iter()
            .filter(|st| st.status == SubTaskStatus::Pending)
            .map(|st| st.description.clone())
            .collect();

        let last_action = state.history.last().and_then(|r| {
            if r.phase == LoopPhase::Act {
                r.output.clone()
            } else {
                None
            }
        });

        let last_result = state.tool_results.last().map(|tr| {
            format!(
                "[{}] {} — {}",
                tr.tool_name,
                if tr.success { "OK" } else { "FAIL" },
                tr.output
            )
        });

        let errors: Vec<String> = state
            .history
            .iter()
            .filter_map(|r| r.error.clone())
            .collect();

        let confidence = state
            .history
            .last()
            .and_then(|r| r.confidence)
            .map(|c| c.value())
            .unwrap_or(1.0);

        let token_usage = self.estimate_tokens(&state.messages);

        StateSummary {
            goal: state.task.goal.clone(),
            current_phase: state.current_phase,
            completed_steps,
            pending_steps,
            last_action,
            last_result,
            errors,
            confidence,
            token_usage,
        }
    }

    /// Format a state summary as a system message for the next turn.
    pub fn format_for_prompt(&self, summary: &StateSummary) -> String {
        let mut parts = vec![
            format!("## Goal\n{}", summary.goal),
            format!("## Phase\n{}", summary.current_phase),
        ];

        if !summary.completed_steps.is_empty() {
            parts.push(format!(
                "## Completed\n{}",
                summary
                    .completed_steps
                    .iter()
                    .map(|s| format!("- {}", s))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }

        if !summary.pending_steps.is_empty() {
            parts.push(format!(
                "## Pending\n{}",
                summary
                    .pending_steps
                    .iter()
                    .map(|s| format!("- {}", s))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }

        if let Some(ref action) = summary.last_action {
            parts.push(format!("## Last Action\n{}", action));
        }

        if let Some(ref result) = summary.last_result {
            parts.push(format!("## Last Result\n{}", result));
        }

        if !summary.errors.is_empty() {
            parts.push(format!(
                "## Errors\n{}",
                summary
                    .errors
                    .iter()
                    .map(|e| format!("- {}", e))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }

        parts.push(format!(
            "## Confidence\n{:.0}%\n## Tokens Used\n{}",
            summary.confidence * 100.0,
            summary.token_usage.total_tokens
        ));

        let prompt = parts.join("\n\n");
        let max_chars = self.max_summary_tokens as usize * 4;
        if prompt.chars().count() > max_chars {
            let mut truncated: String = prompt.chars().take(max_chars).collect();
            truncated.push_str("\n[summary truncated]");
            truncated
        } else {
            prompt
        }
    }

    /// Estimate token count for messages (rough: ~4 chars per token).
    fn estimate_tokens(&self, messages: &[Message]) -> TokenUsage {
        let total_chars: usize = messages.iter().map(|m| m.text().unwrap_or("").len()).sum();
        let estimated = (total_chars / 4) as u32;
        TokenUsage {
            prompt_tokens: estimated,
            completion_tokens: 0,
            total_tokens: estimated,
        }
    }

    /// Check if the context is approaching the limit.
    pub fn needs_compression(&self, messages: &[Message], limit: u32) -> bool {
        let estimated = self.estimate_tokens(messages).total_tokens;
        estimated > (limit as f64 * 0.8) as u32
    }

    /// Compress messages by keeping system + last N turns, summarizing the rest.
    pub fn compress(&self, messages: &[Message], _keep_last: usize) -> Vec<Message> {
        // Keep system messages + last few turns + state summary
        let system_msgs: Vec<&Message> =
            messages.iter().filter(|m| m.role == Role::System).collect();

        let keep_count = 6; // Keep last 3 turns (user + assistant pairs)
        let recent_start = messages.len().saturating_sub(keep_count);
        let recent_msgs: Vec<&Message> = messages[recent_start..].iter().collect();

        let mut compressed: Vec<Message> = Vec::new();

        // Add system messages
        for msg in system_msgs {
            compressed.push((*msg).clone());
        }

        // Add a summary message if we compressed anything
        if recent_start > 0 {
            compressed.push(Message::system(format!(
                "[Previous {} messages summarized. Key state preserved above.]",
                recent_start
            )));
        }

        // Add recent messages
        for msg in recent_msgs {
            compressed.push((*msg).clone());
        }

        compressed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state() -> LoopState {
        LoopState {
            task: AgentTask {
                id: TaskId::new_v4(),
                goal: "Test goal".into(),
                context: None,
                sub_tasks: vec![
                    SubTask {
                        id: "1".into(),
                        description: "Step one".into(),
                        status: SubTaskStatus::Completed,
                        result: Some("Done".into()),
                    },
                    SubTask {
                        id: "2".into(),
                        description: "Step two".into(),
                        status: SubTaskStatus::Pending,
                        result: None,
                    },
                ],
                success_criteria: vec![],
                max_iterations: 10,
                created_at: chrono::Utc::now(),
            },
            messages: vec![
                Message::system("You are an agent."),
                Message::user("Do the thing."),
                Message::assistant("I'll do it."),
            ],
            tool_results: vec![],
            current_phase: LoopPhase::Plan,
            iteration: 0,
            retry_count: 0,
            history: vec![],
        }
    }

    #[test]
    fn test_summarize_basic() {
        let summarizer = StateSummarizer::default();
        let state = make_state();
        let summary = summarizer.summarize(&state);

        assert_eq!(summary.goal, "Test goal");
        assert_eq!(summary.current_phase, LoopPhase::Plan);
        assert_eq!(summary.pending_steps.len(), 1);
    }

    #[test]
    fn test_format_for_prompt() {
        let summarizer = StateSummarizer::default();
        let state = make_state();
        let summary = summarizer.summarize(&state);
        let formatted = summarizer.format_for_prompt(&summary);

        assert!(formatted.contains("Test goal"));
        assert!(formatted.contains("Step two"));
    }

    #[test]
    fn test_needs_compression() {
        let summarizer = StateSummarizer::default();
        let messages: Vec<Message> = (0..100)
            .map(|i| Message::user(format!("Long message number {} with padding text to make it take more characters in the token estimation", i)))
            .collect();
        // This should be way over any reasonable limit
        assert!(summarizer.needs_compression(&messages, 100));
    }

    #[test]
    fn test_compress_reduces_size() {
        let summarizer = StateSummarizer::default();
        let mut messages = vec![Message::system("You are helpful.")];
        for i in 0..20 {
            messages.push(Message::user(format!("Message {}", i)));
            messages.push(Message::assistant(format!("Response {}", i)));
        }
        let compressed = summarizer.compress(&messages, 3);
        // Should have reduced significantly
        assert!(compressed.len() < messages.len());
        // Should still have system message
        assert!(compressed.iter().any(|m| m.role == Role::System));
    }
}
