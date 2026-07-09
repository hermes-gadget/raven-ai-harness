# Raven Agent Audit — 2026-07-09

Workspace: `raven-ai-harness` (product: Raven Agent v0.3.0)  
Branch: `agent/tui-live-feedback` → audit fixes  
Scope: full repo (runtime, orchestrator, TUI, CLI, providers, tools, MCP, scheduler, memory, audit, permissions, gateway, docs, tests, CI, config, examples)

## Status legend

- `[ ]` open
- `[~]` in progress
- `[x]` fixed and verified
- `[d]` deferred (documented, not blocking)

---

## Critical / High

### A1. CLI orchestrated run panics on multi-task goals
- **Area:** CLI / orchestration persistence
- **Symptom:** `raven run "do A and do B"` can panic at `composer.get_graph(goal).unwrap()`
- **Root cause:** In `run_orchestrated`, the loop binds `goal` to each *node* goal, shadowing the root goal used as the task-graph key. `get_graph` looks up by `root_goal`.
- **Fix:** Persist using the root goal key (or graph id), never the sub-task goal.
- **Status:** [x] fixed

### A2. CLI spawns sub-agents even when file locks queue them
- **Area:** CLI / file locks
- **Symptom:** Agents that fail `start_agent` (WaitingForLock) still execute, racing writers.
- **Root cause:** `run_orchestrated` logs the queue reason then always `tokio::spawn`s execution.
- **Fix:** Only spawn when start succeeds; re-check queued agents as locks release (same pattern as TUI runner).
- **Status:** [x] fixed

### A3. Sub-agents not tool-scoped
- **Area:** orchestrator / TUI / CLI tools
- **Symptom:** Every sub-agent gets the full tool registry (or empty allow-list → all tools).
- **Root cause:** `SubAgentConfigBuilder` defaults `allowed_tools` to `[]`; `create_sub_agent` never maps `required_capabilities` → tools; TUI `register_agents` hardcodes `allowed_tools: Vec::new()`.
- **Fix:** Map capabilities to tool names; default a safe agent toolkit when unset; wire `create_sub_agent` + runners to use it; `scoped()` still treats empty as all-tools only for explicit opt-in callers.
- **Status:** [x] fixed

### A4. TUI runner hang if a join-set task panics/aborts without result
- **Area:** TUI runner
- **Symptom:** Run never finishes if a sub-agent task JoinError occurs while others complete.
- **Root cause:** `JoinError` path logs an error but does not mark the agent terminal; `spawned` prevents re-spawn.
- **Fix:** Nest execution so panics become `AgentExecution` failures with agent id; mark terminal and persist failure.
- **Status:** [x] fixed

### A5. TUI Error event leaves stale active-run handles
- **Area:** TUI app state
- **Symptom:** After `RunnerEvent::Error`, mode goes Idle but `runner_rx` / `active_run_id` can linger until refresh.
- **Root cause:** Error handler only clears `runner_tx`.
- **Fix:** Clear `runner_tx`, `runner_rx`, `active_run_id`, and `running_runs` consistently (same as cancel/finish).
- **Status:** [x] fixed

---

## Medium

### B1. Heuristic decomposition never assigns files/capabilities
- **Area:** orchestrator composer
- **Symptom:** File locks / write queues rarely engage for TUI/CLI default path; side panel locks empty.
- **Root cause:** Sync `decompose()` creates nodes with empty `read_files`/`write_files`/`required_capabilities`. LLM path exists but is unused by TUI/CLI runners.
- **Fix:** Assign default capabilities (and thus tools) on heuristic nodes; keep file lists empty unless LLM/user provides them. Optionally use LLM decompose when provider available (deferred if slow).
- **Status:** [x] fixed (default capabilities + tool mapping; LLM path remains available)

### B2. Pause does not stop in-flight model/tool calls
- **Area:** TUI runner / lifecycle
- **Symptom:** `/pause` stops scheduling new work and updates lifecycle, but running join-set tasks continue until completion.
- **Root cause:** Pause only sets a flag + `composer.pause_all()`; does not abort join handles (cancel does).
- **Fix:** Document accurately; on pause, stop spawning and surface "paused (in-flight work may finish)" — full cooperative cancel of model calls needs provider-level cancellation (deferred).
- **Status:** [x] fixed (UI/docs accuracy + in-flight pause notice)

### B3. `refresh_orchestration` on every TUI tick hits SQLite
- **Area:** TUI performance
- **Symptom:** Possible lag on slow disks; not a freeze by itself.
- **Root cause:** Tick handler always awaits full store refresh.
- **Fix:** Keep for correctness of locks/agents; no change unless profiling shows issue.
- **Status:** [d] deferred — live correctness preferred; tick is 500ms

### B4. Provider/model not shown in TUI top bar until configured elsewhere
- **Area:** TUI
- **Symptom:** Top bar shows `-/-` for provider/model.
- **Root cause:** `App::new` never loads config into `provider_name`/`model_name`.
- **Fix:** Load config defaults into app on startup.
- **Status:** [x] fixed

---

## Low / Docs / Naming

### C1. Historical crate names `odin-*` vs product Raven
- **Status:** [d] intentional; documented in README

### C2. Repo slug `raven-ai-harness` vs product name
- **Status:** [d] intentional locator

### C3. CLI `orchestrate pause/resume` only mutates DB markers
- **Status:** [x] already documented in README/CLI help; left as-is

### C4. CI uses `dtolnay/rust-toolchain@stable` matching `rust-toolchain.toml`
- **Status:** [x] OK (edition 2024 / 1.85+ on stable)

### C5. Binary `odin` keeps non-interactive bare invocation
- **Status:** [x] intentional compatibility

---

## Areas audited (summary)

| Area | Result |
|------|--------|
| Runtime | OK — Agent/Runtime wired |
| Orchestrator | Bugs A1–A3, B1 fixed |
| TUI | Live path present; A4–A5, B2, B4 fixed |
| CLI | A1–A2 fixed; MCP load at startup OK |
| Providers | Factory + fallback chain used by TUI |
| Tools | Validation + doctor green |
| MCP | Loaded in CLI run + TUI runner startup |
| Scheduler | SQLite-backed; CLI wired |
| Memory | SQLite store; CLI wired |
| Audit | Redacting logger used in TUI/CLI |
| Permissions | Policy engine + redaction present |
| Gateway | HTTP/WS/Discord present |
| Docs | Updated for pause semantics + scoping |
| Tests | Expanded for fixed bugs |
| CI | fmt/clippy/test/eval/tools/bench jobs present |
| Config/examples | Schema matches deserializers |

---

## Verification gates

- [x] `cargo fmt --all -- --check`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo check --workspace --all-targets`
- [x] `cargo test --workspace --all-targets`
- [x] `cargo bench --no-run`
- [x] `scripts/validate-tools.sh`

---

## Final status

Audit complete. Real bugs above fixed with tests. TUI remains connected to real orchestrator/event bus (no mock UI state outside tests). Completed work is on a PR branch.
