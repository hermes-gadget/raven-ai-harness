# Raven AI Harness — TODO & Implementation Status

> Last updated: 2026-07-07 | Commit: dc777d6

## ✅ Done — v0.1.0

### Core Infrastructure (175 tests pass)
- ✅ **odin-core**: Config (YAML + defaults), types, error types, traits
- ✅ **odin-loop**: 7-phase engine (PLAN→ACT→INSPECT→CRITIQUE→REVISE→VERIFY→DECIDE) with real LLM calls, decomposer, confidence scorer, summarizer
- ✅ **odin-runtime**: Agent lifecycle, session management, sub-agent spawning, task submission, optional MemoryStore
- ✅ **odin-providers**: Factory from config, OpenAI-compat (real HTTP), Anthropic (real HTTP), Local/Ollama (real HTTP), reasoning_content fallback
- ✅ **odin-tools**: File/Shell/Web/Git with sandbox boundaries, tool registry
- ✅ **odin-memory**: SQLite store with search/categories, wired to Runtime
- ✅ **odin-scheduler**: Cron job management
- ✅ **odin-permissions**: Policy engine, approval gates, rate limiting, secrets, path boundaries
- ✅ **odin-audit**: File + async audit logger, wired to CLI
- ✅ **odin-skills**: Markdown-based skill registry
- ✅ **odin-gateway**: HTTP router (axum), /chat handler, Discord stub, WebSocket stub
- ✅ **odin-baseline**: Comparison agent
- ✅ **odin-cli**: `run` executes tasks, `serve` has /chat handler, `config`, `version`

### Key Features Wired
- ✅ CLI `run` — creates provider from config, executes tasks through loop engine
- ✅ CLI `serve` — /chat endpoint dispatches real tasks
- ✅ Provider factory — `create_provider()` from ProviderConfig
- ✅ Escalation chain — engine switches to escalation_provider on low confidence
- ✅ Tool execution — ACT phase dispatches to ToolRegistry
- ✅ DeepSeek reasoning_content fallback in openai_compat
- ✅ Memory wired to Runtime with `with_memory()`

### Documentation
- ✅ ARCHITECTURE.md (278 lines)
- ✅ examples/config.yaml (215 lines)
- ✅ Hermes compatibility matrix updated
- ✅ comparison-analysis.md updated
- ✅ TODO.md (this file)

### Infrastructure
- ✅ CI branch fix (main→master)
- ✅ CI passes: check, test (175 pass), clippy (0 errors)
- ✅ cargo fmt applied
- ✅ Live test against deepseek-v4-flash (ignored, needs key)

### Benchmarks
- ✅ Comparison harness: 5 tests comparing looped vs baseline
- ✅ Live comparison test proves looped engine works (90% conf, 3/3 sub-tasks)

## ⏳ Deferred (v0.2)

- ⏳ Discord bot integration (stub exists)
- ⏳ WebSocket real implementation
- ⏳ Streaming provider support
- ⏳ Web dashboard
- ⏳ Telegram/Slack gateways
- ⏳ Vector embeddings for memory

## ❌ Out of Scope

- ❌ Voice/STT/TTS
- ❌ WhatsApp/Signal
- ❌ Python/JS/TS runtime code
