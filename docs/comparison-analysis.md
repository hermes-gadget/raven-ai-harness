# Synthetic loop comparison

The **odin-loop** comparison tests exercise Raven Agent's loop and the naive baseline against deterministic simulated providers. They are regression tests for retry, phase, token-accounting, and error-recovery logic.

They are not evidence of real-model quality, latency, token cost, or production reliability.

Run them with:

~~~bash
cargo test -p odin-loop --test comparison_harness -- --nocapture
~~~

The first-class small-model suite lives in **odin-eval** and runs through the CLI:

~~~bash
raven eval mocked
raven eval mocked --format json
~~~

That suite covers coding, repo edits, debugging, docs, tool use, multi-file work, long-context distillation, and failed tool recovery. See [small-model-evals.md](small-model-evals.md).

The ignored live comparison test and `raven eval live` readiness gate require explicit provider configuration and, for hosted providers, an API key environment variable. Live output depends on the selected model and should be recorded with model name, provider, date, configuration, task set, profile, and commit before drawing conclusions.
