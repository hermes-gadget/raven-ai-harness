---
name: code-review
description: Review code for bugs, style issues, and test coverage
required_tools:
  - file_read
  - git
enabled: true
---

## Code Review Workflow

Use this skill when you need to review code changes.

### Steps

1. **Read the diff** — Get the full diff of the changes.
2. **Check for bugs** — Look for logic errors, null pointers, off-by-one errors, concurrency issues, and security vulnerabilities.
3. **Style check** — Verify code follows the project's style guide (naming, formatting, comments).
4. **Test coverage** — Check that new code has tests and existing tests still pass.
5. **Provide feedback** — Summarize findings with clear, actionable comments.
