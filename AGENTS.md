# AGENTS.md — AI Agent Conventions

## Quick Start

```bash
just check    # fast compile check
just ci       # full validation (check + clippy + test)
just run      # run the agent
```

## Project Structure

Read `ARCHITECTURE.md` for the full map. Key points:
- Cargo workspace with focused crates under `crates/`
- One tool per file in `nocelium-tools`
- Config in `config/nocelium.toml`

## Conventions

### Files
- Keep files under 300 lines. Split when they grow.
- One primary type/trait per file.
- `lib.rs` files are re-export hubs only — no logic.

### Error Handling
- Use `thiserror` for crate-level error types.
- Use `anyhow` only in the binary (`src/main.rs`) and tests.
- Every crate has its own `Error` enum in `error.rs`.

### Adding a Tool
1. Create `crates/nocelium-tools/src/tool_name.rs`
2. Implement Rig's `Tool` trait with `#[derive(Tool)]`
3. Add `pub mod tool_name;` to `crates/nocelium-tools/src/lib.rs`
4. Register in agent builder (`crates/nocelium-core/src/agent.rs`)

### Adding a Channel
1. Create `crates/nocelium-channels/src/channel_name.rs`
2. Implement the `Channel` trait
3. Re-export from `lib.rs`

### Testing
- Unit tests inline (`#[cfg(test)] mod tests`)
- Integration tests in `tests/` dirs per crate
- Run single crate: `cargo test -p nocelium-core`

### Validation Before Committing
Always run `just ci` before considering work done. The check order:
1. `cargo check` — type errors
2. `cargo clippy -- -D warnings` — lint
3. `cargo test` — tests pass

## Service Management

```bash
cargo run -- service install    # install systemd user service
cargo run -- service start      # start
cargo run -- service stop       # stop
cargo run -- service restart    # restart
cargo run -- service status     # structured status output
cargo run -- service logs       # recent logs (clean format)
cargo run -- service logs -f    # follow logs
```

## Don't

- Don't put logic in `lib.rs` — re-exports only
- Don't use `unwrap()` outside tests
- Don't add workspace-level `build.rs`
- Don't nest modules deeper than 2 levels
