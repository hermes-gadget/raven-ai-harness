# Raven AI Harness — TODO & Implementation Status

> Updated: 2026-07-07 | Commit: 20fd2e2 | 189 tests pass, 1 ignored

## Audit Summary

| Crate | Tests | Status | Wired? |
|-------|-------|--------|--------|
| `odin-core` | 19 | ✅ Complete | Foundation |
| `odin-loop` | 33 | ✅ Complete — all 7 phases use LLM when available | Used by CLI |
| `odin-providers` | 17 | ✅ Complete — OpenAI, Anthropic, Local, DeepSeek | Used by CLI |
| `odin-tools` | 30 | ✅ Complete — registry, file, shell, web, git, sandbox | Used by CLI |
| `odin-memory` | 27 | ✅ Complete — SQLite store | ✅ Wired in CLI |
| `odin-permissions` | 15 | ✅ Complete — policy, approval, secrets | ✅ Wired in CLI + engine |
| `odin-audit` | 7 | ✅ Complete — file logger | ✅ Wired in CLI |
| `odin-scheduler` | 12 | ✅ Complete — cron jobs | ⚠️ NOT wired in CLI |
| `odin-skills` | 10 | ✅ Complete — markdown registry | Not wired |
| `odin-runtime` | 17 | ✅ Complete | Used by CLI |
| `odin-gateway` | 5 | ⚠️ HTTP real, Discord stub, WS stub | Used by CLI serve |
| `odin-baseline` | 2 | ✅ Complete | For benchmarks |
| `odin-cli` | 6 | ✅ Complete — run/serve/config/version | Entry point |
| `odin-loop` (live) | 0+1 | ✅ Real — live DeepSeek test | Ignored |

## ✅ Done (Verified with real execution)

### Foundation
- [x] 13 crates, complete type system, config, error types, core traits
- [x] Config YAML loading/saving with defaults

### Loop Engine — ALL 7 phases use LLM when provider available
- [x] PLAN: LLM decomposition with bullet-point parsing, heuristic fallback
- [x] ACT: real LLM call + real tool dispatch via ToolRegistry
- [x] INSPECT: context window check + tool result validation
- [x] CRITIQUE: LLM confidence scoring, parses "Confidence: 0.X / Score: XX%"
- [x] REVISE: LLM revised approach, heuristic fallback by retry count
- [x] VERIFY: LLM checks success criteria, looks for "VERIFIED" keyword
- [x] DECIDE: iteration bound + sub-task completion + confidence-based decisions
- [x] Escalation: weak provider → strong provider on low confidence
- [x] Context compression via StateSummarizer when nearing token limits

### Safety
- [x] PolicyEngine checks dangerous shell commands before execution
- [x] PolicyEngine checks file read/write path boundaries
- [x] Sandbox enforces allowed_read/allowed_write/denied paths
- [x] Audit logging (file-based) of task start/end

### CLI — verified working with real DeepSeek provider
- [x] `odin run "Write a hello world program in Python"` → 4 iterations, 90% conf, 3/3 sub-tasks ✅
- [x] Memory store + audit logger wired in both `run` and `serve`
- [x] `odin serve` HTTP API with /chat handler
- [x] `odin config` / `odin version`

### Tests (189 pass, 1 ignored)
- [x] Full-cycle with mocked provider (LLM → tools → result)
- [x] Tool dispatch (LLM calls shell → real execution)
- [x] Provider errors gracefully handled
- [x] Escalation: weak → strong provider swap
- [x] Retry on low confidence
- [x] Max iterations bounded
- [x] All 7 phases execute individually
- [x] Empty response handling (reasoning models)
- [x] Looped vs baseline comparison
- [x] CLI integration test (sandbox + shell + file_read)
- [x] Sandbox denies write outside boundary
- [x] Dangerous shell command blocked
- [x] Policy allows/denies tools
- [x] Comparison metrics benchmark (3 task types)

### CI & Docs
- [x] CI: check, test, lint, bench, security on `master` branch
- [x] README: correct CLI usage, Discord marked as stub ⚠️
- [x] ARCHITECTURE.md (278 lines)
- [x] examples/config.yaml (215 lines)
- [x] hermes-compatibility.md (honest about stubs)

## ⚠️ Remaining Work

### Scheduler CLI
- [ ] Add `odin schedule` subcommand: add/list/remove/pause/resume cron jobs
- [ ] Wire Scheduler to Runtime for job execution

### Benchmark
- [ ] Proper criterion benchmark (benches/loop_bench.rs) comparing looped vs baseline
- [ ] Measure: iterations, confidence, token usage, success rate

### Quality
- [ ] cargo fmt + clippy clean
- [ ] All 189+ tests pass
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
- Skills integration into execution pipeline
