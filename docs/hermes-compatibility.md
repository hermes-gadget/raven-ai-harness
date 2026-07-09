# Hermes compatibility notes

Raven Agent is inspired by Hermes Agent but is not a drop-in implementation. This document records functional overlap without claiming protocol, configuration, or behavioral compatibility.

| Area | Raven Agent 0.3.0 |
|---|---|
| Model loop | Seven explicit phases with heuristic fallbacks |
| Multi-agent decomposition | Composer, task graph, concurrent sub-agents, and merge |
| Tools | Rust trait registry, built-ins, validation, dry-run, and MCP stdio adapters |
| Skills | Markdown loading and tool dependency validation |
| Memory | SQLite store; attached to direct runtime execution |
| Scheduler | SQLite definitions and runtime dispatch API |
| Safety | Rules, rate limits, path boundaries, approval decisions, and secret/PII redaction |
| Audit | Redacted in-memory and JSONL records |
| HTTP | Health, chat/task, tools, orchestration state, and WebSocket upgrade |
| Discord | **/raven** runtime and orchestration commands |
| Terminal | Chat-first TUI with in-process orchestration runs, persisted state monitoring, live runner events, model-wait heartbeats, stale-event warnings, and active-run controls |

Important differences:

- Configuration uses Raven Agent's own YAML schema.
- The primary command is **raven**.
- The TUI controls runs it starts in-process; non-interactive orchestration control is still stored-state only.
- General approval-required tool calls fail closed because no interactive tool-call responder is connected.
- Scheduler execution requires a continuously hosted scheduler/runtime process.
- Telegram, Slack, voice, browser automation, a web dashboard, and credential pooling are not implemented.

No performance or memory comparison is claimed without a reproducible benchmark on equivalent workloads.
