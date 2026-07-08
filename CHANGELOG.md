# Changelog

All notable changes to Raven Agent.

## [0.3.0] — 2026-07-08

### Added
- **Multi-agent orchestration**: `odin-orchestrator` crate with Composer, TaskGraph, FileLockManager, MergeResolver
- **Composer pattern**: User-facing agent that auto-delegates work to hidden sub-agents
- **Task graph**: parent goal → sub-goals → agents → files/tools → outputs with topological validation
- **File-level locking**: FileLockManager prevents concurrent edit conflicts with queue and merge resolution
- **Agent lifecycle**: Full state machine (queued→running→blocked→waiting_for_lock→reviewing→done→failed→cancelled)
- **Parallel execution**: Unrelated workstreams detected and spawned as parallel sub-agents
- **Interruption handling**: Pause, cancel, redirect, reprioritize sub-agents mid-execution
- **Persistent orchestration**: Task graph and agent lifecycle survive restart via SQLite
- **Progress tracking**: CLI, API, Discord, WebSocket support for orchestration status
- **Project rename**: `raven-ai-harness` → `raven-agent` across all docs, config, and code

### Changed
- Architecture: odin-orchestrator layer sits above odin-runtime
- Default execution mode: `odin orchestrate` delegates to sub-agents by default
- Rust edition 2024 with workspace resolver 2

## [0.2.0] — 2026-07-07

### Added
- **Skills execution**: Markdown skills loaded from disk, injected into PLAN phase, queryable via `load_skill()`. `odin skills list` CLI.
- **Scheduler persistence**: Jobs now stored in SQLite and survive restart. `SchedulerStore` trait + `SqliteSchedulerStore`.
- **Discord gateway**: Real serenity 0.12 integration with slash commands (`/odin run|status|sessions|tasks`), permission gating, threaded updates.
- **WebSocket gateway**: Real Axum WS upgrade handler with broadcast channel, JSON protocol (task_started/progress/complete/error), ping/pong.
- **Provider fallback chains**: `FallbackProvider` with ordered chain (weak→local→escalation), circuit breaker, health checks. `odin providers list` CLI.
- **Secret redaction**: `SecretRedactor` with 12 patterns (API keys, JWT, tokens, private keys) for sanitizing tool outputs before logging.
- **Expanded dangerous commands**: Default policy now blocks 22 patterns (rm, chmod, chown, git destructive, iptables, systemctl, shutdown, fork bombs).
- **Skills dir config**: `agent.skills_dir` in OdinConfig. Sample skills in `examples/skills/`.
- **WebSocket config**: Gateway section with `ws_enabled`, `ws_addr`, `ws_max_connections`, `ws_ping_interval_secs`.
- **Discord admin role**: Configurable `discord_admin_role` for permission-gated commands.
- **Fallback chain config**: `fallback_chain`, `health_check_interval_secs`, `circuit_breaker_threshold` per provider.
- **Scheduler DB config**: `scheduler.db_path` for job persistence.

### Changed
- **README**: Updated to reflect real implementations (no more stub caveats for Discord, WS, scheduler).
- **examples/config.yaml**: Expanded with all new configuration options.
- **CLI**: New subcommands `Skills`, `Providers`. PhaseContext now carries `skill_registry`.
- **LoopEngine**: New `with_skill_registry()` and `load_skill()` methods.
- **PolicyEngine**: Expanded default dangerous commands from 8 to 22 patterns.

### Fixed
- Discord test compilation with non_exhaustive structs (removed incompatible tests).
- ProviderConfig requires fallback_chain, health_check fields in all constructors.

## [0.1.0] — 2026-07-06

### Initial Release
- 13-crate Rust workspace: odin-core, odin-runtime, odin-loop, odin-providers, odin-tools, odin-memory, odin-scheduler, odin-permissions, odin-audit, odin-gateway, odin-skills, odin-cli, odin-baseline.
- 7-phase loop engine (PLAN→ACT→INSPECT→CRITIQUE→REVISE→VERIFY→DECIDE).
- 4 provider backends: OpenAI-compatible, Anthropic, Local (Ollama), DeepSeek.
- Built-in tools: file_read, file_write, shell, web_search.
- SQLite memory store, file audit logger, cron scheduler, permission engine.
- HTTP API server with `/health` and `/chat` endpoints.
- CLI: `odin run|serve|schedule|config|version`.
- 194 tests passing, CI pipeline (check, test, lint, bench, security audit).
- Benchmark comparing looped vs baseline agent execution.
