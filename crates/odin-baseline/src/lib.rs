//! Odin Baseline — A naive single-pass agent for comparison.
//!
//! Implements the simplest possible agent loop:
//!   1. Call LLM with messages + tools
//!   2. If tool calls, execute them, append results
//!   3. Repeat until no tool calls or max iterations
//!
//! No planning, no critique, no verification, no decomposition.
//! This is what most agent harnesses do — Raven's loop engine should
//! outperform this on smaller models.

use async_trait::async_trait;
use chrono::Utc;
use odin_core::error::OdinResult;
use odin_core::traits::{LoopEngine as LoopEngineTrait, LoopState, PhaseResult, Provider};
use odin_core::types::*;
use std::sync::Arc;

/// A baseline agent that uses a simple call→tool→repeat loop.
///
/// This is intentionally naive — it represents the "control group"
/// against which Raven's looped engine is compared.
pub struct BaselineAgent {
    provider: Arc<dyn Provider>,
    tools: Vec<ToolSchema>,
    max_iterations: u32,
    /// Simulated token cost per prompt (for comparison)
    tokens_per_prompt: u32,
    tokens_per_completion: u32,
}

impl BaselineAgent {
    pub fn new(provider: Arc<dyn Provider>, tools: Vec<ToolSchema>, max_iterations: u32) -> Self {
        Self {
            provider,
            tools,
            max_iterations,
            tokens_per_prompt: 0,
            tokens_per_completion: 0,
        }
    }

    /// Total estimated tokens used (for comparison with looped engine).
    pub fn estimated_tokens_used(&self) -> TokenUsage {
        TokenUsage {
            prompt_tokens: self.tokens_per_prompt,
            completion_tokens: self.tokens_per_completion,
            total_tokens: self.tokens_per_prompt + self.tokens_per_completion,
        }
    }
}

#[async_trait]
impl LoopEngineTrait for BaselineAgent {
    async fn execute_task(&self, task: &AgentTask) -> OdinResult<TaskResult> {
        let start = std::time::Instant::now();

        let mut messages = vec![
            Message::system(
                "You are an AI assistant. Complete the user's task using available tools.",
            ),
            Message::user(format!("Goal: {}", task.goal)),
        ];

        if let Some(ref ctx) = task.context {
            messages.push(Message::system(format!("Context: {}", ctx)));
        }

        let model = "default";
        let options = CompletionOptions::default();
        let mut total_tool_calls: u32 = 0;
        let mut success = false;

        for iteration in 1..=task.max_iterations {
            // Estimate token usage
            self.estimate_tokens(&messages);

            // Call the model
            let response = match self
                .provider
                .chat(model, &messages, &self.tools, &options)
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    let duration_ms = start.elapsed().as_millis() as u64;
                    return Ok(TaskResult {
                        task_id: task.id,
                        success: false,
                        summary: format!("Model error: {}", e),
                        iterations: iteration,
                        tool_calls: total_tool_calls,
                        duration_ms,
                        sub_tasks: vec![],
                        confidence: 0.0,
                        error: Some(e.to_string()),
                    });
                }
            };

            let tool_calls: Vec<ToolCall> = response.message.tool_calls().to_vec();
            let response_text = response.message.text().map(|s| s.to_string());
            messages.push(response.message);

            if tool_calls.is_empty() {
                // No tool calls — assume done
                success = true;
                let duration_ms = start.elapsed().as_millis() as u64;
                return Ok(TaskResult {
                    task_id: task.id,
                    success: true,
                    summary: response_text.unwrap_or_else(|| "Done".into()),
                    iterations: iteration,
                    tool_calls: total_tool_calls,
                    duration_ms,
                    sub_tasks: vec![],
                    confidence: 0.7,
                    error: None,
                });
            }

            // Execute tool calls
            for tc in &tool_calls {
                total_tool_calls += 1;

                // In a real agent, tools would actually execute.
                // For comparison benchmarks, we use a mock provider
                // that simulates tool results.
                let result = ToolResult {
                    call_id: tc.id.clone(),
                    tool_name: tc.function.name.clone(),
                    success: true,
                    output: format!("[Simulated] Executed {}", tc.function.name),
                    error: None,
                    duration_ms: 1,
                    timestamp: Utc::now(),
                };

                messages.push(Message::tool_result(
                    &tc.id,
                    serde_json::to_string(&result).unwrap_or_default(),
                ));
            }
        }

        // Hit max iterations
        let duration_ms = start.elapsed().as_millis() as u64;
        Ok(TaskResult {
            task_id: task.id,
            success,
            summary: "Max iterations reached without completion".into(),
            iterations: task.max_iterations,
            tool_calls: total_tool_calls,
            duration_ms,
            sub_tasks: vec![],
            confidence: if success { 0.5 } else { 0.2 },
            error: if success {
                None
            } else {
                Some("Exceeded max iterations".into())
            },
        })
    }

    async fn execute_phase(
        &self,
        _phase: LoopPhase,
        _state: &mut LoopState,
    ) -> OdinResult<PhaseResult> {
        unimplemented!("Baseline agent has no phases")
    }

    fn state_summary(&self) -> StateSummary {
        StateSummary {
            goal: String::new(),
            current_phase: LoopPhase::Plan,
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

impl BaselineAgent {
    /// Estimate tokens from messages (rough heuristic: ~4 chars per token).
    fn estimate_tokens(&self, _messages: &[Message]) {
        // Token counting happens implicitly through the provider.
        // For comparative benchmarks, we use a mock that tracks this.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use odin_core::traits::ChatStream;

    /// A mock provider that returns deterministic responses for testing.
    struct MockProvider {
        responses: Vec<ChatResponse>,
        call_count: std::sync::Mutex<usize>,
    }

    impl MockProvider {
        fn new(responses: Vec<ChatResponse>) -> Self {
            Self {
                responses,
                call_count: std::sync::Mutex::new(0),
            }
        }
    }

    #[async_trait]
    impl Provider for MockProvider {
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
            let mut count = self.call_count.lock().unwrap();
            let idx = *count;
            *count += 1;

            if idx < self.responses.len() {
                Ok(self.responses[idx].clone())
            } else {
                // Return a final text response (no tool calls)
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

    #[tokio::test]
    async fn test_baseline_completes_simple_task() {
        let mock = MockProvider::new(vec![ChatResponse {
            message: Message::assistant("I've completed the task."),
            usage: TokenUsage::default(),
            finish_reason: Some("stop".into()),
            model: "mock".into(),
        }]);

        let agent = BaselineAgent::new(Arc::new(mock), vec![], 10);
        let task = AgentTask {
            id: TaskId::new_v4(),
            goal: "Say hello".into(),
            context: None,
            sub_tasks: vec![],
            success_criteria: vec![],
            max_iterations: 10,
            created_at: Utc::now(),
        };

        let result = agent.execute_task(&task).await.unwrap();
        assert!(result.success);
        assert_eq!(result.iterations, 1);
    }

    #[tokio::test]
    async fn test_baseline_uses_tools() {
        let tool_call = ToolCall {
            id: "call_1".into(),
            call_type: "function".into(),
            function: FunctionCall {
                name: "read_file".into(),
                arguments: r#"{"path":"test.txt"}"#.into(),
            },
        };

        let mock = MockProvider::new(vec![
            ChatResponse {
                message: Message {
                    role: Role::Assistant,
                    content: MessageContent::ToolCalls {
                        content: Some("Let me read the file.".into()),
                        tool_calls: vec![tool_call],
                    },
                    name: None,
                    tool_call_id: None,
                },
                usage: TokenUsage::default(),
                finish_reason: Some("tool_calls".into()),
                model: "mock".into(),
            },
            ChatResponse {
                message: Message::assistant("File read successfully."),
                usage: TokenUsage::default(),
                finish_reason: Some("stop".into()),
                model: "mock".into(),
            },
        ]);

        let agent = BaselineAgent::new(Arc::new(mock), vec![], 10);
        let task = AgentTask {
            id: TaskId::new_v4(),
            goal: "Read test.txt".into(),
            context: None,
            sub_tasks: vec![],
            success_criteria: vec![],
            max_iterations: 10,
            created_at: Utc::now(),
        };

        let result = agent.execute_task(&task).await.unwrap();
        assert!(result.success);
        assert_eq!(result.tool_calls, 1);
        assert_eq!(result.iterations, 2); // One tool call + one final response
    }
}
