# Nocelium development tasks

default: check

# Fast compile check (no codegen)
check:
    cargo check --workspace

# Lint with clippy
lint:
    cargo clippy --workspace -- -D warnings

# Run all tests
test:
    cargo test --workspace

# Full CI pipeline
ci: check lint test

# Run the agent
run *ARGS:
    cargo run -- {{ARGS}}

# Run with debug logging
run-debug *ARGS:
    RUST_LOG=nocelium=debug cargo run -- {{ARGS}}

# Check a single crate
check-crate CRATE:
    cargo check -p {{CRATE}}

# Test a single crate
test-crate CRATE:
    cargo test -p {{CRATE}}

# Build release binary
build-release:
    cargo build --release

# Clean build artifacts
clean:
    cargo clean
