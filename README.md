# Raven Agent 🦅

**Multi-agent orchestration platform in Rust.** The composer/orchestrator delegates work to hidden sub-agents automatically, with structured looped LLM logic designed for smaller/local/cheaper models.

> Inspired by [Hermes Agent](https://github.com/NousResearch/hermes-agent), reimagined in Rust with multi-agent orchestration.

## Why Raven?

Most AI agent frameworks use a simple "call → tool → repeat" loop that works well with powerful models like Claude or GPT-4, but fails with smaller models. Raven wraps every model call in a structured **7-phase loop**, and splits complex work across **parallel sub-agents**:

```
User → Composer → Task Graph → Sub-Agents (parallel) → Results → Merge → User
            ↑         ↑              ↑
         Intent    File Locks    Lifecycle (queued→running→done)
```

This helps smaller models succeed through:
- **Multi-agent orchestration** — one request spawns many sub-agents automatically
- **Task graph execution** — parent goal → sub-goals → parallel agents → merged output
- **File locking** — safe concurrent edits with queue and merge resolution
- **Decomposition** — break complex goals into bite-sized sub-tasks
- **Self-checking** — every output is scored for confidence
- **State summaries** — compact context for limited windows
- **Retry with escalation** — retry, then escalate to stronger models only when needed
- **Verification** — validate results against success criteria
- **Skills** — reusable markdown workflows loaded and injected automatically

## Features

- 🦀 **Pure Rust** — zero Python/JS in the core runtime
- 🔄 **Looped agent engine** — 7-phase structured execution
- 🤖 **Multi-agent orchestration** — Composer auto-delegates to parallel sub-agents
- 📊 **Task graph** — parent goals → sub-goals → agents → files/tools → outputs
- 🔒 **File locking** — safe concurrent edits with queue and merge resolution
- 🔌 **Provider-agnostic** — OpenAI-compatible, Anthropic, local models, DeepSeek
- 🔗 **Fallback chains** — weak→local→escalation with circuit breakers and health checks
- 🛠️ **Tool system** — file ops, shell, web, git with safety boundaries
- 📋 **Skill system** — reusable markdown-based workflows, auto-injected into agent context
- 🧠 **Persistent memory** — SQLite-backed, semantic search
- 🔒 **Safety-first** — permission engine, approval gates, secret redaction, audit trails
- ⏰ **Persistent scheduler** — cron-style jobs survive restart via SQLite
- 💬 **Discord gateway** — real serenity 0.12 integration with slash commands and permission gating
- 🔌 **WebSocket gateway** — real-time task progress and events
- 🌐 **HTTP API** — REST endpoints for integration
- 📊 **Audit logs** — full traceability of every action
- 🧪 **Thoroughly tested** — 300+ tests and growing
- 🚀 **High performance** — <50MB idle, <100ms overhead per turn

## Quick Start

### Prerequisites

- Rust 1.80+ (install via [rustup](https://rustup.rs))

### Install

```bash
# Clone the repo
git clone https://github.com/hermes-gadget/raven-agent.git
cd raven-agent

# Build
cargo build --release

# Orchestrate a goal with hidden sub-agents (default mode)
cargo run -- orchestrate submit "fix the CLI bug, improve docs, add tests"

# Or use the run command (orchestrated by default)
cargo run -- run "fix the CLI bug, improve docs, add tests for scheduler, and check provider fallback"

# Direct single-agent execution (legacy mode)
cargo run -- run --direct "Create a hello world program in Python"

# Start the HTTP API (also enables WebSocket at /ws)
cargo run -- serve

# List available skills
cargo run -- skills list

# Manage orchestration
cargo run -- orchestrate status
cargo run -- orchestrate agents
cargo run -- orchestrate locks
cargo run -- orchestrate queue

# List configured providers
cargo run -- providers list

# Manage cron jobs (persistent via SQLite)
cargo run -- schedule add "daily-report" "0 9 * * *" "Summarize recent changes"
cargo run -- schedule list
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
  escalation_model: gpt-4o
  providers:
    openai_compat:
      provider_type: openai_compat
      base_url: https://api.openai.com/v1
      api_key_env: OPENAI_API_KEY
      fallback_chain: [anthropic, local]
      circuit_breaker_threshold: 3
    anthropic:
      provider_type: anthropic
      api_key_env: ANTHROPIC_API_KEY
    local:
      provider_type: openai_compat
      base_url: http://localhost:11434/v1

agent:
  max_iterations: 100
  enable_decomposition: true
  skills_dir: ~/.odin/skills

gateway:
  http_enabled: true
  discord_enabled: false
  ws_enabled: false

scheduler:
  enabled: false
  check_interval_secs: 30
```

See [examples/config.yaml](examples/config.yaml) for the full annotated configuration.

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│                    odin-cli (CLI)                         │
├──────────────────────────────────────────────────────────┤
│           odin-orchestrator (composer + task graph)       │
├──────────────────────────────────────────────────────────┤
│              odin-runtime (sub-agent pool)                 │
├────┬────┬────┬────┬────┬────┬────┬───────────────┬────────┤
│loop│prov│tool│mem │sched│perm│audit│  gateway     │ file   │
│eng │ider│s   │ory │uler │issi│     │ HTTP+WS+Disc │ locks  │
├────┴────┴────┴────┴────┴────┴────┴───────────────┴────────┤
│              odin-skills (markdown workflows)              │
├──────────────────────────────────────────────────────────┤
│                 odin-core (types)                          │
└──────────────────────────────────────────────────────────┘
```

See [ARCHITECTURE.md](ARCHITECTURE.md) for full details.

## The Looped Engine

The core innovation: a structured agent loop that helps smaller models succeed.

| Phase | What It Does | Small-Model Helper |
|-------|-------------|-------------------|
| **PLAN** | Decompose goal into sub-tasks, inject skills | Heuristic decomposer |
| **ACT** | Execute tool or generate response | Schema validation |
| **INSPECT** | Examine results, update state | State summarizer |
| **CRITIQUE** | Self-evaluate, score confidence | Confidence scorer |
| **REVISE** | Retry with adjusted approach | Escalation manager |
| **VERIFY** | Check against success criteria | Schema validator |
| **DECIDE** | Continue, stop, or escalate | Decision logic |

## CLI Commands

| Command | Description |
|---------|-------------|
| `odin run <task>` | Execute a task through the agent loop |
| `odin orchestrate <goal>` | Orchestrate with sub-agents (default mode) |
| `odin serve` | Start HTTP + WebSocket API server |
| `odin schedule add/list/remove/enable/disable` | Manage persistent cron jobs |
| `odin skills list|tools` | List loaded skills with tool associations |
| `odin tools list|inspect|validate|test|doctor|catalog|reliability` | Tool ecosystem management |
| `odin providers list` | Show configured providers with health |
| `odin config [--edit]` | View or edit configuration |
| `odin status` | Runtime summary |
| `odin version` | Show version information |

## Hermes Compatibility

| Hermes Feature | Raven Agent | Notes |
|---------------|------------|-------|
| Multi-agent orchestration | ✅ Enhanced | Composer+TaskGraph auto-delegates to parallel sub-agents |
| Persistent memory | ✅ | odin-memory (SQLite) |
| Tools/Skills | ✅ | odin-tools + odin-skills (wired) |
| Task planning | ✅ Improved | Looped PLAN phase + task graph decomposition |
| File locking | ✅ New | FileLockManager with queue + merge resolution |
| Discord | ✅ | serenity 0.12, slash commands |
| WebSocket | ✅ | Real Axum WS, live task updates |
| GitHub workflows | ✅ | CI/CD + git tools |
| Cron scheduling | ✅ | odin-scheduler (SQLite persistent) |
| Audit trails | ✅ | odin-audit (full lifecycle audit) |
| Safety controls | ✅ | odin-permissions (redaction, approval, commands) |
| Provider abstraction | ✅ Improved | Clean Rust traits + fallback chains |
| Web dashboard | ⏳ v0.3 | Planned |
| Telegram/Slack/etc | ⏳ v0.2+ | Discord first |

Full compatibility notes: [docs/hermes-compatibility.md](docs/hermes-compatibility.md)

## Development

```bash
# Run all tests
cargo test --workspace

# Run benchmarks
cargo bench

# Check compilation
cargo check --workspace

# Lint
cargo clippy --workspace -- -D warnings

# Format
cargo fmt --all -- --check

# Build release
cargo build --release
```

## License

MIT — see [LICENSE](LICENSE).

## Acknowledgments

Inspired by [Hermes Agent](https://github.com/NousResearch/hermes-agent) by Nous Research. Raven Agent builds on their multi-agent vision with structured loops, task graphs, file locking, and automatic sub-agent orchestration in Rust.
