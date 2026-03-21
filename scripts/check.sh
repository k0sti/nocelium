#!/usr/bin/env bash
# One-command CI validation
set -euo pipefail

echo "=== cargo check ==="
cargo check --workspace

echo "=== cargo clippy ==="
cargo clippy --workspace -- -D warnings

echo "=== cargo test ==="
cargo test --workspace

echo "=== ALL GOOD ==="
