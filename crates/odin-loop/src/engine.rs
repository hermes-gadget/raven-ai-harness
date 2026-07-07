//! The main loop engine — orchestrates the 7-phase agent loop.
//!
//! This is the heart of Raven's innovation: a structured loop that helps
//! smaller models succeed through decomposition, self-checking, retry,
//! and escalation.

use async_trait::async_trait;
use odin_core::error::OdinResult;
use odin_core::traits::{LoopEngine as LoopEngineTrait, LoopState, PhaseResult, Provider};
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
    /// Optional provider for LLM calls (phases use stubs if None)
    provider: Option<Arc<dyn Provider>>,
    /// Optional stronger provider for escalation (used when confidence is low)
    escalation_provider: Option<Arc<dyn Provider>>,
    /// Optional tool registry for dispatching tool calls
    tool_registry: Option<Arc<odin_tools::ToolRegistry>>,
}

impl Engine {
    /// Create a new loop engine with default settings (no LLM provider — stub mode).
    pub fn new() -> Self {
        Self {
            confidence_scorer: ConfidenceScorer::default(),
            decomposer: GoalDecomposer::default(),
            summarizer: StateSummarizer::default(),
            max_iterations: 100,
            provider: None,
            escalation_provider: None,
            tool_registry: None,
        }
    }

    /// Attach a model provider for real LLM calls.
    pub fn with_provider(mut self, provider: Arc<dyn Provider>) -> Self {
        self.provider = Some(provider);
        self
    }

    /// Attach a stronger escalation provider for low-confidence scenarios.
    pub fn with_escalation_provider(mut self, provider: Arc<dyn Provider>) -> Self {
        self.escalation_provider = Some(provider);
        self
    }

    /// Attach a tool registry for dispatching real tool calls.
    pub fn with_tool_registry(mut self, registry: Arc<odin_tools::ToolRegistry>) -> Self {
        self.tool_registry = Some(registry);
        self
    }

    /// Set the maximum iterations.
    pub fn with_max_iterations(mut self, max: u32) -> Self {
        self.max_iterations = max;
        self
    }

    /// Set custom confidence thresholds.
    pub fn with_confidence_thresholds(mut self, low: f64, high: f64) -> Self {
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
                Message::system(
                    "You are an AI agent. Follow the plan, execute carefully, and verify results.",
                ),
                Message::user(format!("Goal: {}", task.goal)),
            ],
            tool_results: vec![],
            current_phase: LoopPhase::Plan,
            iteration: 0,
            retry_count: 0,
            history: vec![],
        };

        if let Some(ref ctx) = task.context {
            state
                .messages
                .push(Message::system(format!("Context: {}", ctx)));
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

        let mut context = PhaseContext {
            confidence_scorer: self.confidence_scorer.clone(),
            decomposer: self.decomposer.clone(),
            summarizer: self.summarizer.clone(),
            plan: Some(plan.clone()),
            provider: self.provider.clone(),
            escalation_provider: self.escalation_provider.clone(),
            tool_registry: self.tool_registry.clone(),
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
                    // If we have an escalation provider, switch and retry
                    if context.escalation_provider.is_some() && context.provider.is_some() {
                        tracing::info!("[LOOP] Switching to escalation provider for retry");
                        // Swap providers: escalation becomes primary for this retry
                        let _ = std::mem::replace(
                            &mut context.provider,
                            context.escalation_provider.clone(),
                        );
                        state.current_phase = LoopPhase::Act;
                        continue;
                    }
                    // No escalation provider available — give up
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
                            if last_confidence.is_high()
                                && let Some(pending) = plan
                                    .sub_tasks
                                    .iter_mut()
                                    .find(|st| st.status == SubTaskStatus::Pending)
                            {
                                pending.status = SubTaskStatus::Completed;
                                pending.result = Some("Completed successfully".into());
                            }

                            // Check if all sub-tasks are done
                            let all_done = plan.sub_tasks.iter().all(|st| {
                                st.status == SubTaskStatus::Completed
                                    || st.status == SubTaskStatus::Failed
                                    || st.status == SubTaskStatus::Skipped
                            });
                            if all_done {
                                break;
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
            provider: self.provider.clone(),
            escalation_provider: self.escalation_provider.clone(),
            tool_registry: self.tool_registry.clone(),
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

        let result = engine.execute_phase(LoopPhase::Plan, &mut state).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().phase, LoopPhase::Plan);
    }

    // ── Mock Provider Tests ─────────────────────────────────────────

    use async_trait::async_trait;
    use odin_core::error::{OdinError, OdinResult};
    use odin_core::traits::{ChatStream, Provider as ProviderTrait};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockProvider {
        responses: Mutex<Vec<ChatResponse>>,
        call_count: AtomicUsize,
    }

    impl MockProvider {
        fn new(responses: Vec<ChatResponse>) -> Self {
            Self {
                responses: Mutex::new(responses),
                call_count: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl ProviderTrait for MockProvider {
        fn name(&self) -> &str {
            "mock"
        }
        async fn list_models(&self) -> OdinResult<Vec<ModelInfo>> {
            Ok(vec![])
        }
        async fn chat(
            &self,
            _model: &str,
            _messages: &[Message],
            _tools: &[ToolSchema],
            _options: &CompletionOptions,
        ) -> OdinResult<ChatResponse> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            let mut responses = self.responses.lock().unwrap();
            if let Some(resp) = responses.pop() {
                Ok(resp)
            } else {
                Ok(ChatResponse {
                    message: Message::assistant("Done."),
                    usage: TokenUsage::default(),
                    finish_reason: Some("stop".into()),
                    model: "mock".into(),
                })
            }
        }
        async fn chat_stream(
            &self,
            _model: &str,
            _messages: &[Message],
            _tools: &[ToolSchema],
            _options: &CompletionOptions,
        ) -> OdinResult<Box<dyn ChatStream>> {
            unimplemented!()
        }
        async fn health_check(&self) -> OdinResult<bool> {
            Ok(true)
        }
    }

    fn mk_resp(text: &str) -> ChatResponse {
        ChatResponse {
            message: Message::assistant(text),
            usage: TokenUsage::default(),
            finish_reason: Some("stop".into()),
            model: "mock".into(),
        }
    }

    fn mk_tc_resp(tool_name: &str, args: &str) -> ChatResponse {
        ChatResponse {
            message: Message {
                role: Role::Assistant,
                content: MessageContent::ToolCalls {
                    content: Some("Using tool...".into()),
                    tool_calls: vec![ToolCall {
                        id: format!("call_{}", uuid::Uuid::new_v4()),
                        call_type: "function".into(),
                        function: FunctionCall {
                            name: tool_name.to_string(),
                            arguments: args.to_string(),
                        },
                    }],
                },
                name: None,
                tool_call_id: None,
            },
            usage: TokenUsage::default(),
            finish_reason: Some("tool_calls".into()),
            model: "mock".into(),
        }
    }

    #[tokio::test]
    async fn test_full_cycle_with_mock_provider() {
        let mock = Arc::new(MockProvider::new(vec![mk_resp(
            "I'll help you write a hello world program.",
        )]));
        let engine = Engine::new()
            .with_provider(mock.clone())
            .with_max_iterations(10);
        let task = make_task("Write a hello world program");
        let result = engine.execute_task(&task).await.unwrap();
        assert!(result.iterations > 0);
        assert!(result.iterations <= 10);
        assert!(!result.summary.is_empty());
    }

    #[tokio::test]
    async fn test_engine_with_tool_call_and_execution() {
        let mock = Arc::new(MockProvider::new(vec![
            mk_tc_resp("shell", r#"{"command":"echo hello"}"#),
            mk_resp("Command ran successfully. Task done."),
        ]));

        let tool_registry = Arc::new(odin_tools::ToolRegistry::new());
        let _ = tool_registry.register(Box::new(odin_tools::builtins::shell::Shell::new()));

        let engine = Engine::new()
            .with_provider(mock.clone())
            .with_tool_registry(tool_registry)
            .with_max_iterations(10);

        let task = make_task("Run a shell command");
        let result = engine.execute_task(&task).await.unwrap();
        assert!(result.tool_calls >= 1);
    }

    #[tokio::test]
    async fn test_provider_error_graceful_degradation() {
        struct FailingProvider;
        #[async_trait]
        impl ProviderTrait for FailingProvider {
            fn name(&self) -> &str {
                "failing"
            }
            async fn list_models(&self) -> OdinResult<Vec<ModelInfo>> {
                Ok(vec![])
            }
            async fn chat(
                &self,
                _model: &str,
                _messages: &[Message],
                _tools: &[ToolSchema],
                _options: &CompletionOptions,
            ) -> OdinResult<ChatResponse> {
                Err(OdinError::Provider {
                    provider: "failing".into(),
                    message: "Simulated failure".into(),
                    source: None,
                })
            }
            async fn chat_stream(
                &self,
                _model: &str,
                _messages: &[Message],
                _tools: &[ToolSchema],
                _options: &CompletionOptions,
            ) -> OdinResult<Box<dyn ChatStream>> {
                unimplemented!()
            }
            async fn health_check(&self) -> OdinResult<bool> {
                Ok(false)
            }
        }

        let engine = Engine::new()
            .with_provider(Arc::new(FailingProvider))
            .with_max_iterations(5);
        let task = make_task("Do something");
        let result = engine.execute_task(&task).await.unwrap();
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn test_escalation_to_stronger_provider() {
        struct WeakProvider;
        #[async_trait]
        impl ProviderTrait for WeakProvider {
            fn name(&self) -> &str {
                "weak"
            }
            async fn list_models(&self) -> OdinResult<Vec<ModelInfo>> {
                Ok(vec![])
            }
            async fn chat(
                &self,
                _model: &str,
                _messages: &[Message],
                _tools: &[ToolSchema],
                _options: &CompletionOptions,
            ) -> OdinResult<ChatResponse> {
                Ok(ChatResponse {
                    message: Message::assistant("ok"),
                    usage: TokenUsage::default(),
                    finish_reason: Some("stop".into()),
                    model: "weak".into(),
                })
            }
            async fn chat_stream(
                &self,
                _model: &str,
                _messages: &[Message],
                _tools: &[ToolSchema],
                _options: &CompletionOptions,
            ) -> OdinResult<Box<dyn ChatStream>> {
                unimplemented!()
            }
            async fn health_check(&self) -> OdinResult<bool> {
                Ok(true)
            }
        }

        struct StrongProvider {
            call_count: AtomicUsize,
        }
        #[async_trait]
        impl ProviderTrait for StrongProvider {
            fn name(&self) -> &str {
                "strong"
            }
            async fn list_models(&self) -> OdinResult<Vec<ModelInfo>> {
                Ok(vec![])
            }
            async fn chat(
                &self,
                _model: &str,
                _messages: &[Message],
                _tools: &[ToolSchema],
                _options: &CompletionOptions,
            ) -> OdinResult<ChatResponse> {
                self.call_count.fetch_add(1, Ordering::SeqCst);
                Ok(ChatResponse {
                    message: Message::assistant(
                        "I have completed the task successfully. \
                         The implementation is correct and complete. \
                         All requirements have been met.",
                    ),
                    usage: TokenUsage::default(),
                    finish_reason: Some("stop".into()),
                    model: "strong".into(),
                })
            }
            async fn chat_stream(
                &self,
                _model: &str,
                _messages: &[Message],
                _tools: &[ToolSchema],
                _options: &CompletionOptions,
            ) -> OdinResult<Box<dyn ChatStream>> {
                unimplemented!()
            }
            async fn health_check(&self) -> OdinResult<bool> {
                Ok(true)
            }
        }

        let strong = Arc::new(StrongProvider {
            call_count: AtomicUsize::new(0),
        });

        let engine = Engine::new()
            .with_provider(Arc::new(WeakProvider))
            .with_escalation_provider(strong.clone())
            .with_max_iterations(15);

        let task = make_task("Complete a complex task");
        let result = engine.execute_task(&task).await.unwrap();
        assert!(result.iterations > 0);
        assert!(
            strong.call_count.load(Ordering::SeqCst) >= 1,
            "Strong model was never called"
        );
    }

    #[tokio::test]
    async fn test_retry_on_low_confidence() {
        let mock = Arc::new(MockProvider::new(vec![
            mk_resp("k"), // Very short = low confidence
            mk_resp("I have completed the given task successfully."),
        ]));

        let engine = Engine::new()
            .with_provider(mock.clone())
            .with_max_iterations(10);

        let task = make_task("Write some code");
        let result = engine.execute_task(&task).await.unwrap();
        // Should have retried at least once (more than 1 iteration)
        assert!(result.iterations > 1, "Expected retry");
    }

    #[tokio::test]
    async fn test_max_iterations_bound() {
        let mock = Arc::new(MockProvider::new(vec![
            mk_resp("Working..."),
            mk_resp("Still working..."),
            mk_resp("Almost..."),
        ]));

        let engine = Engine::new()
            .with_provider(mock.clone())
            .with_max_iterations(3);

        let mut task = make_task("Long task");
        task.max_iterations = 3;
        let result = engine.execute_task(&task).await.unwrap();
        // The loop increments, checks > max, then breaks — so iterations may be max+1
        assert!(result.iterations <= 4, "iterations={}", result.iterations);
    }

    #[tokio::test]
    async fn test_all_phases_execute_individually() {
        let engine = Engine::new().with_max_iterations(10);
        let task = make_task("Test all phases");
        let mut state = LoopState {
            task: task.clone(),
            messages: vec![Message::user("test")],
            tool_results: vec![],
            current_phase: LoopPhase::Plan,
            iteration: 0,
            retry_count: 0,
            history: vec![],
        };

        let phases = [
            LoopPhase::Plan,
            LoopPhase::Act,
            LoopPhase::Inspect,
            LoopPhase::Critique,
            LoopPhase::Revise,
            LoopPhase::Verify,
            LoopPhase::Decide,
        ];

        for phase in &phases {
            let result = engine.execute_phase(*phase, &mut state).await;
            assert!(result.is_ok(), "Phase {:?} failed", phase);
            assert_eq!(result.unwrap().phase, *phase);
        }
        assert_eq!(state.history.len(), 7);
    }

    #[tokio::test]
    async fn test_confidence_with_empty_response() {
        let mock = Arc::new(MockProvider::new(vec![ChatResponse {
            message: Message::assistant(""),
            usage: TokenUsage::default(),
            finish_reason: Some("stop".into()),
            model: "reasoning".into(),
        }]));

        let engine = Engine::new()
            .with_provider(mock.clone())
            .with_max_iterations(5);

        let task = make_task("Reason about something");
        let result = engine.execute_task(&task).await.unwrap();
        assert!(result.iterations > 0);
        // Should not panic on empty content
    }

    #[tokio::test]
    async fn test_looped_vs_baseline_comparison() {
        let mock_looped = Arc::new(MockProvider::new(vec![mk_resp(
            "Analysis complete. Results show significant improvement.",
        )]));
        let mock_baseline = Arc::new(MockProvider::new(vec![mk_resp("Analysis done.")]));

        let loop_engine = Engine::new()
            .with_provider(mock_looped.clone())
            .with_max_iterations(10);

        let task = make_task("Analyze performance");
        let looped = loop_engine.execute_task(&task).await.unwrap();

        let baseline = odin_baseline::BaselineAgent::new(mock_baseline.clone(), vec![], 10);
        let baseline_result = baseline.execute_task(&task).await.unwrap();

        assert!(looped.iterations > 0);
        assert!(baseline_result.iterations > 0);
        // Looped engine decomposes — baseline doesn't
        assert!(!looped.sub_tasks.is_empty());
        assert!(baseline_result.sub_tasks.is_empty());
    }
}
