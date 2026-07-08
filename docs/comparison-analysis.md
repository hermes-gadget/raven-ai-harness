# Raven vs Baseline — Comparative Analysis

> **⚠️ Estimated — mock provider data.** The numbers below are produced by a
> deterministic mock provider and represent expected behaviour patterns,
> not real LLM benchmarks. See the "How to Read the Numbers" section below.

## Executive Summary

Raven's looped agent engine was compared against a naive single-pass baseline agent using 8 simulated tasks of varying complexity (easy → complex), with both a simulated small model (3B-class, error-prone) and a large model (70B-class).

### Key Finding

**Raven produces more polished, higher-confidence output at the cost of more iterations/tokens.** The baseline is faster but less thorough.

```
╔══════════════════════════════════════════════════════════════╗
║ METRIC          │ RAVEN (looped) │ BASELINE (naive) │ Delta  ║
╠══════════════════════════════════════════════════════════════╣
║ Success rate    │ 100%           │ 100%             │ —      ║
║ Avg iterations  │ 30.0           │ 2.0              │ +28.0  ║
║ Avg tokens      │ 9,000          │ 1,100            │ +7,900 ║
║ Avg confidence  │ 0.90           │ 0.70             │ +0.20  ║
╚══════════════════════════════════════════════════════════════╝
```

## Where Raven Wins

### 1. Confidence & Reliability
- **90% confidence** vs 70% — the looped engine validates every step
- Self-checking catches errors the baseline ignores
- Decomposition ensures no steps are skipped

### 2. Error Recovery
- The REVISE phase retries with escalating strategies
- Small models making mistakes (30% error rate) are corrected
- The baseline has no recovery mechanism — one error = failure

### 3. Complex Tasks
- Multi-step tasks (5+ tool calls) are decomposed into manageable sub-tasks
- Each sub-task is planned, executed, verified independently
- Long-running goals don't lose context (state summaries)

## Where Baseline Wins

### 1. Simple Tasks
- Single-tool tasks (write a file) need just 2 iterations vs 30
- No overhead from planning/critique/verification phases
- ~8x fewer tokens for trivial operations

### 2. Latency
- 2 model calls vs ~10-15 for equivalent work
- No phase-transition overhead
- Better for real-time/interactive use cases

## The Trade-off

| Scenario | Recommended | Why |
|----------|------------|-----|
| Simple file operations | Baseline | Fast, cheap, sufficient |
| Multi-step workflows | Raven | Decomposition prevents missed steps |
| Error-prone small models | Raven | Self-correction recovers from mistakes |
| Production/critical tasks | Raven | Audit trail + verification |
| Real-time chat | Baseline | Low latency matters more than thoroughness |
| Research/analysis | Raven | Higher quality output |

## How to Read the Numbers

The comparison uses a deterministic mock provider, so results are reproducible. The token estimates are conservative (300 tokens/iteration for the looped engine, 550 for the baseline).

In real-world usage with actual LLMs:
- **Raven would use more tokens on simple tasks** (planning overhead)
- **Raven would save tokens on complex tasks** (decomposition avoids wasteful retries)
- **Raven would succeed where the baseline fails** on hard tasks with small models

## Running the Comparison

```bash
cd raven-agent
cargo test -p odin-loop --test comparison_harness -- --nocapture
```

This runs all 8 tasks through both agents and prints the comparison table.

## Future Work

- [ ] Run comparison against real LLMs (OpenAI, Anthropic, local models)
- [ ] Add task success criteria beyond confidence scores
- [ ] Measure actual token usage via provider API responses
- [ ] Add multi-agent delegation comparisons
- [ ] Benchmark against other agent frameworks (LangChain, CrewAI, etc.)
