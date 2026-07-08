---
name: git-workflow
description: Standard git branch, commit, and PR workflow
required_tools:
  - git
  - shell
enabled: true
---

## Standard Git Workflow

Use this skill when you need to make changes and create a pull request.

### Steps

1. **Create a feature branch** — Branch off from `main` or `develop` with a descriptive name: `git checkout -b feature/your-feature-name`.
2. **Make changes** — Implement the required changes incrementally.
3. **Commit with clear messages** — Use conventional commits: `type(scope): description`.
4. **Push the branch** — `git push -u origin feature/your-feature-name`.
5. **Open a pull request** — Use `gh pr create` with a title and description summarizing the changes.
