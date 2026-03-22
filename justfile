# Nocelium development tasks

default:
    @just --list

# --- Agent ---

# Run agent interactively (stdio)
agent *ARGS:
    cargo run -- {{ARGS}}

# Run agent with debug logging
agent-debug *ARGS:
    RUST_LOG=nocelium=debug cargo run -- {{ARGS}}

# Run agent with Telegram channel
agent-telegram *ARGS:
    cargo run --features telegram -- {{ARGS}}

# --- Service ---

# Install systemd user service
service-install *ARGS:
    cargo run -- service install {{ARGS}}

# Start the service
service-start:
    systemctl --user enable --now nocelium

# Stop the service
service-stop:
    systemctl --user stop nocelium

# Show service status
service-status:
    systemctl --user status nocelium

# Follow service logs
service-logs:
    journalctl --user -u nocelium -f

# Uninstall the service
service-uninstall:
    cargo run -- service uninstall

# --- Install ---

# Install nocelium binary to ~/.cargo/bin
install:
    cargo install --path . --features telegram

# --- Development ---

# Fast compile check
check:
    cargo check --workspace

# Check with Telegram feature
check-all:
    cargo check --workspace --features telegram

# Lint with clippy
lint:
    cargo clippy --workspace -- -D warnings

# Run all tests
test:
    cargo test --workspace

# Full CI pipeline
ci: check lint test

# Build release binary
build-release:
    cargo build --release --features telegram

# Check a single crate
check-crate CRATE:
    cargo check -p {{CRATE}}

# Test a single crate
test-crate CRATE:
    cargo test -p {{CRATE}}

# Clean build artifacts
clean:
    cargo clean
