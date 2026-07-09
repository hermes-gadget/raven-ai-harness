# Synthetic loop comparison

The **odin-loop** comparison tests exercise Raven Agent's loop and the naive baseline against deterministic simulated providers. They are regression tests for retry, phase, token-accounting, and error-recovery logic.

They are not evidence of real-model quality, latency, token cost, or production reliability.

Run them with:

~~~bash
cargo test -p odin-loop --test comparison_harness -- --nocapture
~~~

The ignored live comparison test requires an explicit provider key and network access. Its output depends on the selected model and should be recorded with model name, provider, date, configuration, and task set before drawing conclusions.
