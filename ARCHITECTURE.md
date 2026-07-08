# Raven Agent — Architecture

> **Multi-agent orchestration platform in Rust.** Composer delegates to parallel sub-agents with structured looped LLM logic for smaller/local/cheaper models.
> Multi-agent, task graphs, file locking, persistent memory, tools/skills, safety-first. Inspired by Hermes.

## Design Philosophy

Raven Agent is designed from the ground up for **smaller, cheaper, local models** running as **specialized sub-agents**. The Composer/Orchestrator receives user intent, decomposes it into a task graph, spawns sub-agents with scoped tools and files, and merges results. Unlike single-pass agent loops that expect a powerful model to get everything right in one shot, the orchestrator breaks work into isolated, parallel, independently-verifiable units of work.

Small models succeed because the orchestrator:
1. **Decomposes** complex goals into bite-sized sub-tasks in a task graph
2. **Scopes** each sub-agent to only the files, tools, and context it needs
3. **Locks** files so parallel agents don't clobber each other's work
4. **Maintains state summaries** to keep context windows small
5. **Self-checks** every output before committing
6. **Retries** with escalating strategies on failure
7. **Scores confidence** and escalates to stronger models only when needed
8. **Verifies** tool outputs against expected schemas

## Crate Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        odin-cli (binary)                         │
│                  CLI / TUI / Gateway entrypoint                  │
└──────────────────────────┬──────────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────────┐
│               odin-orchestrator (composer + task graph)          │
│   Intent intake, task graph decomposition, sub-agent steering,   │
│   file lock management, merge resolution, lifecycle tracking     │
└──────────────────────────┬──────────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────────┐
│                odin-runtime (sub-agent pool + execution)          │
│    Agent lifecycle, session management, parallel execution       │
└──┬────────┬────────┬────────┬────────┬────────┬────────┬────────┘
   │        │        │        │        │        │        │
   ▼        ▼        ▼        ▼        ▼        ▼        ▼
┌──────┐┌──────┐┌──────┐┌──────┐┌──────┐┌──────┐┌──────────┐
│ loop ││provid││tools ││memory││sched ││permis││ audit    │
│engine││ -ers ││skills││      ││ -uler││ -sions││ gateway  │
└──┬───┘└──┬───┘└──┬───┘└──┬───┘└──┬───┘└──┬───┘└────┬─────┘
   │       │       │       │       │       │         │
   └───────┴───────┴───────┴───────┴───────┴─────────┘
                           │
┌──────────────────────────▼──────────────────────────────────────┐
│                       odin-core                                  │
│        Shared types, config, error types, traits, constants       │
└─────────────────────────────────────────────────────────────────┘
```

### Crate Responsibilities

| Crate | Purpose | Key Types |
|-------|---------|-----------|
| `odin-core` | Foundation: types, config, errors, traits | `AgentTask`, `ToolResult`, `ModelConfig`, `OdinError` |
| `odin-orchestrator` | Composer: task graph, sub-agent steering, file locks | `Composer`, `TaskGraph`, `FileLockManager`, `MergeResolver` |
| `odin-runtime` | Sub-agent pool, execution, lifecycle | `Runtime`, `Session`, `Agent`, `AgentState` |
| `odin-loop` | The looped agent engine — the key innovation | `LoopEngine`, `Phase`, `ConfidenceScore`, `StateSummary` |
| `odin-providers` | Abstract model provider layer | `Provider`, `OpenAiCompatProvider`, `AnthropicProvider` |
| `odin-tools` | Tool system with safety boundaries | `Tool`, `ToolRegistry`, `Sandbox`, `ToolSchema` |
| `odin-skills` | Procedural knowledge / reusable workflows | `Skill`, `SkillRegistry`, `SkillTemplate` |
| `odin-memory` | Persistent memory across sessions | `MemoryStore`, `MemoryEntry`, `VectorIndex` |
| `odin-scheduler` | Cron-style task scheduling | `Scheduler`, `Job`, `Schedule` |
| `odin-permissions` | Safety controls, command approval | `Policy`, `ApprovalGate`, `SecretManager` |
| `odin-audit` | Audit logging and traceability | `AuditLog`, `AuditEntry`, `Trace` |
| `odin-gateway` | Discord, HTTP API, WebSocket interfaces | `Gateway`, `DiscordAdapter`, `ApiServer` |
| `odin-runtime` | Orchestrator: agent lifecycle, session mgmt | `Runtime`, `Session`, `Agent` |
| `odin-cli` | CLI entrypoint binary | `main.rs`, `Args` |

## The Looped Agent Engine (odin-loop)

This is Raven's core innovation. Instead of a simple "call LLM → get tools → execute → repeat" loop, Raven uses 7 structured phases:

```
┌──────────────────────────────────────────────────────────────────┐
│                        AGENT LOOP                                 │
│                                                                   │
│  ┌────────┐   ┌────────┐   ┌──────────┐   ┌──────────┐          │
│  │  PLAN  │──▶│  ACT   │──▶│ INSPECT  │──▶│ CRITIQUE │          │
│  └────────┘   └────────┘   └──────────┘   └──────────┘          │
│       ▲                                        │                  │
│       │                                        ▼                  │
│  ┌────────┐   ┌──────────┐   ┌──────────┐                        │
│  │CONTINUE│◀──│  VERIFY  │◀──│  REVISE  │                        │
│  │ /STOP  │   └──────────┘   └──────────┘                        │
│  └────────┘                                                       │
│                                                                   │
│  Small-model helpers at each phase:                               │
│  • Decomposition of complex goals (PLAN)                          │
│  • State summaries to stay within context window (INSPECT)        │
│  • Schema validation of tool outputs (INSPECT)                    │
│  • Self-check and confidence scoring (CRITIQUE)                   │
│  • Retry with escalating strategies (REVISE)                      │
│  • Verification against expected outcome (VERIFY)                 │
│  • Escalation to stronger model when confidence < threshold       │
└──────────────────────────────────────────────────────────────────┘
```

### Phase Details

1. **PLAN** — The model (or a lightweight planning model) decomposes the goal into concrete steps. For small models, this includes explicit reasoning chains and success criteria.

2. **ACT** — Execute the planned action. This may be a tool call, generating text, or making a decision. Tool calls are validated against schemas before execution.

3. **INSPECT** — Examine what happened. Parse tool outputs, validate against expected schemas, compute state diffs, update the state summary. This keeps context windows small by summarizing, not accumulating.

4. **CRITIQUE** — The model self-evaluates its action. Did it achieve what was planned? Is there an error? Compute a confidence score (0.0–1.0). If confidence is below threshold, flag for revision.

5. **REVISE** — If critique found issues, revise the approach. This may mean retrying with different parameters, asking for clarification, or escalating to a stronger model.

6. **VERIFY** — Final verification against success criteria. Did we actually accomplish the step? Is the output valid? For tool calls, did the tool succeed?

7. **CONTINUE/STOP** — Decision point. If more steps remain and confidence is high enough, loop back to PLAN. If done or stuck, stop and return results.

### Small-Model Helpers

| Helper | Purpose | Phase |
|--------|---------|-------|
| Goal Decomposer | Break complex goals into atomic sub-tasks | PLAN |
| State Summarizer | Compress conversation history into compact state | INSPECT |
| Schema Validator | Validate tool inputs/outputs against JSON schemas | INSPECT, VERIFY |
| Confidence Scorer | 0.0–1.0 score of how well the action achieved its goal | CRITIQUE |
| Retry Strategist | Escalating retry: same params → different params → different model → human | REVISE |
| Escalation Manager | Route to stronger model when confidence is low | REVISE |
| Context Compressor | Keep within token limits by summarizing old turns | All phases |

## Safety Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     PERMISSION BOUNDARY                       │
│                                                               │
│  ┌──────────┐    ┌──────────────┐    ┌──────────────────┐    │
│  │  Policy  │───▶│ ApprovalGate │───▶│  SandboxExecutor  │   │
│  │  Engine  │    │ (interactive)│    │ (container/chroot)│   │
│  └──────────┘    └──────────────┘    └──────────────────┘    │
│        │                │                      │              │
│        ▼                ▼                      ▼              │
│  ┌──────────┐    ┌──────────────┐    ┌──────────────────┐    │
│  │  Rules   │    │ Human-in-the │    │  Filesystem       │    │
│  │  (allow/ │    │ -loop for    │    │  Boundaries       │    │
│  │  deny)   │    │ destructive  │    │  (read-only by    │    │
│  │          │    │ commands     │    │   default)        │    │
│  └──────────┘    └──────────────┘    └──────────────────┘    │
│                                                               │
│  ┌──────────────────────────────────────────────────────┐    │
│  │                   AUDIT LOG                            │    │
│  │  Every action, decision, and tool call is logged with: │    │
│  │  • Timestamp  • Agent ID  • Action  • Result           │    │
│  │  • Policy decision  • Approval chain  • Diff of state  │    │
│  └──────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

### Safety Controls

| Control | Description |
|---------|-------------|
| Filesystem boundaries | Tools run in a configurable root directory; cannot escape |
| Command approval | Destructive commands require interactive approval |
| Secret handling | Secrets stored in encrypted config; never passed to models |
| Rate limiting | Per-provider, per-tool, per-session rate limits |
| Audit trail | Every action logged with full context for debugging/review |
| Permission model | Fine-grained allow/deny rules per tool, per agent, per session |
| Sandboxing | Optional container/chroot execution for untrusted tool calls |

## Provider Abstraction

```rust
/// Every model provider implements this trait.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Unique provider identifier (e.g., "openai", "anthropic", "ollama")
    fn name(&self) -> &str;

    /// List available models for this provider
    async fn list_models(&self) -> Result<Vec<ModelInfo>>;

    /// Send a chat completion request
    async fn chat(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[ToolSchema],
        options: &CompletionOptions,
    ) -> Result<ChatResponse>;

    /// Stream a chat completion (returns a Stream of deltas)
    async fn chat_stream(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[ToolSchema],
        options: &CompletionOptions,
    ) -> Result<ChatStream>;

    /// Check if the provider is healthy
    async fn health_check(&self) -> Result<bool>;
}
```

Supported providers (initially):
- **OpenAI-compatible** — works with OpenAI, Ollama, vLLM, LM Studio, Groq, DeepSeek, local models
- **Anthropic** — Claude models via Anthropic API
- **Local** — Direct integration with llama.cpp, mistral.rs

## Hermes Feature Compatibility

| Hermes Feature | Raven Status | Notes |
|----------------|-------------|-------|
| Multi-agent task execution | ✅ Matched | odin-runtime orchestrates sub-agents |
| Persistent memory | ✅ Matched | odin-memory with SQLite + vector index |
| Tool/skill support | ✅ Matched | odin-tools + odin-skills |
| Repo/workspace management | ✅ Matched | Built into runtime with git integration |
| Task planning | ✅ Improved | Looped PLAN phase with decomposition |
| Logging | ✅ Matched | odin-audit with structured tracing |
| Discord/control interface | ✅ Matched | odin-gateway with Discord adapter |
| GitHub workflow support | ✅ Matched | Tool integrations + CI/CD |
| Long-running goal execution | ✅ Matched | odin-scheduler with persistent goals |
| Audit trails | ✅ Matched | odin-audit with full traceability |
| Safe permission boundaries | ✅ Matched | odin-permissions with approval gates |
| Model provider abstraction | ✅ Improved | Clean Rust trait, easier to add providers |
| Profile system | ✅ Planned | Multi-profile support in v0.2 |
| Cron scheduling | ✅ Matched | odin-scheduler |
| Web dashboard | ⏳ Deferred | Planned for v0.3 |
| Platform gateways (Telegram, etc.) | ⏳ Deferred | Discord first, others planned |
| MCP server support | ⏳ Deferred | Planned for v0.2 |
| Context compression | ✅ Improved | Built into the loop engine phases |
| Subagent delegation | ✅ Matched | odin-runtime spawns agents |
| Credential pooling | ⏳ Deferred | Simpler single-credential model first |
| Voice/STT/TTS | ❌ Removed | Out of scope for initial release |
| Browser automation | ⏳ Deferred | Planned for v0.3 |

## Performance Targets

| Metric | Target |
|--------|--------|
| Agent loop latency (single turn) | < 100ms overhead + model inference time |
| Memory usage (idle) | < 50 MB |
| Concurrent agent sessions | 100+ |
| Tool execution overhead | < 5ms |
| Audit log write latency | < 1ms |
| Startup time | < 500ms |

## Directory Structure

```
raven-ai-harness/
├── Cargo.toml                  # Workspace manifest
├── Cargo.lock
├── README.md
├── ARCHITECTURE.md             # This file
├── LICENSE
├── Makefile
├── rust-toolchain.toml
├── .github/
│   └── workflows/
│       ├── ci.yml              # Build, test, lint, audit
│       └── release.yml         # Release builds
├── crates/
│   ├── odin-core/              # Foundation types
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── types.rs        # AgentTask, ToolResult, Message, etc.
│   │       ├── config.rs       # Configuration types
│   │       ├── error.rs        # Error types
│   │       └── traits.rs       # Core traits
│   ├── odin-loop/              # Looped agent engine
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── engine.rs       # LoopEngine
│   │       ├── phases.rs       # Plan, Act, Inspect, Critique, Revise, Verify
│   │       ├── confidence.rs   # Confidence scoring
│   │       ├── decomposer.rs   # Goal decomposition
│   │       └── summarizer.rs   # State summarization
│   ├── odin-providers/         # Model provider abstraction
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── traits.rs       # Provider trait
│   │       ├── openai_compat.rs
│   │       ├── anthropic.rs
│   │       ├── local.rs
│   │       └── registry.rs     # Provider registry
│   ├── odin-tools/             # Tool system
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── tool.rs         # Tool trait + ToolSchema
│   │       ├── registry.rs     # ToolRegistry
│   │       ├── sandbox.rs      # Sandboxed execution
│   │       └── builtins/       # Built-in tools
│   │           ├── mod.rs
│   │           ├── file.rs
│   │           ├── shell.rs
│   │           ├── web.rs
│   │           └── git.rs
│   ├── odin-skills/            # Skill system
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── skill.rs
│   │       └── registry.rs
│   ├── odin-memory/            # Persistent memory
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── store.rs
│   │       └── models.rs
│   ├── odin-scheduler/         # Task scheduling
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── scheduler.rs
│   │       └── job.rs
│   ├── odin-permissions/       # Safety controls
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── policy.rs
│   │       ├── approval.rs
│   │       └── secrets.rs
│   ├── odin-audit/             # Audit logging
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       └── logger.rs
│   ├── odin-gateway/           # External interfaces
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── http.rs
│   │       ├── discord.rs
│   │       └── ws.rs
│   ├── odin-runtime/           # Orchestrator
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── runtime.rs
│   │       ├── session.rs
│   │       └── agent.rs
│   └── odin-cli/               # CLI binary
│       ├── Cargo.toml
│       └── src/
│           └── main.rs
├── tests/                      # Integration tests
│   ├── integration/
│   │   ├── loop_engine_tests.rs
│   │   ├── provider_tests.rs
│   │   ├── tool_tests.rs
│   │   ├── full_agent_run.rs
│   │   └── failure_retry_tests.rs
│   └── common/
│       └── mod.rs
├── benches/                    # Benchmarks
│   ├── loop_bench.rs
│   ├── provider_bench.rs
│   └── tool_bench.rs
├── examples/                   # Example configurations
│   ├── config.yaml
│   ├── simple_agent.rs
│   └── discord_bot.rs
└── docs/
    ├── getting-started.md
    ├── configuration.md
    ├── loop-strategy.md
    ├── tools-and-skills.md
    ├── providers.md
    ├── safety.md
    └── hermes-compatibility.md
```
