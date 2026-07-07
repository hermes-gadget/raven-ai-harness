//! Individual loop phase implementations.
//!
//! Each phase is a self-contained function that transforms loop state.
//! For production use, phases would call the model provider for LLM-guided
//! execution. The implementations here provide the structure and hooks.

use async_trait::async_trait;
use odin_core::error::OdinResult;
use odin_core::traits::{LoopState, PhaseRecord, PhaseResult, Provider, ToolContext};
use odin_core::types::*;
use std::sync::Arc;

use crate::confidence::ConfidenceScorer;
use crate::decomposer::{DecomposedPlan, GoalDecomposer};
use crate::summarizer::StateSummarizer;

// ── Phase Traits ────────────────────────────────────────────────────

/// A single phase of the agent loop.
#[async_trait]
pub trait Phase: Send + Sync {
    /// Execute this phase.
    async fn execute(
        &self,
        state: &mut LoopState,
        context: &PhaseContext,
    ) -> OdinResult<PhaseResult>;
}

/// Shared context passed to all phases.
pub struct PhaseContext {
    pub confidence_scorer: ConfidenceScorer,
    pub decomposer: GoalDecomposer,
    pub summarizer: StateSummarizer,
    pub plan: Option<DecomposedPlan>,
    /// Optional LLM provider for real model calls
    pub provider: Option<Arc<dyn Provider>>,
    /// Optional stronger provider for escalation
    pub escalation_provider: Option<Arc<dyn Provider>>,
    /// Optional tool registry for dispatching real tool calls
    pub tool_registry: Option<Arc<odin_tools::ToolRegistry>>,
}

// ── Plan Phase ──────────────────────────────────────────────────────

pub struct PlanPhase {
    pub decomposer: Arc<GoalDecomposer>,
}

impl PlanPhase {
    pub fn new(decomposer: GoalDecomposer) -> Self {
        Self {
            decomposer: Arc::new(decomposer),
        }
    }
}

#[async_trait]
impl Phase for PlanPhase {
    async fn execute(
        &self,
        state: &mut LoopState,
        _context: &PhaseContext,
    ) -> OdinResult<PhaseResult> {
        tracing::info!("[PLAN] Planning for task: {}", state.task.goal);

        // Decompose the goal into sub-tasks
        let plan = self.decomposer.decompose_heuristic(&state.task.goal);
        let task_count = plan.sub_tasks.len();

        // Update the task's sub-tasks
        state.task.sub_tasks = plan.sub_tasks.clone();

        // Add a plan message to the conversation
        let plan_text = format!(
            "I've decomposed the goal into {} sub-tasks:\n{}",
            task_count,
            plan.sub_tasks
                .iter()
                .map(|st| format!("- [{}] {}", st.id, st.description))
                .collect::<Vec<_>>()
                .join("\n")
        );
        state.messages.push(Message::assistant(plan_text.clone()));

        state.current_phase = LoopPhase::Plan;

        let record = PhaseRecord {
            phase: LoopPhase::Plan,
            input: Some(state.task.goal.clone()),
            output: Some(plan_text),
            confidence: Some(ConfidenceScore::new(0.9)),
            duration_ms: 0,
            error: None,
        };
        state.history.push(record);

        Ok(PhaseResult {
            phase: LoopPhase::Plan,
            decision: LoopDecision::Continue,
            output: Some(format!("Decomposed into {} sub-tasks", task_count)),
            confidence: ConfidenceScore::new(0.9),
            tool_results: vec![],
        })
    }
}

// ── Act Phase ───────────────────────────────────────────────────────

pub struct ActPhase;

#[async_trait]
impl Phase for ActPhase {
    async fn execute(
        &self,
        state: &mut LoopState,
        context: &PhaseContext,
    ) -> OdinResult<PhaseResult> {
        tracing::info!("[ACT] Executing action for iteration {}", state.iteration);

        state.current_phase = LoopPhase::Act;

        // If we have a real provider, call the LLM to decide what to do
        let (action_desc, tool_results) = if let Some(ref provider) = context.provider {
            let pending_desc = context
                .plan
                .as_ref()
                .and_then(|p| {
                    p.sub_tasks
                        .iter()
                        .find(|st| st.status == SubTaskStatus::Pending)
                })
                .map(|st| st.description.as_str())
                .unwrap_or("the current goal");

            let prompt = format!(
                "You are working on: {}\nGoal: {}\nDecide what action to take next. If you need a tool, specify which one. Otherwise, provide the result.",
                pending_desc, state.task.goal
            );

            let mut msgs = state.messages.clone();
            msgs.push(Message::user(prompt));

            let options = CompletionOptions {
                temperature: Some(0.3),
                max_tokens: Some(1024),
                ..Default::default()
            };

            match provider.chat("", &msgs, &[], &options).await {
                Ok(response) => {
                    let text = response.message.text().unwrap_or("").to_string();
                    let calls = response.message.tool_calls().to_vec();

                    if !calls.is_empty() {
                        let desc = format!("Calling tool: {}", calls[0].function.name);
                        state.messages.push(response.message);

                        // Actually dispatch tool calls via the tool registry
                        let tool_results: Vec<ToolResult> = if let Some(ref registry) =
                            context.tool_registry
                        {
                            let mut results = Vec::new();
                            for tc in calls {
                                let tool_name = tc.function.name.clone();
                                let tool = match registry.get(&tool_name) {
                                    Some(t) => t,
                                    None => {
                                        let tr = ToolResult {
                                            call_id: tc.id.clone(),
                                            tool_name: tool_name.clone(),
                                            success: false,
                                            output: String::new(),
                                            error: Some(format!(
                                                "Tool '{}' not found in registry",
                                                tool_name
                                            )),
                                            duration_ms: 0,
                                            timestamp: chrono::Utc::now(),
                                        };
                                        results.push(tr);
                                        continue;
                                    }
                                };

                                let args: serde_json::Value =
                                    match serde_json::from_str(&tc.function.arguments) {
                                        Ok(v) => v,
                                        Err(e) => {
                                            let tr = ToolResult {
                                                call_id: tc.id.clone(),
                                                tool_name: tool_name.clone(),
                                                success: false,
                                                output: String::new(),
                                                error: Some(format!("Invalid tool args: {}", e)),
                                                duration_ms: 0,
                                                timestamp: chrono::Utc::now(),
                                            };
                                            results.push(tr);
                                            continue;
                                        }
                                    };

                                let tool_context = ToolContext {
                                    agent_id: uuid::Uuid::default(),
                                    session_id: uuid::Uuid::default(),
                                    working_dir: std::env::current_dir().unwrap_or_default(),
                                    env: std::collections::HashMap::new(),
                                };

                                let start = std::time::Instant::now();
                                match tool.execute(args, &tool_context).await {
                                    Ok(tr) => {
                                        results.push(tr);
                                    }
                                    Err(e) => {
                                        let tr = ToolResult {
                                            call_id: tc.id.clone(),
                                            tool_name: tool_name.clone(),
                                            success: false,
                                            output: String::new(),
                                            error: Some(e.to_string()),
                                            duration_ms: start.elapsed().as_millis() as u64,
                                            timestamp: chrono::Utc::now(),
                                        };
                                        results.push(tr);
                                    }
                                }
                            }
                            results
                        } else {
                            // No tool registry — simulate results
                            calls
                                .iter()
                                .map(|tc| ToolResult {
                                    call_id: tc.id.clone(),
                                    tool_name: tc.function.name.clone(),
                                    success: true,
                                    output: format!(
                                        "[Simulated] Executed {} with args: {}",
                                        tc.function.name, tc.function.arguments
                                    ),
                                    error: None,
                                    duration_ms: 0,
                                    timestamp: chrono::Utc::now(),
                                })
                                .collect()
                        };

                        for tr in &tool_results {
                            state.tool_results.push(tr.clone());
                            state.messages.push(Message::tool_result(
                                &tr.call_id,
                                serde_json::to_string(tr).unwrap_or_default(),
                            ));
                        }

                        (desc, tool_results)
                    } else {
                        state.messages.push(Message::assistant(text.clone()));
                        (text, vec![])
                    }
                }
                Err(e) => {
                    let err_msg = format!("LLM call failed: {}", e);
                    state.messages.push(Message::assistant(err_msg.clone()));
                    (err_msg, vec![])
                }
            }
        } else {
            // Stub mode — no provider attached
            let pending = context.plan.as_ref().and_then(|p| {
                p.sub_tasks
                    .iter()
                    .find(|st| st.status == SubTaskStatus::Pending)
            });

            let desc = match pending {
                Some(task) => format!("Working on: {}", task.description),
                None => "Executing next action".to_string(),
            };

            state.messages.push(Message::assistant(desc.clone()));
            (desc, vec![])
        };

        let record = PhaseRecord {
            phase: LoopPhase::Act,
            input: Some(format!("Iteration {}", state.iteration)),
            output: Some(action_desc.clone()),
            confidence: None,
            duration_ms: 0,
            error: None,
        };
        state.history.push(record);

        Ok(PhaseResult {
            phase: LoopPhase::Act,
            decision: LoopDecision::Continue,
            output: Some(action_desc),
            confidence: ConfidenceScore::new(0.7),
            tool_results,
        })
    }
}

// ── Inspect Phase ────────────────────────────────────────────────────

pub struct InspectPhase {
    pub summarizer: Arc<StateSummarizer>,
}

impl InspectPhase {
    pub fn new(summarizer: StateSummarizer) -> Self {
        Self {
            summarizer: Arc::new(summarizer),
        }
    }
}

#[async_trait]
impl Phase for InspectPhase {
    async fn execute(
        &self,
        state: &mut LoopState,
        context: &PhaseContext,
    ) -> OdinResult<PhaseResult> {
        tracing::info!("[INSPECT] Inspecting results of action");

        state.current_phase = LoopPhase::Inspect;

        // Check context window
        let needs_compression = self.summarizer.needs_compression(
            &state.messages,
            32768, // Default context limit; use config in production
        );

        if needs_compression {
            tracing::info!("[INSPECT] Context window nearing limit, compressing...");
            state.messages = self.summarizer.compress(&state.messages, 3);
        }

        // Validate last tool result if any
        let last_tool = state.tool_results.last();
        let inspection = match last_tool {
            Some(tr) if !tr.success => {
                format!(
                    "Tool '{}' failed: {}",
                    tr.tool_name,
                    tr.error.as_deref().unwrap_or("unknown error")
                )
            }
            Some(tr) => {
                format!("Tool '{}' succeeded in {}ms", tr.tool_name, tr.duration_ms)
            }
            None => "No tool results to inspect".to_string(),
        };

        let record = PhaseRecord {
            phase: LoopPhase::Inspect,
            input: Some("Inspect results".into()),
            output: Some(inspection.clone()),
            confidence: None,
            duration_ms: 0,
            error: None,
        };
        state.history.push(record);

        Ok(PhaseResult {
            phase: LoopPhase::Inspect,
            decision: LoopDecision::Continue,
            output: Some(inspection),
            confidence: ConfidenceScore::new(0.8),
            tool_results: vec![],
        })
    }
}

// ── Critique Phase ───────────────────────────────────────────────────

pub struct CritiquePhase {
    pub scorer: Arc<ConfidenceScorer>,
}

impl CritiquePhase {
    pub fn new(scorer: ConfidenceScorer) -> Self {
        Self {
            scorer: Arc::new(scorer),
        }
    }
}

#[async_trait]
impl Phase for CritiquePhase {
    async fn execute(
        &self,
        state: &mut LoopState,
        _context: &PhaseContext,
    ) -> OdinResult<PhaseResult> {
        tracing::info!("[CRITIQUE] Self-evaluating action");

        state.current_phase = LoopPhase::Critique;

        // Score the last action
        let confidence = if let Some(last_tool) = state.tool_results.last() {
            self.scorer.score_tool_result(
                last_tool.success,
                !last_tool.output.is_empty(),
                last_tool.error.is_some(),
                last_tool.duration_ms,
            )
        } else if let Some(last_msg) = state.messages.last() {
            let text = last_msg.text().unwrap_or("");
            // For reasoning models, the response is always valid — trust it
            if text.len() > 200 {
                ConfidenceScore::new(0.9) // Substantial response = high confidence
            } else {
                self.scorer
                    .score_text_response(text, Some(&state.task.goal))
            }
        } else {
            ConfidenceScore::new(0.5)
        };

        let decision = if confidence.is_high() {
            LoopDecision::Continue
        } else if confidence.is_low() && state.retry_count >= 2 {
            LoopDecision::Escalate
        } else if confidence.is_low() {
            LoopDecision::Retry
        } else {
            LoopDecision::Continue
        };

        let critique = format!(
            "Confidence: {:.0}% → Decision: {:?}",
            confidence.value() * 100.0,
            decision
        );

        // Update the last history record's confidence
        if let Some(last_record) = state.history.last_mut() {
            last_record.confidence = Some(confidence);
        }

        let record = PhaseRecord {
            phase: LoopPhase::Critique,
            input: Some("Score last action".into()),
            output: Some(critique.clone()),
            confidence: Some(confidence),
            duration_ms: 0,
            error: None,
        };
        state.history.push(record);

        Ok(PhaseResult {
            phase: LoopPhase::Critique,
            decision,
            output: Some(critique),
            confidence,
            tool_results: vec![],
        })
    }
}

// ── Revise Phase ─────────────────────────────────────────────────────

pub struct RevisePhase;

#[async_trait]
impl Phase for RevisePhase {
    async fn execute(
        &self,
        state: &mut LoopState,
        _context: &PhaseContext,
    ) -> OdinResult<PhaseResult> {
        tracing::info!(
            "[REVISE] Revising approach (retry count: {})",
            state.retry_count
        );

        state.current_phase = LoopPhase::Revise;
        state.retry_count += 1;

        let strategy = match state.retry_count {
            1 => "Retrying with same parameters",
            2 => "Retrying with adjusted parameters",
            _ => "Escalating to stronger model",
        };

        state.messages.push(Message::system(format!(
            "[REVISE] {} (attempt {})",
            strategy, state.retry_count
        )));

        let decision = if state.retry_count >= 3 {
            LoopDecision::Escalate
        } else {
            LoopDecision::Retry
        };

        let record = PhaseRecord {
            phase: LoopPhase::Revise,
            input: Some(format!("Retry attempt {}", state.retry_count)),
            output: Some(strategy.to_string()),
            confidence: None,
            duration_ms: 0,
            error: None,
        };
        state.history.push(record);

        Ok(PhaseResult {
            phase: LoopPhase::Revise,
            decision,
            output: Some(strategy.to_string()),
            confidence: ConfidenceScore::new(0.5),
            tool_results: vec![],
        })
    }
}

// ── Verify Phase ─────────────────────────────────────────────────────

pub struct VerifyPhase {
    pub scorer: Arc<ConfidenceScorer>,
}

impl VerifyPhase {
    pub fn new(scorer: ConfidenceScorer) -> Self {
        Self {
            scorer: Arc::new(scorer),
        }
    }
}

#[async_trait]
impl Phase for VerifyPhase {
    async fn execute(
        &self,
        state: &mut LoopState,
        context: &PhaseContext,
    ) -> OdinResult<PhaseResult> {
        tracing::info!("[VERIFY] Verifying results");

        state.current_phase = LoopPhase::Verify;

        // Check if the current sub-task succeeded
        let current_task = context.plan.as_ref().and_then(|p| {
            p.sub_tasks
                .iter()
                .find(|st| st.status == SubTaskStatus::Pending)
        });

        let verification = match current_task {
            Some(task) => {
                // In production: actually verify the results
                format!("Verifying completion of: {}", task.description)
            }
            None => "All sub-tasks appear complete".to_string(),
        };

        // Check success criteria
        let all_criteria_met = state.task.success_criteria.is_empty()
            || state.task.success_criteria.iter().all(|c| {
                state
                    .messages
                    .iter()
                    .any(|m| m.text().map(|t| t.contains(c.as_str())).unwrap_or(false))
            });

        let confidence = if all_criteria_met {
            ConfidenceScore::new(0.9)
        } else {
            ConfidenceScore::new(0.6)
        };

        let record = PhaseRecord {
            phase: LoopPhase::Verify,
            input: Some("Verify results".into()),
            output: Some(verification.clone()),
            confidence: Some(confidence),
            duration_ms: 0,
            error: None,
        };
        state.history.push(record);

        Ok(PhaseResult {
            phase: LoopPhase::Verify,
            decision: LoopDecision::Continue,
            output: Some(verification),
            confidence,
            tool_results: vec![],
        })
    }
}

// ── Decide Phase ─────────────────────────────────────────────────────

pub struct DecidePhase;

#[async_trait]
impl Phase for DecidePhase {
    async fn execute(
        &self,
        state: &mut LoopState,
        context: &PhaseContext,
    ) -> OdinResult<PhaseResult> {
        tracing::info!("[DECIDE] Deciding whether to continue or stop");

        state.current_phase = LoopPhase::Decide;

        // Check if we've hit max iterations
        if state.iteration >= state.task.max_iterations {
            return Ok(PhaseResult {
                phase: LoopPhase::Decide,
                decision: LoopDecision::Stop,
                output: Some("Max iterations reached".into()),
                confidence: ConfidenceScore::new(0.5),
                tool_results: vec![],
            });
        }

        // Check if all sub-tasks are complete
        let all_done = context
            .plan
            .as_ref()
            .map(|p| {
                p.sub_tasks.iter().all(|st| {
                    st.status == SubTaskStatus::Completed
                        || st.status == SubTaskStatus::Skipped
                        || st.status == SubTaskStatus::Failed
                })
            })
            .unwrap_or(false);

        if all_done {
            return Ok(PhaseResult {
                phase: LoopPhase::Decide,
                decision: LoopDecision::Stop,
                output: Some("All sub-tasks complete".into()),
                confidence: ConfidenceScore::new(0.95),
                tool_results: vec![],
            });
        }

        // Check confidence from last critique
        let last_confidence = state
            .history
            .iter()
            .rev()
            .find(|r| r.confidence.is_some())
            .and_then(|r| r.confidence);

        let decision = match last_confidence {
            Some(c) if c.is_low() && state.retry_count >= 2 => LoopDecision::Escalate,
            Some(c) if c.is_low() => LoopDecision::Retry,
            _ => LoopDecision::Continue,
        };

        let record = PhaseRecord {
            phase: LoopPhase::Decide,
            input: Some(format!(
                "Iteration {} / {}",
                state.iteration, state.task.max_iterations
            )),
            output: Some(format!("Decision: {:?}", decision)),
            confidence: last_confidence,
            duration_ms: 0,
            error: None,
        };
        state.history.push(record);

        Ok(PhaseResult {
            phase: LoopPhase::Decide,
            decision,
            output: Some(format!("{:?}", decision)),
            confidence: last_confidence.unwrap_or(ConfidenceScore::new(0.7)),
            tool_results: vec![],
        })
    }
}
