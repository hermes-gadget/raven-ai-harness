//! Benchmarks for the Raven loop engine.
//!
//! Compare the looped engine against a basic single-pass agent loop.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use odin_core::error::OdinResult;
use odin_core::traits::{ChatStream, LoopEngine as LoopEngineTrait, LoopState, Provider};
use odin_core::types::*;
use odin_loop::LoopEngine;
use std::sync::Arc;

fn make_task(goal: &str) -> AgentTask {
    AgentTask {
        id: TaskId::new_v4(),
        goal: goal.to_string(),
        context: None,
        sub_tasks: vec![],
        success_criteria: vec![],
        max_iterations: 5,
        created_at: chrono::Utc::now(),
    }
}

fn bench_loop_engine_execution(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("loop_engine_simple_task", |b| {
        b.iter(|| {
            let engine = LoopEngine::new().with_max_iterations(3);
            let task = make_task("Create a hello world file");
            rt.block_on(async {
                black_box(engine.execute_task(&task).await.unwrap());
            });
        })
    });

    c.bench_function("loop_engine_complex_task", |b| {
        b.iter(|| {
            let engine = LoopEngine::new().with_max_iterations(5);
            let task = make_task("Build a REST API with authentication and database integration");
            rt.block_on(async {
                black_box(engine.execute_task(&task).await.unwrap());
            });
        })
    });
}

fn bench_loop_phases(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let engine = LoopEngine::new();

    c.bench_function("phase_plan", |b| {
        b.iter(|| {
            let task = make_task("Test plan phase");
            let mut state = LoopState {
                task: task.clone(),
                messages: vec![Message::user("test")],
                tool_results: vec![],
                current_phase: LoopPhase::Plan,
                iteration: 0,
                retry_count: 0,
                history: vec![],
            };
            rt.block_on(async {
                black_box(
                    engine
                        .execute_phase(LoopPhase::Plan, &mut state)
                        .await
                        .unwrap(),
                );
            });
        })
    });
}

fn bench_state_summarization(c: &mut Criterion) {
    let summarizer = odin_loop::StateSummarizer::default();

    c.bench_function("summarize_large_state", |b| {
        b.iter(|| {
            let state = LoopState {
                task: make_task("A long complex goal"),
                messages: (0..50)
                    .map(|i| Message::user(format!("Message number {} with some content", i)))
                    .collect(),
                tool_results: vec![],
                current_phase: LoopPhase::Act,
                iteration: 10,
                retry_count: 0,
                history: vec![],
            };
            black_box(summarizer.summarize(&state));
        })
    });
}

// ── Looped vs Baseline Comparison ────────────────────────────────────

/// A mock provider that returns "Done" immediately (no tool calls).
struct MockProvider;

#[async_trait::async_trait]
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
        Ok(ChatResponse {
            message: Message::assistant("Done."),
            usage: TokenUsage::default(),
            finish_reason: Some("stop".into()),
            model: "mock".into(),
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

fn bench_looped_vs_baseline(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("looped_vs_baseline");

    // Benchmark the looped engine (no provider — stub mode, fast)
    group.bench_function("looped_engine", |b| {
        b.iter(|| {
            let engine = LoopEngine::new().with_max_iterations(5);
            let task = make_task("Write a hello world program");
            rt.block_on(async {
                black_box(engine.execute_task(&task).await.unwrap());
            });
        })
    });

    // Benchmark the baseline agent with a mock provider
    group.bench_function("baseline_agent", |b| {
        b.iter(|| {
            let mock = Arc::new(MockProvider);
            let baseline = odin_baseline::BaselineAgent::new(mock, vec![], 5);
            let task = make_task("Write a hello world program");
            rt.block_on(async {
                black_box(baseline.execute_task(&task).await.unwrap());
            });
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_loop_engine_execution,
    bench_loop_phases,
    bench_state_summarization,
    bench_looped_vs_baseline,
);
criterion_main!(benches);
