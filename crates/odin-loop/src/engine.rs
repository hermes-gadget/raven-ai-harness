//! The main loop engine — orchestrates the 7-phase agent loop.
//!
//! This is the heart of Raven's innovation: a structured loop that helps
//! smaller models succeed through decomposition, self-checking, retry,
//! and escalation.

use async_trait::async_trait;
use odin_core::error::OdinResult;
use odin_core::traits::{LoopEngine as LoopEngineTrait, LoopState, PhaseResult};
use odin_core::types::*;
use std::sync::Arc;

use crate::confidence::ConfidenceScorer;
use crate::decomposer::GoalDecomposer;
use crate::phases::{
    ActPhase, CritiquePhase, DecidePhase, InspectPhase, Phase, PhaseContext, PlanPhase,
    RevisePhase, VerifyPhase,
};
use crate::summarizer::StateSummarizer;

/// The main loop engine implementation.
///
/// Orchestrates the full 7-phase loop:
///   PLAN → ACT → INSPECT → CRITIQUE → REVISE → VERIFY → DECIDE
pub struct Engine {
    confidence_scorer: ConfidenceScorer,
    decomposer: GoalDecomposer,
    summarizer: StateSummarizer,
    /// Maximum total iterations across all phases
    max_iterations: u32,
}

impl Engine {
    /// Create a new loop engine with default settings.
    pub fn new() -> Self {
        Self {
            confidence_scorer: ConfidenceScorer::default(),
            decomposer: GoalDecomposer::default(),
            summarizer: StateSummarizer::default(),
            max_iterations: 100,
        }
    }

    /// Set the maximum iterations.
    pub fn with_max_iterations(mut self, max: u32) -> Self {
        self.max_iterations = max;
        self
    }

    /// Set custom confidence thresholds.
    pub fn with_confidence_thresholds(
        mut self,
        low: f64,
        high: f64,
    ) -> Self {
        self.confidence_scorer.low_threshold = low;
        self.confidence_scorer.high_threshold = high;
        self
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LoopEngineTrait for Engine {
    async fn execute_task(&self, task: &AgentTask) -> OdinResult<TaskResult> {
        let start = std::time::Instant::now();

        tracing::info!(
            "[LOOP] Starting task: {} (max iterations: {})",
            task.goal,
            task.max_iterations
        );

        // Initialize loop state
        let mut state = LoopState {
            task: task.clone(),
            messages: vec![
                Message::system("You are an AI agent. Follow the plan, execute carefully, and verify results."),
                Message::user(format!("Goal: {}", task.goal)),
            ],
            tool_results: vec![],
            current_phase: LoopPhase::Plan,
            iteration: 0,
            retry_count: 0,
            history: vec![],
        };

        if let Some(ref ctx) = task.context {
            state.messages.push(Message::system(format!("Context: {}", ctx)));
        }

        // Decompose the goal
        let mut plan = self.decomposer.decompose_heuristic(&task.goal);

        // Create phase instances
        let plan_phase = PlanPhase::new(self.decomposer.clone());
        let act_phase = ActPhase;
        let inspect_phase = InspectPhase::new(self.summarizer.clone());
        let critique_phase = CritiquePhase::new(self.confidence_scorer.clone());
        let revise_phase = RevisePhase;
        let verify_phase = VerifyPhase::new(self.confidence_scorer.clone());
        let decide_phase = DecidePhase;

        let context = PhaseContext {
            confidence_scorer: self.confidence_scorer.clone(),
            decomposer: self.decomposer.clone(),
            summarizer: self.summarizer.clone(),
            plan: Some(plan.clone()),
        };

        let mut total_tool_calls = 0u32;
        let mut last_confidence = ConfidenceScore::new(1.0);

        // Main loop
        loop {
            state.iteration += 1;

            if state.iteration > task.max_iterations {
                tracing::warn!("[LOOP] Max iterations reached");
                break;
            }

            // ── PLAN ── (only on first iteration — decomposition is done once)
            if state.iteration == 1 {
                let result = plan_phase.execute(&mut state, &context).await?;
                last_confidence = result.confidence;
                if result.decision == LoopDecision::Stop {
                    break;
                }
                // After planning, go straight to ACT
                state.current_phase = LoopPhase::Act;
                continue;
            }

            // ── ACT ──
            let result = act_phase.execute(&mut state, &context).await?;
            if !result.tool_results.is_empty() {
                total_tool_calls += result.tool_results.len() as u32;
            }

            // ── INSPECT ──
            inspect_phase.execute(&mut state, &context).await?;

            // ── CRITIQUE ──
            let critique = critique_phase.execute(&mut state, &context).await?;
            last_confidence = critique.confidence;

            match critique.decision {
                LoopDecision::Escalate => {
                    tracing::warn!(
                        "[LOOP] Escalating — confidence too low ({:.0}%)",
                        last_confidence.value() * 100.0
                    );
                    // In production: switch to escalation model
                    break;
                }
                LoopDecision::Retry => {
                    // ── REVISE ──
                    let revise = revise_phase.execute(&mut state, &context).await?;
                    if revise.decision == LoopDecision::Escalate {
                        break;
                    }
                    // Go back to ACT with revised approach
                    state.current_phase = LoopPhase::Act;
                    continue;
                }
                LoopDecision::Stop => break,
                LoopDecision::Continue => {
                    // ── VERIFY ──
                    let verify = verify_phase.execute(&mut state, &context).await?;
                    last_confidence = verify.confidence;

                    // ── DECIDE ──
                    let decide = decide_phase.execute(&mut state, &context).await?;
                    match decide.decision {
                        LoopDecision::Stop => break,
                        LoopDecision::Escalate => break,
                        LoopDecision::Retry => {
                            state.current_phase = LoopPhase::Act;
                            continue;
                        }
                        LoopDecision::Continue => {
                            // Mark current sub-task as complete if confidence is high
                            if last_confidence.is_high() {
                                if let Some(pending) = plan
                                    .sub_tasks
                                    .iter_mut()
                                    .find(|st| st.status == SubTaskStatus::Pending)
                                {
                                    pending.status = SubTaskStatus::Completed;
                                    pending.result = Some("Completed successfully".into());
                                }
                            }

                            state.current_phase = LoopPhase::Act;
                            continue;
                        }
                    }
                }
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        let success = last_confidence.is_high();

        let summary = format!(
            "Task {} after {} iterations, {} tool calls. Confidence: {:.0}%",
            if success { "completed" } else { "stopped" },
            state.iteration,
            total_tool_calls,
            last_confidence.value() * 100.0
        );

        tracing::info!("[LOOP] {}", summary);

        Ok(TaskResult {
            task_id: task.id,
            success,
            summary,
            iterations: state.iteration,
            tool_calls: total_tool_calls,
            duration_ms,
            sub_tasks: plan.sub_tasks,
            confidence: last_confidence.value(),
            error: if success {
                None
            } else {
                Some("Did not meet confidence threshold".into())
            },
        })
    }

    async fn execute_phase(
        &self,
        phase: LoopPhase,
        state: &mut LoopState,
    ) -> OdinResult<PhaseResult> {
        let context = PhaseContext {
            confidence_scorer: self.confidence_scorer.clone(),
            decomposer: self.decomposer.clone(),
            summarizer: self.summarizer.clone(),
            plan: None,
        };

        match phase {
            LoopPhase::Plan => {
                PlanPhase::new(self.decomposer.clone())
                    .execute(state, &context)
                    .await
            }
            LoopPhase::Act => ActPhase.execute(state, &context).await,
            LoopPhase::Inspect => {
                InspectPhase::new(self.summarizer.clone())
                    .execute(state, &context)
                    .await
            }
            LoopPhase::Critique => {
                CritiquePhase::new(self.confidence_scorer.clone())
                    .execute(state, &context)
                    .await
            }
            LoopPhase::Revise => RevisePhase.execute(state, &context).await,
            LoopPhase::Verify => {
                VerifyPhase::new(self.confidence_scorer.clone())
                    .execute(state, &context)
                    .await
            }
            LoopPhase::Decide => DecidePhase.execute(state, &context).await,
        }
    }

    fn state_summary(&self) -> StateSummary {
        StateSummary {
            goal: String::new(),
            current_phase: LoopPhase::Decide,
            completed_steps: vec![],
            pending_steps: vec![],
            last_action: None,
            last_result: None,
            errors: vec![],
            confidence: 0.0,
            token_usage: TokenUsage::default(),
        }
    }

    fn confidence(&self) -> ConfidenceScore {
        ConfidenceScore::new(0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(goal: &str) -> AgentTask {
        AgentTask {
            id: TaskId::new_v4(),
            goal: goal.to_string(),
            context: None,
            sub_tasks: vec![],
            success_criteria: vec![],
            max_iterations: 10,
            created_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_engine_runs_basic_task() {
        let engine = Engine::new();
        let task = make_task("Create a hello world program");
        let result = engine.execute_task(&task).await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.task_id, task.id);
        assert!(result.iterations > 0);
        // With heuristic decomposition and no actual tools, it should complete
        assert!(result.iterations <= task.max_iterations);
    }

    #[tokio::test]
    async fn test_engine_tracks_iterations() {
        let engine = Engine::new();
        let mut task = make_task("Test iteration tracking");
        task.max_iterations = 5;
        let result = engine.execute_task(&task).await.unwrap();

        assert!(result.iterations <= 5);
    }

    #[tokio::test]
    async fn test_engine_completes_simple_task() {
        let engine = Engine::new();
        let task = make_task("Write a test");
        let result = engine.execute_task(&task).await.unwrap();

        // Should complete or stop within iterations
        assert!(result.iterations > 0);
        assert!(!result.summary.is_empty());
    }

    #[tokio::test]
    async fn test_engine_sub_tasks_populated() {
        let engine = Engine::new();
        let task = make_task("Fix a bug in the login system");
        let result = engine.execute_task(&task).await.unwrap();

        // Sub-tasks should be populated by the decomposer
        assert!(!result.sub_tasks.is_empty());
    }

    #[tokio::test]
    async fn test_execute_single_phase() {
        let engine = Engine::new();
        let task = make_task("Test phase execution");
        let mut state = LoopState {
            task: task.clone(),
            messages: vec![Message::user("test")],
            tool_results: vec![],
            current_phase: LoopPhase::Plan,
            iteration: 0,
            retry_count: 0,
            history: vec![],
        };

        let result = engine
            .execute_phase(LoopPhase::Plan, &mut state)
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().phase, LoopPhase::Plan);
    }
}
