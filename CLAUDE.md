# CLAUDE.md — Claude Code Instructions

Read AGENTS.md for full conventions. This file has Claude-specific shortcuts.

## Build & Validate

```bash
just ci       # always run this before finishing
just check    # quick type-check during iteration
```

## Workflow

1. Read `ARCHITECTURE.md` to understand where things are
2. Make changes in the relevant crate
3. Run `just check` after each significant change
4. Run `just ci` before committing
5. Commit with conventional commit messages (`feat:`, `fix:`, `refactor:`)

## Key Files

- `src/main.rs` — CLI entrypoint
- `crates/nocelium-core/src/agent.rs` — agent loop
- `crates/nocelium-core/src/config.rs` — config structs
- `config/nocelium.toml` — runtime config

## Common Tasks

**Add a tool:** New file in `crates/nocelium-tools/src/`, impl `Tool` trait, register in agent builder.

**Add a channel:** New file in `crates/nocelium-channels/src/`, impl `Channel` trait.

**Change config:** Update struct in `config.rs`, update `nocelium.toml`, update `ARCHITECTURE.md`.

## Testing

```bash
cargo test -p nocelium-core     # test one crate
cargo test                      # test all
```
