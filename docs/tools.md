# Raven Agent tools

Raven Agent 0.3.0 builds one standard built-in registry for execution, the TUI catalog, validation, and diagnostics. Configuration **tools.enabled** selects built-ins and **tools.disabled** overrides enabled names.

## Inspection

| Command | Purpose |
|---|---|
| **raven tools list** | List tools, schemas, and capability tags |
| **raven tools inspect &lt;name&gt;** | Show one schema and its safety metadata |
| **raven tools validate** | Validate schemas, tags, and permission metadata |
| **raven tools doctor** | Run registry-wide consistency checks |
| **raven tools catalog** | Group tools by capability |
| **raven tools reliability** | Report whether persisted samples are available |

## Execution

**--dry-run** validates without invoking the wrapped tool:

~~~bash
raven tools test file_write \
  --args '{"path":"/tmp/example.txt","content":"hello"}' \
  --dry-run
~~~

Real execution of a dangerous tool is rejected unless the operator passes **--approve**:

~~~bash
raven tools test file_write \
  --args '{"path":"/tmp/example.txt","content":"hello"}' \
  --approve
~~~

Model-driven calls take a different route: registry lookup, rate limiting, policy decision, approval requirement, command/path checks, execution, redaction, and audit logging. If no approval responder is connected, approval-required calls fail closed.

## Built-in groups

- Files: read, write, list, exists, delete, text search
- Process/system: shell, process list, system information, disk usage, ping
- Web/data: fetch, search, HTTP request, JSON extract/validate
- Git and GitHub: local Git plus issue, pull request, and Actions operations
- Utility: environment lookup, time, and random number

The source of truth is **odin_tools::builtin_registry**. Tool metadata declares safety, required approval, JSON schema, and capability tags.

## MCP

Configured stdio MCP servers are started at CLI/runtime initialization. Their configured environment is passed to the child process. Discovered tools share one client per server.

Unknown MCP tools default to:

- unsafe;
- approval-required;
- capability tags **mcp**, **external**, and **dangerous**.

Trusted servers can set **safe: true** and **requires_approval: false**. Individual discovered names can be blocked through **tools.disabled**.

HTTP/SSE MCP transport is not implemented in 0.3.0.

## Output handling

Tool output and errors are redacted for supported secret and PII patterns before they enter loop state or audit records. Direct CLI test output is redacted as well. Output size caps remain tool-specific; the shell tool defaults to 1 MiB.
