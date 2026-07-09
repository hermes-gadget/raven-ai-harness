# Changelog

All notable Raven Agent changes are recorded here.

## Unreleased — Repo Tidy + Stabilisation Pass

### Changed

- Made **raven** the primary command and default Cargo binary; **odin** remains a compatibility alias in a separate target.
- Standardized the current workspace and documentation version on **0.3.0**.
- Replaced the stale README with the real quickstart, command surface, architecture, safety behavior, and known limitations.
- Moved canonical configuration to **~/.config/raven/config.yaml** while retaining legacy read fallbacks.
- Moved default runtime data to **~/.raven-agent** and wired configured memory, audit, scheduler, sandbox, enabled-tool, disabled-tool, and MCP environment settings.
- Made TUI submission persist a real plan and populated its status, graph, agents, lock, tools, log, history, and conflict views from real state.
- Removed disconnected TUI approval, pause, resume, interrupt, and cancel controls.

### Fixed

- Persisted orchestration graphs under the run ID shown to users instead of an unrelated goal key.
- Kept graph JSON and summary status synchronized, including paused state and legacy goal lookup.
- Persisted real run graph transitions, agent lifecycles, and lock snapshots from orchestrated CLI execution.
- Wired the real tool registry and audit logger into direct, orchestrated, and HTTP execution paths.
- Made HTTP diagnostics report the live configured tool registry and mounted the advertised WebSocket route with its shared broadcast manager.
- Wired configured Discord startup and shutdown into **raven serve**.
- Enforced rate limits, permission decisions, approval requirements, command checks, and path boundaries before tool execution.
- Replaced simulated successful tool calls with actionable failures when no registry exists.
- Made unknown MCP tools unsafe and approval-required by default.
- Required explicit approval for direct execution of dangerous tools.
- Made audit secret and PII redaction unconditional; redacted config display, tool output, TUI logs, and sensitive diagnostic paths.
- Fixed scheduler CLI persistence, restored task goals across invocations, and corrected runtime dispatch to use a registered agent rather than a task ID or inert closure.
- Fixed a deadlocking provider circuit-breaker test, placeholder assertions, all-target lint failures, and Unicode TUI cursor handling.
- Replaced the stale tool-validation shell harness with direct tests, registry validation, doctor checks, and side-effect-free dangerous-tool dry runs.

## 0.3.0 — 2026-07-08

- Added the composer, task graph, sub-agent lifecycle, file lock, merge, and SQLite orchestration layers.
- Added persistent orchestration inspection through CLI and gateway interfaces.
- Added the Raven Agent terminal UI.
- Renamed the product to Raven Agent. Internal Rust crates intentionally retain **odin-\*** package names.

## 0.2.0 — 2026-07-07

- Added skills, scheduler persistence, Discord and WebSocket gateways, provider fallbacks, safety patterns, and expanded tool diagnostics.
- The user-facing command at this release was the legacy **odin** binary.

## 0.1.0 — 2026-07-06

- Initial Rust workspace with the seven-phase loop, providers, built-in tools, memory, audit, scheduler, permissions, HTTP API, CLI, tests, and benchmarks.
- The user-facing command at this release was the legacy **odin** binary.
