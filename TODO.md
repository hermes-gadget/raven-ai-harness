# Raven AI Harness — TODO & Real Implementation Status

> Generated: 2026-07-07 | Based on full codebase audit (15 crates, 175 tests, 1 ignored)

## Audit Summary

| Crate | Tests | Real vs Stub | Wired? |
|-------|-------|-------------|--------|
| `odin-core` | 19 | ✅ Real — types, config, traits, error | Foundation |
| `odin-loop` | 8 | ⚠️ Mixed — engine real, phases mostly heuristic | ACT uses LLM; others don't |
| `odin-providers` | 17 | ✅ Real — OpenAI, Anthropic, Local, factory | Used by CLI |
| `odin-tools` | 30 | ✅ Real — registry, file, shell, web, git, sandbox | Used by CLI |
| `odin-memory` | 27 | ✅ Real — SQLite store | Not wired in CLI |
| `odin-permissions` | 15 | ✅ Real — policy, approval, secrets | Not wired in CLI |
| `odin-audit` | 7 | ✅ Real — file logger | Not wired in CLI |
| `odin-scheduler` | 12 | ✅ Real — cron jobs | Not wired in CLI |
| `odin-skills` | 10 | ✅ Real — markdown skill registry | Not wired |
| `odin-runtime` | 17 | ✅ Real — agent/session/sub-agent management | Used by CLI |
| `odin-gateway` | 5 | ⚠️ Mixed — HTTP real, Discord stub, WS stub | Used by CLI serve |
| `odin-baseline` | 2 | ✅ Real — naive single-pass agent | For benchmarks |
| `odin-config` | 0 | ⚠️ Empty crate | Referenced but empty |
| `odin-cli` | 6 | ✅ Real — run/serve/config/version | Entry point |
| `odin-loop` (live) | 0+1 ignored | ✅ Real — live DeepSeek test | Ignored |

**Total: 175 passing, 0 failing, 1 ignored (live API test)**

## ✅ Done (Real Implementation)

### Foundation
- [x] `odin-core`: Complete type system, config structs, error types, core traits
- [x] Provider trait abstraction (chat, stream, health_check, list_models)
- [x] Config YAML loading/saving with defaults

### Loop Engine
- [x] 7-phase structure: PLAN → ACT → INSPECT → CRITIQUE → REVISE → VERIFY → DECIDE
- [x] ACT phase uses real LLM with provider, dispatches real tool calls via ToolRegistry
- [x] Goal decomposer (heuristic, keyword-based)
- [x] Confidence scorer (heuristic scoring)
- [x] State summarizer (context compression)
- [x] Escalation chain in engine.js (provider swap on low confidence)
- [x] Sub-task completion tracking

### Providers
- [x] OpenAI-compatible: real HTTP POST to /v1/chat/completions
- [x] Anthropic: real HTTP POST to /v1/messages
- [x] Local/Ollama: real HTTP POST
- [x] Provider factory: `create_provider()` from config
- [x] API key resolution (direct or env var)
- [x] DeepSeek `reasoning_content` fallback

### Tools
- [x] ToolRegistry: thread-safe add/remove/get/list
- [x] FileRead/FileWrite with sandbox path boundaries
- [x] Shell with dangerous command blocking
- [x] Web search/fetch
- [x] Git command wrapping

### Runtime
- [x] Agent registration and lifecycle
- [x] Session creation, labeling, deletion
- [x] Task submission to agents
- [x] Sub-agent spawning
- [x] Memory store trait integration point
- [x] `RuntimeSummary` for state inspection

### Memory, Permissions, Audit, Scheduler, Skills
- [x] SqliteMemoryStore: store/search/delete/categorize with tags
- [x] PolicyEngine: allow/deny/ask-user rules
- [x] ApprovalGate: interactive approval flow
- [x] SecretManager: secure credential handling
- [x] AuditLoggerImpl: file-based structured logging
- [x] Scheduler: cron-style job scheduling (add/remove/pause/resume)
- [x] Skill Registry: markdown-based skill loading

### CLI
- [x] `odin run <task>` — creates provider + engine + tools + agent + runtime, submits task
- [x] `odin serve` — HTTP server with real /chat handler
- [x] `odin config` — show/edit config
- [x] `odin version` — version info

### CI & Docs
- [x] CI: check, test, lint, bench, security (all green)
- [x] ARCHITECTURE.md (278 lines)
- [x] examples/config.yaml (215 lines)
- [x] hermes-compatibility.md (honest about stubs)

## ⚠️ Gaps — To Fix This Session

### Critical: Phases Don't Use LLM
- [ ] **CRITIQUE phase**: Heuristic-only, never calls LLM. Should call provider when available to get real critique of the last action.
- [ ] **VERIFY phase**: Heuristic-only, never calls LLM. Should call provider to verify results against success criteria.
- [ ] **REVISE phase**: Heuristic-only, never calls LLM. Should call provider to generate revised approach.
- [ ] **INSPECT phase**: Only checks context window size and tool results. Should use LLM for deeper inspection.
- [ ] **DECIDE phase**: Uses simple decision tree. Should optionally use LLM for complex decisions.
- [ ] **PLAN phase**: Uses heuristic keyword decomposer only. Should prefer LLM decomposition when provider is available.

### Critical: CLI Missing Wiring
- [ ] **Memory not wired**: `cmd_run` and `cmd_serve` don't create SqliteMemoryStore or pass it to Runtime
- [ ] **Audit not wired**: No audit logging during task execution in CLI
- [ ] **Permissions not wired**: No policy/approval checks during tool execution
- [ ] **Scheduler not wired**: No cron job support in CLI

### Tests Needed
- [ ] Full-cycle test with mocked LLM (all 7 phases execute)
- [ ] Escalation path test (mock low confidence → escalate → retry)
- [ ] Retry/revise path test
- [ ] Safety boundary violation test (sandbox blocks)
- [ ] ToolRegistry dispatch test (LLM calls tool → real execution)
- [ ] Provider error handling test (HTTP failure, timeout)
- [ ] Confidence scorer edge cases (empty output, error output, reasoning models)
- [ ] Integration test: CLI run → mocked provider → task result

### Benchmark
- [ ] Real comparison benchmark: looped engine vs baseline agent with same mocked provider
- [ ] Measure: iterations, confidence, token usage, success rate

### Docs Fixes
- [ ] README: remove `--goal` flag (actual flag is positional `task`)
- [ ] README: remove `--provider` flag (not implemented)
- [ ] README: mark Discord as "stub (planned for v0.2)"
- [ ] README: mark audit as "partial (backend exists, CLI wiring needed)"

### CI Fixes
- [ ] bench job: remove duplicate `cargo test` (already in test job)
- [ ] bench job: run actual `cargo bench` or at least `cargo check --benches`

### Quality
- [ ] Verify CLI `run` actually executes a task end-to-end (with mocked provider)
- [ ] cargo fmt + clippy clean
- [ ] All 175 tests pass
- [ ] Commit and push

## ⏳ Deferred (v0.2+)
- Discord bot integration (stub exists)
- WebSocket implementation
- Streaming provider support
- Web dashboard
- Telegram/Slack gateways
- Vector embeddings for memory
- Plugin system
- MCP server support
- PII redaction
- Interactive TUI
- Profiles support
