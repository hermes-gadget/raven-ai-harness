//! Benchmarks for the Raven loop engine.
//!
//! Compare the looped engine against a basic single-pass agent loop.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use odin_core::traits::LoopState;
use odin_core::types::*;
use odin_loop::engine::Engine as LoopEngine;
use odin_loop::traits::LoopEngine as LoopEngineTrait;

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
                black_box(engine.execute_phase(LoopPhase::Plan, &mut state).await.unwrap());
            });
        })
    });
}

fn bench_state_summarization(c: &mut Criterion) {
    use odin_loop::summarizer::StateSummarizer;

    let summarizer = StateSummarizer::default();

    c.bench_function("summarize_large_state", |b| {
        b.iter(|| {
            let mut state = LoopState {
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

criterion_group!(
    benches,
    bench_loop_engine_execution,
    bench_loop_phases,
    bench_state_summarization,
);
criterion_main!(benches);
