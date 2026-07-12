.PHONY: all build check test bench lint clean docs run

all: build

build:
	cargo build --release

check:
	cargo check --workspace --all-targets

test:
	cargo test --workspace --all-targets

test-verbose:
	cargo test --workspace -- --nocapture

bench:
	cargo bench --no-run

validate-tools:
	./scripts/validate-tools.sh

lint:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings

lint-fix:
	cargo fmt --all
	cargo clippy --workspace --all-targets --fix --allow-dirty

clean:
	cargo clean

docs:
	cargo doc --no-deps --open

run:
	cargo run --release

# Run a specific task
task:
	cargo run --release -- run --goal "$(GOAL)"

# Start the HTTP API server
serve:
	cargo run --release -- serve

# Watch for changes
watch:
	cargo watch -x check -x test
