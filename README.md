# Raven AI Harness 🦅

**Next-generation AI agent harness in Rust.** Looped LLM logic designed for smaller/local/cheaper models.

> Inspired by [Hermes Agent](https://github.com/NousResearch/hermes-agent), reimagined in Rust with structured agent loops.

## Why Raven?

Most AI agent frameworks use a simple "call → tool → repeat" loop that works well with powerful models like Claude or GPT-4, but fails with smaller models. Raven wraps every model call in a structured **7-phase loop**:

```
PLAN → ACT → INSPECT → CRITIQUE → REVISE → VERIFY → CONTINUE/STOP
```

This helps smaller models succeed through:
- **Decomposition** — break complex goals into bite-sized sub-tasks
- **Self-checking** — every output is scored for confidence
- **State summaries** — compact context for limited windows
- **Retry with escalation** — retry, then escalate to stronger models only when needed
- **Verification** — validate results against success criteria

## Features

- 🦀 **Pure Rust** — zero Python/JS in the core runtime
- 🔄 **Looped agent engine** — 7-phase structured execution
- 🔌 **Provider-agnostic** — OpenAI-compatible, Anthropic, local models
- 🛠️ **Tool system** — file ops, shell, web, git with safety boundaries
- 🧠 **Persistent memory** — SQLite-backed, semantic search
- 📋 **Skill system** — reusable procedural workflows
- 🔒 **Safety-first** — permission engine, approval gates, audit trails
- ⏰ **Scheduler** — cron-style job scheduling
- 💬 **Discord gateway** — control agents from Discord
- 🌐 **HTTP API** — REST endpoints for integration
- 📊 **Audit logs** — full traceability of every action
- 🧪 **Thoroughly tested** — unit, integration, and benchmark tests
- 🚀 **High performance** — <50MB idle, <100ms overhead per turn

## Quick Start

### Prerequisites

- Rust 1.80+ (install via [rustup](https://rustup.rs))

### Install

```bash
# Clone the repo
git clone https://github.com/hermes-gadget/raven-ai-harness.git
cd raven-ai-harness

# Build
cargo build --release

# Run a task
cargo run -- run --goal "Create a hello world program in Python" --provider openai_compat

# Start the HTTP API
cargo run -- serve --port 9177
```

### Configuration

Create `~/.odin/config.yaml`:

```yaml
general:
  instance_name: raven
  log_level: info

models:
  default_provider: openai_compat
  default_model: gpt-4o-mini
  planning_model: gpt-4o-mini
  escalation_model: gpt-4o
  providers:
    openai_compat:
      provider_type: openai_compat
      base_url: https://api.openai.com/v1
      api_key_env: OPENAI_API_KEY
    anthropic:
      provider_type: anthropic
      api_key_env: ANTHROPIC_API_KEY
    local:
      provider_type: openai_compat
      base_url: http://localhost:11434/v1

agent:
  max_iterations: 100
  confidence_threshold: 0.5
  enable_decomposition: true
  enable_summarization: true
```

## Architecture

```
┌──────────────────────────────────────────────┐
│                 odin-cli (CLI)                │
├──────────────────────────────────────────────┤
│              odin-runtime (orchestrator)       │
├────┬────┬────┬────┬────┬────┬────┬───────────┤
│loop│prov│tool│mem │sched│perm│audit│ gateway  │
│eng │ider│s   │ory │uler │issi│     │ (Discord) │
│    │    │    │    │     │ons │     │           │
├────┴────┴────┴────┴────┴────┴────┴───────────┤
│                 odin-core (types)              │
└──────────────────────────────────────────────┘
```

See [ARCHITECTURE.md](ARCHITECTURE.md) for full details.

## The Looped Engine

The core innovation: a structured agent loop that helps smaller models succeed.

| Phase | What It Does | Small-Model Helper |
|-------|-------------|-------------------|
| **PLAN** | Decompose goal into sub-tasks | Heuristic decomposer |
| **ACT** | Execute tool or generate response | Schema validation |
| **INSPECT** | Examine results, update state | State summarizer |
| **CRITIQUE** | Self-evaluate, score confidence | Confidence scorer |
| **REVISE** | Retry with adjusted approach | Escalation manager |
| **VERIFY** | Check against success criteria | Schema validator |
| **DECIDE** | Continue, stop, or escalate | Decision logic |

## Hermes Compatibility

| Hermes Feature | Raven | Notes |
|---------------|-------|-------|
| Multi-agent | ✅ | odin-runtime |
| Persistent memory | ✅ | odin-memory (SQLite) |
| Tools/Skills | ✅ | odin-tools + odin-skills |
| Repo management | ✅ | Git tool integration |
| Task planning | ✅ Improved | Looped PLAN phase |
| Discord | ✅ | odin-gateway |
| GitHub workflows | ✅ | CI/CD + git tools |
| Cron scheduling | ✅ | odin-scheduler |
| Audit trails | ✅ | odin-audit |
| Safety controls | ✅ | odin-permissions |
| Provider abstraction | ✅ Improved | Clean Rust traits |
| Web dashboard | ⏳ v0.3 | Planned |
| Telegram/Slack/etc | ⏳ v0.2 | Discord first |
| Voice/STT/TTS | ❌ | Out of scope |

Full compatibility notes: [docs/hermes-compatibility.md](docs/hermes-compatibility.md)

## Benchmarks

| Metric | Target | Status |
|--------|--------|--------|
| Loop latency (overhead) | <100ms | ✅ |
| Idle memory | <50MB | ✅ |
| Concurrent sessions | 100+ | ✅ |
| Tool execution overhead | <5ms | ✅ |
| Startup time | <500ms | ✅ |

## Development

```bash
# Run all tests
cargo test --workspace

# Run benchmarks
cargo bench

# Check compilation
cargo check --workspace

# Build release
cargo build --release
```

## License

MIT — see [LICENSE](LICENSE).

## Acknowledgments

Inspired by [Hermes Agent](https://github.com/NousResearch/hermes-agent) by Nous Research.
