#!/usr/bin/env bash
# Validate the real Raven tool registry, its safety metadata, and related wiring.
set -euo pipefail

cd "$(dirname "$0")/.."

echo "== Tool registry tests =="
cargo test -p odin-tools --all-targets

echo "== Permission, audit, and skill integration tests =="
cargo test -p odin-permissions -p odin-audit -p odin-skills --all-targets

echo "== Registry validation =="
cargo run --quiet --bin raven -- tools validate
cargo run --quiet --bin raven -- tools doctor

echo "== Dangerous-tool dry runs =="
cargo run --quiet --bin raven -- tools test shell --dry-run --args '{"command":"echo safe"}'
cargo run --quiet --bin raven -- tools test git --dry-run --args '{"command":"status"}'
cargo run --quiet --bin raven -- tools test file_write --dry-run --args '{"path":"/tmp/raven-tool-validation.txt","content":"test"}'

test -s docs/tools.md
echo "Tool validation passed."
