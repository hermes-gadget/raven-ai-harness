# Raven Agent

Raven Agent is a Rust agent runtime with structured model loops, multi-agent task decomposition, persistent orchestration state, tools, skills, scheduling, memory, and HTTP, WebSocket, Discord, and terminal interfaces.

Current workspace version: **0.3.0**.

The user-facing command is **raven**. Internal crates retain their historical **odin-\*** names; the **odin** binary remains as a compatibility alias.
The GitHub repository still has the historical **raven-ai-harness** slug; this is a repository locator, not the product name.

## Quick start

Prerequisites:

- Rust 1.88 or newer (the workspace uses Rust edition 2024)
- A model provider configured through YAML
- Provider credentials supplied through environment variables, not committed config values

~~~bash
git clone https://github.com/hermes-gadget/raven-ai-harness.git
cd raven-ai-harness

cargo build --workspace

# Create the canonical config file, then edit it.
cargo run -- config
cargo run -- config --edit

# cargo run selects the raven binary and opens the TUI.
cargo run

# Execute a goal with orchestration.
cargo run -- run "review this repository and report concrete issues"

# Execute with one agent.
cargo run -- run --direct "summarize README.md"
~~~

To install only the primary command:

~~~bash
cargo install --path crates/odin-cli --bin raven
raven --help
~~~

## Configuration

The canonical path is **~/.config/raven/config.yaml**. **RAVEN_CONFIG** selects another path. Raven also reads **ODIN_CONFIG**, **~/.odin/config.yaml**, and **odin.yaml** / **odin.yml** as compatibility fallbacks.

Minimal local-provider configuration:

~~~yaml
general:
  instance_name: raven
  log_level: info

models:
  default_provider: local
  default_model: qwen2.5-coder
  providers:
    local:
      provider_type: openai_compat
      base_url: http://localhost:11434/v1
      default_model: qwen2.5-coder

safety:
  require_approval: true

tools:
  path_boundary:
    allowed_read: ["."]
    allowed_write: ["."]
    denied: [".git", ".env"]
~~~

For a hosted provider, set **api_key_env** in YAML and export that environment variable. See [examples/config.yaml](examples/config.yaml) for the annotated schema.

MCP tools are treated as unsafe and approval-required by default. A server can opt into **safe: true** and **requires_approval: false** only when its complete tool surface is trusted.

## Commands

| Command | Behavior |
|---|---|
| **raven** | Open the interactive terminal UI |
| **raven run &lt;goal&gt;** | Decompose and execute a goal with sub-agents; persist graph, lifecycle, and lock state |
| **raven run --direct &lt;goal&gt;** | Execute through one runtime agent |
| **raven orchestrate submit &lt;goal&gt;** | Save a decomposed plan without executing it |
| **raven orchestrate status** | List persisted run/plan IDs and lifecycle state |
| **raven orchestrate inspect &lt;id&gt;** | Inspect a persisted graph or agent lifecycle |
| **raven orchestrate cancel &lt;id&gt;** | Mark a stored graph or lifecycle cancelled |
| **raven orchestrate pause / resume** | Change stored status markers; this does not signal another process |
| **raven orchestrate agents / locks / queue / restore** | Inspect persisted orchestration data |
| **raven serve** | Start the HTTP API; WebSocket upgrades are served at **/ws** |
| **raven schedule add / list / remove / enable / disable** | Manage SQLite-backed scheduled job definitions |
| **raven tools list / inspect / validate / doctor / catalog / reliability** | Inspect the built-in tool system |
| **raven tools test &lt;name&gt; --dry-run** | Validate a tool call without executing it |
| **raven tools test &lt;name&gt; --args &lt;json&gt; --approve** | Explicitly approve direct execution of a dangerous tool |
| **raven skills list / tools** | Inspect markdown skills and their tool dependencies |
| **raven providers list** | Show configured providers |
| **raven eval mocked / profiles / live** | Run deterministic small-model evals, inspect profiles, or check live-eval readiness |
| **raven tasks**, **raven sessions**, **raven audit replay** | Inspect audit-derived history |
| **raven config**, **raven status**, **raven version** | Inspect local configuration and build information |

Run **raven &lt;command&gt; --help** for arguments.

## Terminal UI

**raven** opens a chat-first terminal UI with an always-visible orchestration side panel on normal terminal widths:

1. Chat
2. Agents
3. Task Graph
4. Files/Locks
5. Tools
6. Logs/Audit
7. Run and plan history
8. Conflicts

Entering a goal starts an in-process orchestration run, persists the graph/lifecycle/lock state, and streams status into the chat and side panel. The first feedback is shown immediately while the run is being created, decomposed, and wired to provider/tool resources. Active agents show heartbeat frames, current stage, current model/tool/lock wait, elapsed time, last event age, and the latest blocker or error. Model calls emit `waiting for model...` progress after 10 seconds, and the UI warns if no runner event arrives for 15 seconds.

Follow-up chat messages steer the active run instead of creating disconnected fake runs. The TUI runner uses the configured provider fallback chain, permission policy, built-in and MCP tool registry, redacted audit logger, and configured provider HTTP timeouts. Runtime tracing for the TUI is written to `~/.raven-agent/tui.log` so logs do not corrupt the alternate-screen interface.

| Key | Action |
|---|---|
| Enter | Submit a new goal, or steer the active run |
| Shift+Enter / Alt+Enter | Insert a newline |
| Tab / Shift+Tab | Move between tabs |
| Alt+1 … Alt+8 | Select a tab |
| Up, Down, PageUp, PageDown | Scroll |
| Ctrl+F | Search the UI log |
| ? | Toggle help |
| Ctrl+D | Quit |
| Esc | Close search/help, then quit |

Chat commands:

- **/pause** and **/resume** control the active in-process TUI run. Pause stops scheduling new agents; in-flight model/tool calls may still finish until they complete or the run is cancelled.
- **/cancel** opens an approve/deny modal before cancelling the active run.
- **/redirect &lt;text&gt;** steers pending work in the active run.
- **/prio &lt;agent-id-prefix&gt; &lt;priority&gt;** reprioritises matching active agents.

## Architecture

~~~text
raven CLI / TUI / HTTP / WebSocket / Discord
                    |
          composer + task graph
                    |
      runtime agents + seven-phase loop
                    |
 providers | tools | skills | permissions
                    |
 memory | scheduler | audit | SQLite state
~~~

The internal crate boundaries are:

- **odin-core**: shared types, configuration, errors, and traits
- **odin-loop**: PLAN → ACT → INSPECT → CRITIQUE → REVISE → VERIFY → DECIDE
- **odin-eval**: deterministic small/local/cheap model evaluation harness and reports
- **odin-orchestrator**: decomposition, task graphs, sub-agent lifecycle, locks, merge, persistence
- **odin-runtime**: agents and sessions
- **odin-providers**: OpenAI-compatible, Anthropic, local, and fallback providers
- **odin-tools** and **odin-mcp**: built-in and external tools
- **odin-permissions** and **odin-audit**: policy, approval decisions, redaction, and audit records
- **odin-memory** and **odin-scheduler**: SQLite-backed memory and scheduled job definitions
- **odin-gateway**: HTTP, WebSocket, and Discord
- **odin-tui**: terminal UI, in-process orchestration runner, live state rendering, and run controls

More detail: [ARCHITECTURE.md](ARCHITECTURE.md).

## Safety behavior

- Tool execution goes through rate limits, explicit allow/deny rules, approval requirements, command checks, and path boundaries.
- Calls fail closed when an approval-required tool has no approval responder.
- Direct dangerous-tool testing requires **--approve**.
- Unknown MCP tools are unsafe and approval-required by default.
- Tool results, TUI logs, configuration display, and audit entries redact supported secret and PII patterns.
- Audit redaction cannot be disabled, including by legacy **mask_secrets: false** configuration.
- Real (non-dry-run) tool attempts persist only their outcome class, duration, and timestamp in the bounded **reliability.db** store under the configured data directory. CLI and TUI reliability views share this store.

## Known limitations

- The TUI controls runs it starts in-process. **raven orchestrate pause/resume/cancel** still update persistent markers only and do not signal a separate process.
- The HTTP and Discord orchestration submission endpoints create persisted plans. Task execution is available through **raven run**, HTTP **/chat**, and Discord **/raven run**.
- **raven serve** also starts Discord when **gateway.discord_enabled** is true and a token is available from the configured value or environment variable.
- The model loop still has no general interactive tool-call approval responder. The TUI currently approval-gates dangerous TUI actions such as cancellation; approval-required tool calls are denied unless policy explicitly allows them or approval requirements are disabled for a trusted environment.
- Scheduler definitions persist, but a separate long-running scheduler process is required to execute due jobs continuously.
- WebSocket clients receive task/orchestration events, but inbound pause/resume/cancel control messages are not dispatched.
- Memory is attached to the direct runtime path; orchestrated sub-agent memory retrieval is not yet integrated.

Deferred work is tracked in [TODO.md](TODO.md) and repository issues.

## Development

~~~bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo check --workspace --all-targets
cargo test --workspace --all-targets
cargo run -p odin-cli --bin raven -- eval mocked --format json
cargo bench --no-run
scripts/validate-tools.sh
~~~

Small/local/cheap model evaluation details are in [docs/small-model-evals.md](docs/small-model-evals.md).

## License

MIT. See [LICENSE](LICENSE).
