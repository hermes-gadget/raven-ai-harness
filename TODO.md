# Raven Agent — TODO

> Updated: 2026-07-24 | Workspace version: 0.3.0

## Full audit pass (2026-07-09)

See [`audit.md`](audit.md) for the complete checklist, root causes, and verification.

- [x] CLI multi-task `get_graph` goal-shadowing panic fixed
- [x] CLI no longer executes agents that failed file-lock acquisition
- [x] Sub-agent tool scoping from capabilities (not full registry by default)
- [x] TUI runner JoinError cannot hang a run forever
- [x] TUI Error event clears active-run handles
- [x] Pause messaging documents in-flight work may finish
- [x] TUI loads provider/model into top bar from config
- [x] All required cargo/script gates green

## TUI Live Execution Bugfix

...all sub-items complete (see git history for full detail)...

## Small-Model Excellence Phase

...all sub-items complete (see git history for full detail)...

## TUI Responsiveness + Live Agent Feedback Debug

...all sub-items complete (see git history for full detail)...

## End-to-End Verification + TUI Rework

...all sub-items complete (see git history for full detail)...

## Repo Tidy + Stabilisation Pass — Complete

...all sub-items complete (see git history for full detail)...

## Deferred work

These items are intentionally not represented as complete:

- [x] [#1 Interactive approval responder](https://github.com/hermes-gadget/Raven-Agent/issues/1) — **PR #13 merged**
- [x] [#2 Persisted real tool reliability samples](https://github.com/hermes-gadget/Raven-Agent/issues/2) — **PR #12 merged**
- [x] [#3 Continuously hosted scheduler execution mode](https://github.com/hermes-gadget/Raven-Agent/issues/3) — **PR #14 created, CI pending**
- [ ] [#4 Live cross-process orchestration control and WebSocket dispatch](https://github.com/hermes-gadget/Raven-Agent/issues/4) — **PR in branch feat/live-control-memory-4-5**
- [ ] [#5 Orchestrated sub-agent memory integration](https://github.com/hermes-gadget/Raven-Agent/issues/5) — **PR in branch feat/live-control-memory-4-5**

## Definition of done

- [x] Product naming, command naming, version, config paths, and repository metadata are consistent.
- [x] No known placeholder UX or fabricated status output remains in user-facing paths.
- [x] Execution paths use real registries, persistence, permissions, redaction, and audit hooks.
- [x] README and architecture state limitations instead of presenting deferred behavior as complete.
- [x] Genuine deferrals have focused GitHub issues.
- [x] Every required verification gate passes on the final worktree.
