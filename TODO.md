# Raven AI Harness — TODO & Implementation Status

> Updated: 2026-07-07 | Based on commit 6d48306 audit (14 crates, 184 tests, 1 ignored)

## Audit Summary

| Crate | Tests | Status | Wired? |
|-------|-------|--------|--------|
| `odin-core` | 19 | ✅ Complete | Foundation |
| `odin-loop` | 28 | ✅ Complete — all phases use LLM when available | Used by CLI |
| `odin-providers` | 17 | ✅ Complete — OpenAI, Anthropic, Local, DeepSeek fallback | Used by CLI |
| `odin-tools` | 30 | ✅ Complete — registry, file, shell, web, git, sandbox | Used by CLI |
| `odin-memory` | 27 | ✅ Complete — SQLite store | ✅ Wired in CLI |
| `odin-permissions` | 15 | ✅ Complete — policy, approval, secrets | ⚠️ NOT wired in CLI |
| `odin-audit` | 7 | ✅ Complete — file logger | ✅ Wired in CLI |
| `odin-scheduler` | 12 | ✅ Complete — cron jobs | ⚠️ NOT wired in CLI |
| `odin-skills` | 10 | ✅ Complete — markdown registry | Not wired |
| `odin-runtime` | 17 | ✅ Complete | Used by CLI |
| `odin-gateway` | 5 | ⚠️ HTTP real, Discord stub, WS stub | Used by CLI serve |
| `odin-baseline` | 2 | ✅ Complete | For benchmarks |
| `odin-config` | 0 | ⚜️ Empty crate | Not used |
| `odin-cli` | 6 | ✅ Complete — run/serve/config/version | Entry point |
| `odin-loop` (live) | 0+1 | ✅ Real — live DeepSeek test | Ignored |

**Total: 184 passing (+5 integration), 0 failing, 1 ignored (live API test)**

## ✅ Done (Verified)

### Foundation
- [x] `odin-core`: Complete type system, config, error types, core traits
- [x] Config YAML loading/saving with defaults

### Loop Engine
- [x] 7-phase structure: PLAN → ACT → INSPECT → CRITIQUE → REVISE → VERIFY → DECIDE
- [x] PLAN uses LLM decomposition when provider available (bullet-point parsing with heuristic fallback)
- [x] CRITIQUE uses LLM for confidence scoring (parses "Confidence: 0.X" / "Score: XX%" patterns)
- [x] VERIFY uses LLM to check success criteria (looks for "VERIFIED" keyword)
- [x] REVISE uses LLM for revised approach (heuristic fallback by retry count)
- [x] ACT dispatches real tool calls via ToolRegistry
- [x] Escalation chain: weak provider → strong provider on low confidence
- [x] Sub-task completion tracking

### Providers
- [x] OpenAI-compatible: real HTTP POST
- [x] Anthropic: real HTTP POST
- [x] Local/Ollama: real HTTP POST
- [x] Provider factory: `create_provider()` from config
- [x] DeepSeek `reasoning_content` fallback

### CLI
- [x] `odin run <task>` — creates provider + engine + tools + agent + runtime, submits task
- [x] Memory store wired (SqliteMemoryStore) with in-memory fallback
- [x] Audit logger wired (AuditLoggerImpl with file) — logs task start/end
- [x] `odin serve` — HTTP server with /chat handler + memory + audit
- [x] `odin config` / `odin version`

### Tests (184 pass)
- [x] Full-cycle with mocked provider (LLM calls → tool dispatch → result)
- [x] Tool call dispatch test (LLM calls tool → Shell executes)
- [x] Provider error graceful degradation (failing → handled)
- [x] Escalation to stronger provider (weak → strong swap)
- [x] Retry on low confidence (short response → retry → better)
- [x] Max iterations bound
- [x] All 7 phases execute individually
- [x] Empty response handling (reasoning models)
- [x] Looped vs baseline comparison test
- [x] 5 comparison harness tests (small/large model, token efficiency, error recovery)

### CI & Docs
- [x] CI: check, test, lint, bench, security on `master` branch
- [x] README: correct CLI usage, Discord marked as stub
- [x] ARCHITECTURE.md, examples/config.yaml, hermes-compatibility.md

## ⚠️ Remaining Work

### Wire Permissions to Tool Execution
- [ ] Create PolicyEngine from config in CLI cmd_run/cmd_serve
- [ ] Wrap tool calls with PolicyEngine.check() before execution
- [ ] Wire ApprovalGate for dangerous shell commands
- [ ] Test: policy denies tool → blocked; approval required → gate

### Wire Scheduler to CLI
- [ ] Add `odin schedule` subcommand (add/list/remove/pause/resume cron jobs)
- [ ] Wire Scheduler to Runtime for job execution

### Test Gaps
- [ ] CLI integration test: run task through mocked provider, verify stdout output
- [ ] Safety boundary test: sandbox denies write outside boundary
- [ ] Safety boundary test: dangerous shell command blocked
- [ ] Permission policy test: allowed tool passes, denied tool blocked

### Benchmark
- [ ] Real metric-measuring comparison: looped vs baseline with identical mock provider
- [ ] Measure: iterations, confidence, token usage, success rate, tool calls

### Quality
- [ ] `cargo fmt` + `cargo clippy` clean
- [ ] All tests pass (184+)
- [ ] Commit and push

## ⏳ Deferred (v0.2+)
- Discord bot integration (stub exists)
- WebSocket implementation
- Streaming provider support
- Web dashboard
- Telegram/Slack gateways
- Vector embeddings for memory
- Plugin system / MCP server support
- PII redaction / Interactive TUI / Profiles
