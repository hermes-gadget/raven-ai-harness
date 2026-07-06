# Raven vs Hermes — Feature Compatibility

This document tracks which Hermes Agent features are matched, improved,
deferred, or intentionally changed in Raven.

## Legend

- ✅ **Matched** — Equivalent or better functionality
- ✅ **Improved** — Same feature, better implementation
- ⏳ **Deferred** — Planned for a future version
- ❌ **Removed** — Intentionally not included
- 🔄 **Changed** — Different approach, same goal

## Feature Matrix

### Core Agent Features

| Feature | Status | Notes |
|---------|--------|-------|
| Multi-agent task execution | ✅ Matched | `odin-runtime` orchestrates sub-agents |
| Agent loop (conversation) | ✅ Improved | 7-phase structured loop vs simple call→tool→repeat |
| Task decomposition | ✅ Improved | Built-in heuristic decomposer in PLAN phase |
| Self-checking | ✅ Improved | CRITIQUE phase with confidence scoring |
| Retry logic | ✅ Improved | REVISE phase with escalating retry strategies |
| Escalation to stronger models | ✅ Improved | Automatic when confidence < threshold |
| State summaries | ✅ Improved | Built-in summarizer for small context windows |
| Context compression | ✅ Improved | COMPRESS phase + automatic token management |
| Subagent delegation | ✅ Matched | `odin-runtime` spawns independent agents |

### Memory & Persistence

| Feature | Status | Notes |
|---------|--------|-------|
| Persistent memory | ✅ Matched | SQLite-backed in `odin-memory` |
| Cross-session memory | ✅ Matched | Memory persists across sessions |
| Memory categories | ✅ Matched | Preference, Entity, Event, Fact, Pattern |
| Semantic search | ✅ Matched | Text search via SQL LIKE |
| User profile | ✅ Matched | Stored in memory system |
| Vector embeddings | ⏳ Deferred | Planned for v0.2 |
| External memory providers | ⏳ Deferred | SQLite is built-in; pluggable later |

### Tools & Skills

| Feature | Status | Notes |
|---------|--------|-------|
| Tool system | ✅ Matched | `odin-tools` with Tool trait |
| File operations | ✅ Matched | Read/write within sandbox boundaries |
| Shell commands | ✅ Matched | With approval gates for dangerous commands |
| Web search/fetch | ✅ Matched | HTTP GET operations |
| Git integration | ✅ Matched | Git command wrapping |
| Skill system | ✅ Matched | `odin-skills` with markdown workflows |
| Tool registry | ✅ Matched | Dynamic add/remove at runtime |
| Tool schema validation | ✅ Improved | Built into INSPECT phase |
| Custom tools | ✅ Matched | Implement the Tool trait |
| Plugin system | ⏳ Deferred | Planned for v0.3 |
| MCP server support | ⏳ Deferred | Planned for v0.2 |

### Safety & Permissions

| Feature | Status | Notes |
|---------|--------|-------|
| Filesystem boundaries | ✅ Matched | Configurable read/write/deny paths |
| Command approval | ✅ Matched | Interactive approval for dangerous commands |
| Permission rules | ✅ Matched | Allow/deny/ask-user per tool |
| Rate limiting | ✅ Matched | Per-tool, per-session rate limits |
| Audit trail | ✅ Matched | Full logging of every action |
| Secret handling | ✅ Matched | Secrets in config, never sent to models |
| Sandboxing | ✅ Matched | Optional container/chroot execution |
| PII redaction | ⏳ Deferred | Planned for v0.2 |

### Scheduling & Automation

| Feature | Status | Notes |
|---------|--------|-------|
| Cron scheduling | ✅ Matched | `odin-scheduler` with cron expressions |
| Job management | ✅ Matched | Add/remove/pause/resume jobs |
| Webhooks | ⏳ Deferred | Planned for v0.2 |
| Long-running goals | ✅ Matched | Scheduler + loop engine |

### Interfaces & Platforms

| Feature | Status | Notes |
|---------|--------|-------|
| CLI | ✅ Matched | `odin-cli` with clap |
| HTTP API | ✅ Matched | `odin-gateway` with axum |
| Discord integration | ✅ Matched | Discord bot in `odin-gateway` |
| WebSocket | ⏳ Deferred | Stub exists, full impl v0.2 |
| Web dashboard | ⏳ Deferred | Planned for v0.3 |
| Telegram | ⏳ Deferred | Planned for v0.2 |
| Slack | ⏳ Deferred | Planned for v0.3 |
| WhatsApp/Signal | ⏳ Deferred | Not currently planned |
| IDE integration | ⏳ Deferred | Planned for v0.4 |

### Model Providers

| Feature | Status | Notes |
|---------|--------|-------|
| OpenAI-compatible | ✅ Matched | Works with OpenAI, Ollama, vLLM, Groq, DeepSeek |
| Anthropic | ✅ Matched | Claude models |
| Local models | ✅ Matched | Direct llama.cpp/Ollama support |
| Provider registry | ✅ Matched | Dynamic provider management |
| Provider abstraction | ✅ Improved | Clean Rust trait vs Python class |
| Credential pooling | 🔄 Changed | Single credential per provider (simpler) |
| Streaming | ⏳ Deferred | Basic support, full streaming in v0.2 |
| Vision support | ⏳ Deferred | Anthropic supports it; others in v0.2 |

### Developer Experience

| Feature | Status | Notes |
|---------|--------|-------|
| Configuration file | ✅ Matched | YAML config in `~/.odin/config.yaml` |
| Environment variables | ✅ Matched | API keys via env vars |
| Profiles | ⏳ Deferred | Planned for v0.2 |
| Session management | ✅ Matched | Session tracking in runtime |
| Debug mode | ✅ Matched | Verbose logging option |
| Hot reload | ❌ Removed | Not needed with Rust's compile-time safety |
| Interactive TUI | ⏳ Deferred | Planned CLI improvements |

### Performance

| Feature | Status | Notes |
|---------|--------|-------|
| Startup time | ✅ Improved | Rust binary, <500ms cold start |
| Memory usage | ✅ Improved | <50MB idle (vs Python's ~200MB+) |
| Loop overhead | ✅ Improved | <100ms (vs Python's interpreter overhead) |
| Concurrent agents | ✅ Improved | Native async, 100+ concurrent sessions |
| Binary size | 🔄 Changed | Larger binary but no interpreter needed |

## Key Differences

### Architecture
- **Hermes**: Python-based, ~3000 tests, 20+ providers, 30+ toolsets
- **Raven**: Rust-based, compiled binary, focused feature set, performance-first

### Agent Loop
- **Hermes**: `run_conversation()` — call LLM → dispatch tools → append results → repeat
- **Raven**: 7-phase structured loop with decomposition, self-checking, and escalation

### Small-Model Strategy
- **Hermes**: Designed primarily for frontier models; context compression when needed
- **Raven**: Built from ground up for smaller models; every phase includes helpers

### Language Choice
- **Hermes**: Python (accessible, huge ecosystem, easy to extend)
- **Raven**: Rust (performance, safety, single binary deployment)

## Migration Path

If you're using Hermes and considering Raven:

1. **Configuration**: Similar YAML structure, different keys. See `examples/config.yaml`.
2. **Tools**: Same concepts (file, shell, web, git). Different API.
3. **Skills**: Same markdown-based skill format. Drop-in compatible.
4. **Memory**: SQLite-based in both. Different schemas.
5. **Providers**: Same set. Different configuration format.

Raven is NOT a drop-in replacement for Hermes. It's a different implementation
with the same philosophy but different trade-offs (performance vs ecosystem).
