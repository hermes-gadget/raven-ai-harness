# Raven AI Harness — TODO & Implementation Status

> Last updated: 2026-07-07 | Commit: 8162f5b

## Current Reality

### ✅ Working (real code, tests passing — 158 tests)
- **odin-core**: Config (full YAML + defaults), types, error types, traits (Provider, LoopEngine, Tool)
- **odin-loop**: 7-phase engine (PLAN→ACT→INSPECT→CRITIQUE→REVISE→VERIFY→DECIDE), decomposer, confidence scorer, summarizer
- **odin-runtime**: Agent lifecycle, session management, sub-agent spawning, task submission
- **odin-providers**: OpenAI-compat (real HTTP calls), Anthropic (real), Local/Ollama (real, delegates to OpenAI compat)
- **odin-tools**: File read/write, shell execution, web search/fetch, git commands, sandbox
- **odin-memory**: SQLite store with search/categories
- **odin-scheduler**: Cron job management
- **odin-permissions**: Policy engine, approval gates, rate limiting, secrets, path boundaries
- **odin-audit**: File + async audit logger
- **odin-skills**: Markdown-based skill registry
- **odin-gateway**: HTTP router (axum), Discord stub, WebSocket stub
- **odin-baseline**: Comparison agent for benchmarking

### ❌ Not Yet Wired / Stubs / Missing

#### 🔴 Critical (blocks real use)
- [ ] **CLI `run` doesn't execute tasks** — prints "scaffold, wire this up later"
- [ ] **CLI `serve` /chat returns 503** — no task handler registered
- [ ] **No provider factory from config** — can't create providers from YAML
- [ ] **Engine ESCALATE has no fallback** — marks ESCALATE but no stronger model
- [ ] **Loop engine ACT phase simulates tool execution** — doesn't call actual tool registry

#### 🟡 Important (blocks full features)
- [ ] **Memory not wired to runtime sessions** — sessions are in-memory only
- [ ] **Audit not wired to CLI/runtime** — audit logger exists but unused
- [ ] **Scheduler not wired to CLI** — scheduler exists but no CLI command
- [ ] **Permissions not wired to CLI/runtime** — policy engine unused
- [ ] **Gateway Discord stub** — has struct but not functional
- [ ] **Gateway WebSocket stub** — has struct but not functional
- [ ] **Engine REVISE phase is a stub** — retries but doesn't change strategy
- [ ] **No DeepSeek reasoning_content fallback in odin-providers** — only in test

#### 🟢 Docs & Infrastructure
- [ ] **ARCHITECTURE.md missing** — README links to it but doesn't exist
- [ ] **examples/config.yaml missing** — config docs claim it exists
- [ ] **CI branch mismatch** — CI triggers on `main` but repo uses `master`
- [ ] **cargo bench fails** — no `[[bench]]` targets defined
- [ ] **No integration tests** — only mock/simulated unit tests
- [ ] **Comparison analysis doc uses mock data** — claims "real API calls coming soon"
- [ ] **Hermes compatibility matrix claims** — some features marked ✅ are stubs

---

## Execution Plan

### Phase 1: Core CLI & Provider Wiring (THIS SESSION)
1. [ ] Fix CI branch (main→master) and `cargo bench`
2. [ ] Create provider factory from OdinConfig
3. [ ] Wire CLI `run` to create provider → loop engine → execute task
4. [ ] Wire CLI `serve` /chat handler to execute tasks
5. [ ] Add DeepSeek reasoning_content fallback to odin-providers
6. [ ] Create escalation chain: build cheaper + stronger provider pair
7. [ ] Wire tool registry into ACT phase for real tool execution
8. [ ] Add integration test: end-to-end CLI run with mock provider
9. [ ] Run cargo fmt + clippy + check + test — verify all green

### Phase 2: Feature Completion
10. [ ] Wire memory (SQLite) to runtime sessions
11. [ ] Wire audit logger to CLI/runtime
12. [ ] Wire permissions engine into tool execution
13. [ ] Add `odin schedule` CLI command
14. [ ] Make Gateway Discord functional (or clearly mark as stub with test)
15. [ ] Implement REVISE phase with actual strategy changes

### Phase 3: Docs & Polish
16. [ ] Create ARCHITECTURE.md
17. [ ] Create examples/config.yaml
18. [ ] Update Hermes compatibility matrix to reflect reality
19. [ ] Update comparison-analysis.md with real provider benchmark
20. [ ] Add benchmark comparing looped vs baseline (criterion)

---

## Running the Live Test

```bash
# Store API key (never in repo)
echo 'DEEPSEEK_API_KEY=sk-...' > ~/.odin/.env && chmod 600 ~/.odin/.env

# Run
cargo test -p odin-loop --test live_comparison -- --nocapture --ignored
```

Expected: 4 iterations, 3/3 sub-tasks completed, 90% confidence, ~3200 tokens, ~22s.
