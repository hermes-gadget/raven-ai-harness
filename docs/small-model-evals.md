# Small-model excellence evals

Raven has a deterministic small-model eval harness in **odin-eval** and a CLI surface under **raven eval**. The goal is to measure where Raven's looped/orchestrated execution helps smaller, local, or cheaper models compared with a single-pass baseline.

## What the mocked suite covers

The CI-safe suite covers one task in each required category:

| Category | What it exercises |
|---|---|
| coding | small code-generation task and test-plan evidence |
| repo_edit | read/write discipline for a scoped repository edit |
| debugging | reproduction, root-cause reasoning, and verifier evidence |
| docs | concise documentation update |
| tool_use | structured tool output handling |
| multi_file | decomposition for a multi-file change |
| long_context | context distillation into facts, decisions, errors, and next action |
| failed_tool_recovery | one-shot repair of malformed tool-call JSON |

Run it locally:

~~~bash
raven eval mocked
raven eval mocked --format json
raven eval mocked --profile ollama-qwen2.5-coder-7b --output eval-report.md
~~~

CI runs:

~~~bash
cargo run -p odin-cli --bin raven -- eval mocked --format json
~~~

## Dashboard metrics

The report includes:

- success rate;
- average iterations;
- average tokens;
- tool calls, tool errors, and repaired tool arguments;
- estimated cost;
- escalation rate;
- per-task Raven-vs-baseline winner.

The mocked suite is deterministic. It is useful for regressions and architectural comparison, not as proof of a live provider's real latency, price, or quality.

## Built-in model profiles

List profiles:

~~~bash
raven eval profiles
raven eval profiles --format json
~~~

Current recommendations:

| Profile | Best fit | Watch for |
|---|---|---|
| `ollama-qwen2.5-coder-7b` | local repo edits, short code generation, strict JSON with examples | long multi-file context, ambiguous tool schemas |
| `ollama-qwen2.5-coder-14b` | stronger local multi-file code edits and tool planning | large refactors still need verifier evidence |
| `deepseek-small-cheap` | cheap code reasoning and verifier passes | over-confident answers without evidence |
| `ollama-llama3.1-8b` | summaries and simple docs edits | strict tool JSON, shell/git tool complexity, long context |

Raven uses these profiles to bound prompts, set retry limits, choose when to decompose, and decide when stronger verification or model escalation is warranted.

## Loop features used for small models

- Strict JSON planning with bullet-list fallback.
- Short action prompts with explicit tool-argument hints.
- One-shot repair for malformed tool-call arguments or common aliases such as `cmd` → `command`.
- Context distillation into facts, decisions, files changed, errors, and next action.
- Evidence-based verification helpers so a verifier can check concrete outputs rather than self-confidence alone.
- Failure taxonomy for model confusion, bad tool args, missing context, permission denied, timeout, hallucinated file/tool, provider error, and verification gaps.

## Optional live eval readiness

Live evals are deliberately gated by provider config and keys so CI does not spend money or depend on local services.

Examples:

~~~bash
raven eval live --provider ollama --model qwen2.5-coder:7b --base-url http://localhost:11434/v1
raven eval live --provider deepseek --model deepseek-chat --api-key-env DEEPSEEK_API_KEY
~~~

Record live results with the exact date, provider, model, base URL, profile, task set, and Raven commit before comparing model quality.
