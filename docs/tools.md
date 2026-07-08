# Odin Tools — Reference

> Auto-generated tool documentation. Each tool has a unique name, JSON schema,
> capability tags, and permission requirements.

## Health Checks

| Command | Description |
|---------|-------------|
| `odin tools list` | List all registered tools with parameters |
| `odin tools inspect <name>` | Full detail on one tool (schema, tags, permissions) |
| `odin tools validate` | Run validation on all tools (schema, args, permissions) |
| `odin tools doctor` | Comprehensive ecosystem health check |
| `odin tools test <name> --args '{...}'` | Execute a tool with JSON arguments |

### Doctor Checks

The `odin tools doctor` command runs a comprehensive health check across every
tool and the ecosystem:

**Per-tool checks:**
- Unique name (non-empty)
- Description (non-empty)
- Valid JSON schema (type=object)
- Required parameters documented
- Capability tags present
- Safety consistency (dangerous tag ↔ is_dangerous())
- Safe tag consistency (safe tag ↔ is_safe())
- Schema name matches tool name

**Ecosystem checks:**
- Duplicate tool detection
- Tool count

Exit code is non-zero if any check fails — suitable for CI enforcement.

## Tool Categories

| Category | Tools | Description |
|----------|-------|-------------|
| **filesystem** | `file_read`, `file_write` | Read/write files within sandbox boundaries |
| **shell** | `shell` | Execute shell commands (dangerous — requires approval) |
| **web** | `web_fetch`, `web_search` | HTTP GET and web search |
| **version-control** | `git` | Git repository operations |
| **github** | `github_issue_create`, `github_issue_search`, `github_pr_create`, `github_pr_status`, `github_actions_status` | GitHub issue, PR, and Actions management via `gh` CLI |
| **system** | `system_info`, `disk_usage` | OS and filesystem diagnostics |
| **data** | `json_extract` | JSON query and transformation |

---

## `file_read`

**Category:** filesystem | **Safety:** safe | **Approval:** not required

**Capability tags:** `filesystem`, `read`, `safe`

Read the contents of a file at the given path. The path must be within the
configured sandbox boundaries (`allowed_read` in config). Denied paths and
paths outside boundaries are rejected.

### Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `path` | string | ✅ | Absolute or relative path to the file to read |

### Example

```json
{"path": "/tmp/report.txt"}
```

### Returns

File contents as a string. If the file doesn't exist or is outside boundaries,
returns an error.

### Tests

- `test_file_read_write_roundtrip` — write then read back
- `test_file_read_nonexistent` — error on missing file
- `test_file_write_denied` — error on denied path

---

## `file_write`

**Category:** filesystem | **Safety:** dangerous | **Approval:** deferred

**Capability tags:** `filesystem`, `write`, `dangerous`

Write content to a file at the given path. Creates parent directories if needed.
The path must be within the configured sandbox boundaries (`allowed_write`).
Denied paths are rejected.

### Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `path` | string | ✅ | Absolute or relative path to the file to write |
| `content` | string | ✅ | Content to write to the file |

### Example

```json
{"path": "/tmp/output.txt", "content": "Hello, Odin!"}
```

### Returns

Confirmation with byte count and path, e.g. `Successfully wrote 12 bytes to /tmp/output.txt`.

### Tests

- `test_file_read_write_roundtrip` — write then read back
- `test_file_write_denied` — error on denied path (/etc)

---

## `shell`

**Category:** shell | **Safety:** dangerous | **Approval:** required

**Capability tags:** `shell`, `system`, `dangerous`

Execute a shell command via `/bin/sh -c`. Commands matching dangerous patterns
(17 regex patterns including `rm -rf`, `sudo`, `chmod 777`, `mkfs`, `dd`, etc.)
are blocked before execution. All shell commands require approval.

### Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `command` | string | ✅ | Shell command to execute |
| `workdir` | string | ❌ | Working directory (defaults to agent working dir) |
| `timeout_secs` | integer | ❌ | Timeout in seconds (default: 60) |

### Example

```json
{"command": "ls -la /tmp", "timeout_secs": 10}
```

### Dangerous patterns blocked

`rm -rf`, `rm -r /`, `git reset --hard`, `git push --force`, `sudo`, `chmod 777`,
`> /dev/`, `mkfs.`, `dd if=`, fork bombs, `> /dev/sda`, `mv /`, `shutdown`,
`reboot`, `init 0`, `init 6`, `poweroff`

### Returns

Combined stdout and stderr. Non-zero exit codes are reported as errors.

### Tests

- `test_shell_echo` — echo returns output
- `test_shell_pwd` — pwd returns working dir
- `test_shell_dangerous_blocked` — `rm -rf` is blocked
- `test_is_dangerous` — pattern matching verification
- `test_shell_timeout` — timeout handling
- `test_shell_invalid_command` — error on nonexistent command

---

## `web_fetch`

**Category:** web | **Safety:** safe | **Approval:** not required

**Capability tags:** `web`, `http`, `read`, `safe`

Fetch the content of a URL via HTTP GET. Returns the raw text response body.
URLs must start with `http://` or `https://`. Output is truncated at 100KB.
Uses a 30-second timeout with OdinTools/1.0 user-agent.

### Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `url` | string | ✅ | The URL to fetch (must start with http:// or https://) |

### Example

```json
{"url": "https://example.com/api/data"}
```

### Returns

Response body as text (truncated at 100KB). HTTP errors return the status code.

### Tests

- `test_web_fetch_invalid_url` — rejects non-http URLs
- `test_web_fetch_http_error` — handles HTTP 404
- `test_web_fetch_timeout` — handles slow endpoints
- `test_urlencoding` — URL encoding verification

---

## `web_search`

**Category:** web | **Safety:** safe | **Approval:** not required

**Capability tags:** `web`, `search`, `read`, `safe`

Search the web for information. When a search URL template is configured via
`with_search_url()`, performs an HTTP GET to the search provider. Without
a template, returns an informative message about configuration.

### Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `query` | string | ✅ | The search query |
| `max_results` | integer | ❌ | Maximum results (default: 5) |

### Example

```json
{"query": "rust async programming", "max_results": 3}
```

### Returns

Search results as text (truncated at 100KB). If no search provider is
configured, returns a configuration notice.

### Tests

- `test_web_search_no_template` — returns config notice
- `test_urlencoding` — special character encoding

---

## `git`

**Category:** version-control | **Safety:** dangerous | **Approval:** required

**Capability tags:** `version-control`, `git`, `dangerous`

Execute git commands in a repository. Supports any git subcommand with
proper argument splitting (respects single and double quotes). All git
commands require approval.

### Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `command` | string | ✅ | Git command and args (e.g., "status", "log --oneline -5") |
| `repo_path` | string | ❌ | Path to git repo (defaults to agent working dir) |
| `timeout_secs` | integer | ❌ | Timeout in seconds (default: 120) |

### Example

```json
{"command": "log --oneline -5", "repo_path": "/home/user/project"}
```

### Returns

Combined stdout and stderr. Non-zero exit codes include the git subcommand
and stderr in the error message. `color.ui=false` is set automatically.

### Tests

- `test_git_version` — `git version` works
- `test_git_invalid_repo` — error on nonexistent path
- `test_git_init_and_status` — init + status roundtrip
- `test_shlex_split` — argument splitting verification
- `test_build_args` — git arg construction

---

## `system_info`

**Category:** system | **Safety:** safe | **Approval:** not required

**Capability tags:** `diagnostic`, `read`, `safe`

Get operating system information: kernel version, hostname, CPU architecture,
and memory usage. Runs `uname -a` and `free -h` internally. Safe read-only
diagnostic tool with no side effects.

### Parameters

None — takes an empty object `{}`.

### Example

```json
{}
```

### Returns

Formatted text report with kernel/OS info followed by memory usage table.

### Tests

- `test_system_info_basic` — returns system info successfully
- `test_system_info_empty_args` — handles null args

---

## `disk_usage`

**Category:** system | **Safety:** safe | **Approval:** not required

**Capability tags:** `diagnostic`, `read`, `safe`

Show disk usage information via `df -h`. Optionally scoped to a specific path.

### Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `path` | string | ❌ | Optional path to check disk usage (default: all filesystems) |

### Example

```json
{"path": "/"}
```

### Returns

`df -h` output as text. Non-existent paths return an error.

### Tests

- `test_disk_usage_basic` — returns df output
- `test_disk_usage_with_path` — scoped to root
- `test_disk_usage_invalid_path` — handles bad path gracefully

---

## `http_request`

**Category:** web | **Safety:** safe | **Approval:** not required

**Capability tags:** `web`, `http`, `safe`

Make an arbitrary HTTP request with configurable method, URL, headers, and body.
Supports GET, POST, PUT, DELETE. URLs must start with `http://` or `https://`.
Uses a 30-second timeout. Responses are truncated at 100KB.

### Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `method` | string | ✅ | HTTP method: GET, POST, PUT, or DELETE |
| `url` | string | ✅ | The URL to request |
| `headers` | array | ❌ | Optional headers as `[{name, value}, ...]` |
| `body` | string | ❌ | Optional request body (for POST/PUT) |

### Example

```json
{
  "method": "POST",
  "url": "https://api.example.com/data",
  "headers": [{"name": "Authorization", "value": "Bearer token123"}],
  "body": "{\"key\": \"value\"}"
}
```

### Returns

Response body as text (truncated at 100KB). HTTP errors return the status code.

### Tests

- `test_http_request_invalid_url` — rejects non-http URLs
- `test_http_request_invalid_method` — rejects bad methods
- `test_http_request_get` — performs GET and handles response

---

## `json_extract`

**Category:** data | **Safety:** safe | **Approval:** not required

**Capability tags:** `data`, `transform`, `safe`

Extract a value from a JSON string using a simple dot-path query (e.g.,
`users.0.name`). Supports nested objects and array indices. Safe read-only
data transformation tool.

### Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `input` | string | ✅ | The JSON string to extract from |
| `query` | string | ✅ | Dot-path query (e.g. `users.0.name`) |

### Example

```json
{
  "input": "{\"users\":[{\"name\":\"Alice\"},{\"name\":\"Bob\"}]}",
  "query": "users.1.name"
}
```

### Returns

The extracted value as pretty-printed JSON. Errors if the path doesn't exist
or if the input is not valid JSON.

### Tests

- `test_json_extract_nested_object` — extracts from nested objects
- `test_json_extract_missing_key` — error on missing key
- `test_json_extract_invalid_json` — error on invalid input
- `test_json_extract_array_out_of_bounds` — error on bad index
- `test_json_extract_top_level_array` — extracts from arrays

---

## `github_issue_create`

**Category:** github | **Safety:** dangerous | **Approval:** required

**Capability tags:** `github`, `issue`, `write`, `dangerous`

Create a GitHub issue in a repository. Requires `gh` CLI to be installed
and authenticated (`gh auth login`).

### Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `repo` | string | ✅ | Repository in owner/repo format (e.g. `octocat/Hello-World`) |
| `title` | string | ✅ | Issue title |
| `body` | string | ❌ | Issue body / description (optional) |
| `labels` | string | ❌ | Comma-separated label names (e.g. `bug,urgent`) |

### Example

```json
{"repo": "octocat/Hello-World", "title": "Fix the widget", "body": "The widget is broken", "labels": "bug"}
```

### Returns

URL of the created issue on success. Error message on failure.

### Tests

- `test_github_issue_create_schema` — schema validation
- `test_github_issue_create_tags` — capability tag verification
- `test_build_issue_create_args_minimal` — minimal arg construction
- `test_build_issue_create_args_with_body_and_labels` — full arg construction

---

## `github_issue_search`

**Category:** github | **Safety:** safe | **Approval:** not required

**Capability tags:** `github`, `issue`, `read`, `safe`

Search GitHub issues in a repository with optional state and text filters.

### Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `repo` | string | ✅ | Repository in owner/repo format |
| `query` | string | ❌ | Search query (matches title, body, comments) |
| `state` | string | ❌ | Issue state: `open` or `closed` (default: open) |
| `limit` | integer | ❌ | Maximum results (default: 10) |

### Example

```json
{"repo": "octocat/Hello-World", "query": "bug", "state": "open", "limit": 5}
```

### Returns

List of issues matching the search criteria.

### Tests

- `test_github_issue_search_schema` — schema validation
- `test_github_issue_search_tags` — capability tag verification
- `test_build_issue_search_args_defaults` — default arg construction
- `test_build_issue_search_args_with_all` — full arg construction

---

## `github_pr_create`

**Category:** github | **Safety:** dangerous | **Approval:** required

**Capability tags:** `github`, `pr`, `write`, `dangerous`

Create a pull request on a GitHub repository. Requires `gh` CLI to be
installed and authenticated.

### Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `repo` | string | ✅ | Repository in owner/repo format |
| `title` | string | ✅ | Pull request title |
| `body` | string | ❌ | PR body / description (optional) |
| `base` | string | ❌ | Base branch to merge into (default: `main`) |
| `head` | string | ✅ | Head branch to merge from |

### Example

```json
{"repo": "octocat/Hello-World", "title": "Add feature", "head": "feature-branch", "base": "main"}
```

### Returns

URL of the created pull request. Error message on failure.

### Tests

- `test_github_pr_create_schema` — schema validation
- `test_github_pr_create_tags` — capability tag verification
- `test_build_pr_create_args_minimal` — minimal arg construction
- `test_build_pr_create_args_with_body` — full arg construction

---

## `github_pr_status`

**Category:** github | **Safety:** safe | **Approval:** not required

**Capability tags:** `github`, `pr`, `read`, `safe`

View details of a specific pull request or list all open PRs for a repository.

### Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `repo` | string | ✅ | Repository in owner/repo format |
| `pr_number` | integer | ❌ | PR number to view (optional — lists all open PRs if omitted) |

### Example

```json
{"repo": "octocat/Hello-World", "pr_number": 42}
```

### Returns

If `pr_number` is provided: PR details (title, state, URL, head/base refs,
author). Otherwise: list of up to 10 open PRs.

### Tests

- `test_github_pr_status_schema` — schema validation
- `test_github_pr_status_tags` — capability tag verification
- `test_build_pr_status_args_with_number` — specific PR view
- `test_build_pr_status_args_without_number` — list mode

---

## `github_actions_status`

**Category:** github | **Safety:** safe | **Approval:** not required

**Capability tags:** `github`, `ci`, `read`, `safe`

View recent GitHub Actions workflow runs for a repository. Can filter by
workflow filename or name.

### Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `repo` | string | ✅ | Repository in owner/repo format |
| `workflow` | string | ❌ | Workflow filename (e.g. `ci.yml`) or name (optional) |

### Example

```json
{"repo": "octocat/Hello-World", "workflow": "ci.yml"}
```

### Returns

List of up to 5 most recent workflow runs with their status and conclusion.

### Tests

- `test_github_actions_status_schema` — schema validation
- `test_github_actions_status_tags` — capability tag verification
- `test_build_actions_status_args_minimal` — default arg construction
- `test_build_actions_status_args_with_workflow` — workflow filter

---

## Adding New Tools

To add a new tool:

1. Implement the `Tool` trait from `odin_core::traits::Tool`
2. Define your struct with `name()`, `description()`, `schema()`, `execute()`
3. Override `requires_approval()` and `is_safe()` appropriately
4. Implement `capability_tags()` with at least one tag
5. Register with `ToolRegistry::register()`
6. Add tests in your tool's module
7. Add an entry to this document

### Required trait methods

```rust
#[async_trait]
impl Tool for MyTool {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> ToolSchema;
    fn capability_tags(&self) -> &[&str];
    fn requires_approval(&self) -> bool;
    fn is_safe(&self) -> bool;
    fn is_dangerous(&self) -> bool;
    async fn execute(&self, args: Value, ctx: &ToolContext) -> OdinResult<ToolResult>;
}
```
