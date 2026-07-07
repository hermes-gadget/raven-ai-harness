# Raven AI Harness — Architecture

> **Status:** This document reflects the codebase as of 2026-07-07.

## Overview

Raven is a **next-generation AI agent harness written in Rust**, inspired by [Hermes Agent](https://github.com/NousResearch/hermes-agent). It provides a structured, looped agent execution engine designed to help smaller/cheaper/local models succeed through decomposition, self-checking, retry logic, and escalation.

The project is organized as a **Rust workspace** of 12 crates with a layered architecture:

```
┌──────────────────────────────────────────────┐
│              odin-cli (CLI)                    │
├──────────────────────────────────────────────┤
│            odin-runtime (orchestrator)         │
├────┬────┬────┬────┬────┬────┬────┬───────────┤
│loop│prov│tool│mem │sched│perm│audit│ gateway  │
│eng │ider│s   │ory │uler │issi│     │ (Discord) │
│    │    │    │    │     │ons │     │           │
├────┴────┴────┴────┴────┴────┴────┴───────────┤
│              odin-core (types)                 │
└──────────────────────────────────────────────┘
```

### Design Principles

- **Performance-first:** Rust native, <50MB idle, <100ms loop overhead
- **Small-model friendly:** Every phase includes helpers for weaker models
- **Safety by default:** Permission engine, approval gates, filesystem boundaries
- **Provider-agnostic:** Clean trait abstraction — OpenAI-compatible, Anthropic, local
- **Loosely coupled:** Each crate depends only on `odin-core` for shared types

---

## Crate Map

| Crate | Path | Description | Key Types / Exports | Dependencies (external) |
|-------|------|-------------|---------------------|------------------------|
| **odin-core** | `crates/odin-core/` | Foundation types, config, errors, traits. Minimal dependencies. | `OdinConfig`, `OdinError`, `Message`, `ToolCall`, `ToolSchema`, `AgentTask`, `TaskResult`, `LoopPhase`, `ConfidenceScore`, `StateSummary`, `Provider` trait, `Tool` trait, `LoopEngine` trait, `MemoryStore` trait, `AuditLogger` trait, `PermissionEngine` trait | serde, thiserror, chrono, uuid, strum |
| **odin-cli** | `crates/odin-cli/` | Command-line interface via clap. Entry point for the binary. | `Cli` (clap Parser), `Commands` (Run/Serve/Config/Version) | clap, tracing-subscriber, odin-core, odin-runtime, odin-gateway |
| **odin-runtime** | `crates/odin-runtime/` | Session management, agent lifecycle, sub-agent spawning. | `Runtime`, `Agent`, `Session`, `RuntimeSummary` | dashmap, odin-core, uuid |
| **odin-loop** | `crates/odin-loop/` | The core 7-phase agent loop engine. Plugs into the Runtime via the `LoopEngine` trait. | `LoopEngine` (engine::Engine), `ConfidenceScorer`, `GoalDecomposer`, `StateSummarizer`, phase modules (Plan, Act, Inspect, Critique, Revise, Verify, Decide) | odin-core, tokio, tracing |
| **odin-providers** | `crates/odin-providers/` | Model provider implementations. | `ProviderRegistry`, `OpenAiCompatProvider`, `AnthropicProvider`, `LocalProvider`, `ProviderExt` | reqwest, serde_json, odin-core |
| **odin-tools** | `crates/odin-tools/` | Tool system: registry, sandbox, built-in tools. | `ToolRegistry`, `Sandbox`, `PathBoundary` enforcement | odin-core, tokio |
| **odin-memory** | `crates/odin-memory/` | Persistent memory backed by SQLite. | `SqliteMemoryStore` (implements `MemoryStore` trait) | sqlx, odin-core |
| **odin-scheduler** | `crates/odin-scheduler/` | Cron-like job scheduling. | `Scheduler`, `Job`, `Schedule`, `CronField` | tokio, chrono, odin-core |
| **odin-permissions** | `crates/odin-permissions/` | Safety engine: policy, approval, secrets. | `PolicyEngine`, `ApprovalGate`, `SecretManager` | odin-core |
| **odin-audit** | `crates/odin-audit/` | Audit trail logging (file + SQLite). | `AuditLoggerImpl` (implements `AuditLogger` trait) | odin-core, sqlx, serde_json |
| **odin-gateway** | `crates/odin-gateway/` | External interfaces: HTTP API, Discord bot (stub), WebSocket (stub). | `run_http_server`, `GatewayState`, `ChatRequest`/`ChatResponse` | axum, tower-http, tokio, odin-runtime |
| **odin-skills** | `crates/odin-skills/` | Reusable markdown workflow skills. | `Skill`, `SkillFrontmatter`, `SkillRegistry` | odin-core |
| **odin-baseline** | `crates/odin-baseline/` | Naive single-pass agent for comparison benchmarks. | `BaselineAgent` (implements `LoopEngine`) | odin-core, async-trait |

---

## Data Flow

The typical request flow through the system:

```
User (CLI / HTTP / future UI)
        │
        ▼
   odin-cli ──► odin-runtime ──► odin-loop (LoopEngine)
                                     │
                          ┌──────────┼──────────┐
                          ▼          ▼          ▼
                   odin-providers  odin-tools  odin-memory
                          │          │
                          ▼          ▼
                    OpenAI/Anthropic  file ops, shell,
                    /Local models     web, git
                          │
                          ▼
                    odin-audit (every action logged)
                          │
                          ▼
                    odin-permissions (checked at runtime)
```

### Step-by-step

1. **CLI** (`odin run <goal>`) or **HTTP API** (`POST /chat`) receives a goal
2. **Runtime** creates an `AgentTask`, looks up or creates a `Session`, dispatches to the registered `LoopEngine`
3. **LoopEngine** (odin-loop) executes the 7-phase loop, calling providers for LLM completions and tools for actions
4. **Providers** (odin-providers) wrap API calls to OpenAI, Anthropic, or local models
5. **Tools** (odin-tools) execute file operations, shell commands, web fetches, git operations — each checked against permissions
6. **Memory** (odin-memory) persists facts, preferences, and patterns across sessions
7. **Audit** (odin-audit) records every action and decision for traceability
8. **Result** flows back up through Runtime → CLI/HTTP response

---

## The Looped Engine

The core innovation: a structured 7-phase agent loop designed to help smaller models succeed.

### Seven Phases

```
PLAN → ACT → INSPECT → CRITIQUE → REVISE → VERIFY → DECIDE
  │                                                       │
  └────────────────────── cycle ──────────────────────────┘
```

| Phase | What It Does | Small-Model Helper |
|-------|-------------|-------------------|
| **PLAN** | Decompose the goal into sub-tasks, define success criteria | `GoalDecomposer` — heuristic decomposer for complex goals |
| **ACT** | Execute tools or generate a response | Schema validation helps models produce valid tool calls |
| **INSPECT** | Examine tool results, update internal state | `StateSummarizer` compacts context for small windows |
| **CRITIQUE** | Score the output for confidence and completeness | `ConfidenceScorer` evaluates output quality (0.0–1.0) |
| **REVISE** | If confidence is low, retry with adjusted approach | `EscalationManager` retries locally, then escalates to stronger models |
| **VERIFY** | Check results against success criteria | Schema validator ensures structured output is correct |
| **DECIDE** | Continue the loop, stop, or escalate | Decision logic based on confidence × completion status |

### Confidence Scoring

Every output is scored on a 0.0–1.0 scale:
- `≥ 0.8` → High confidence, proceed
- `0.5–0.8` → Moderate confidence, consider revise
- `< 0.5` → Low confidence, escalate to stronger model

### Escalation Strategy

When confidence is low:
1. **Retry** same model with more context (up to `max_retries`)
2. **Escalate** to the `escalation_model` (stronger/expensive model)
3. **Fail** if escalation also fails

### State Management

The `LoopState` carries:
- The current task and its sub-tasks
- Full message history (`Vec<Message>`)
- Tool results and phase records
- Iteration count and retry count
- Current phase

When the context approaches `context_limit` tokens, the `COMPRESS` phase automatically summarizes to `compression_ratio` of the limit.

---

## Provider Architecture

### Trait Structure

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    async fn list_models(&self) -> OdinResult<Vec<ModelInfo>>;
    async fn chat(&self, model: &str, messages: &[Message],
                  tools: &[ToolSchema], options: &CompletionOptions) -> OdinResult<ChatResponse>;
    async fn chat_stream(&self, model: &str, messages: &[Message],
                          tools: &[ToolSchema], options: &CompletionOptions)
                          -> OdinResult<Box<dyn ChatStream>>;
    async fn health_check(&self) -> OdinResult<bool>;
}
```

### Provider Registry

`ProviderRegistry` in `odin-providers` manages dynamic provider registration and lookup by name. Providers are registered at startup from the configuration.

### Supported Providers

| Provider | Type | Configuration |
|----------|------|--------------|
| OpenAI-compatible | `openai_compat` | `base_url` + `api_key` or `api_key_env` |
| Anthropic | `anthropic` | `api_key_env: ANTHROPIC_API_KEY` |
| Local | `local` | `base_url: http://localhost:11434/v1` (Ollama) |

Adding a provider requires implementing the `Provider` trait and registering it in `ProviderRegistry`.

---

## Safety Model

### Layers

1. **Filesystem Boundaries** (`PathBoundary`)
   - Configurable read/write/deny paths
   - Default denies `/etc/passwd`, `/etc/shadow`, `~/.ssh`

2. **Command Approval** (`ApprovalGate`)
   - Regex patterns match dangerous commands (e.g., `rm -rf`, `sudo`, `dd if=`)
   - Each dangerous command requires interactive user approval

3. **Permission Rules** (`PolicyEngine`)
   - Per-tool: Allow / Deny / AskUser
   - Per-tool rate limiting (calls per minute)

4. **Secrets Management** (`SecretManager`)
   - API keys stored in config, never sent to models
   - Environment variable indirection (`api_key_env`)

5. **Sandbox** (`Sandbox` in odin-tools)
   - Optional container/chroot execution for high-risk operations

### Audit Trail

Every action is logged through `AuditLoggerImpl`:
- Tool calls (name, arguments, result, duration)
- Model calls (prompt tokens, completion tokens)
- Decisions (continue, stop, escalate)
- Permission checks (allowed, denied, approved)
- Session lifecycle events

---

## Configuration

Configuration is loaded from a YAML file (default `~/.odin/config.yaml` or `odin.yaml`/`odin.yml` in the working directory). The `OdinConfig` struct mirrors the YAML structure.

### Config Sections

| Section | Struct | Key Features |
|---------|--------|-------------|
| `general` | `GeneralConfig` | `instance_name`, `data_dir`, `log_level`, `debug` |
| `models` | `ModelsConfig` | `default_provider`, `default_model`, `planning_model`, `critique_model`, `escalation_model`, `providers` map |
| `agent` | `AgentConfig` | `max_iterations`, `max_tool_calls_per_turn`, `confidence_threshold`, `enable_decomposition`, `enable_summarization`, `context_limit`, `compression_ratio` |
| `tools` | `ToolsConfig` | `enabled`, `disabled`, `default_timeout_secs`, `path_boundary`, `sandbox_enabled`, `tool_dirs` |
| `memory` | `MemoryConfig` | `enabled`, `db_path`, `max_entries`, `save_interval_secs` |
| `safety` | `SafetyConfig` | `require_approval`, `dangerous_commands`, `max_rate_per_minute`, `permissions` |
| `audit` | `AuditConfig` | `enabled`, `log_path`, `json_format` |
| `gateway` | `GatewayConfig` | `http_enabled`, `http_addr`, `discord_enabled`, `discord_token`, `discord_token_env` |
| `scheduler` | `SchedulerConfig` | `enabled`, `check_interval_secs`, `max_concurrent` |

### Environment Variables

- `ODIN_CONFIG` — Path to config file
- `ODIN_HTTP_ADDR` — HTTP API listen address
- `OPENAI_API_KEY`, `ANTHROPIC_API_KEY` — Provider API keys (via `api_key_env`)
- `RUST_LOG` — Tracing/logging level (default: `info`)

See [examples/config.yaml](../examples/config.yaml) for a complete configuration example.

---

## CLI Commands

| Command | Description |
|---------|-------------|
| `odin run <goal>` | Execute a task through the agent loop |
| `odin serve [--addr]` | Start the HTTP API server |
| `odin config [--show|--edit]` | View or edit configuration |
| `odin version` | Show version and crate information |

---

## Gateway Interfaces

| Interface | Status | Notes |
|-----------|--------|-------|
| HTTP API | ✅ Functional | axum-based REST API on configurable port (default 127.0.0.1:9177) |
| Discord | ⚠️ Stub exists, needs wiring | Module scaffold in odin-gateway, not connected to runtime |
| WebSocket | ⚠️ Stub exists | Module scaffold in odin-gateway, not fully implemented |

---

## Development

```bash
# Build all crates
cargo build --workspace

# Run all tests
cargo test --workspace

# Check compilation
cargo check --workspace

# Lint
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check

# Build release binary
cargo build --release
```
