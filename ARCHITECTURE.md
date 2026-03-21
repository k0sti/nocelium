# Architecture

## Overview

Nocelium is a Nostr-native AI agent runtime. Single Rust binary, workspace of focused crates.

## Directory Map

```
nocelium/
├── src/main.rs                     # binary entrypoint, CLI, wires crates together
├── config/nocelium.toml            # default config
├── crates/
│   ├── nocelium-core/              # agent loop, config, identity, service management
│   │   ├── src/agent.rs            # agent builder + run loop
│   │   ├── src/config.rs           # TOML config structs
│   │   ├── src/identity.rs         # Nostr keypair management
│   │   ├── src/service.rs          # systemd service install/start/stop/logs
│   │   └── src/lib.rs              # re-exports
│   ├── nocelium-tools/             # tool implementations (one file per tool)
│   │   ├── src/shell.rs            # shell command execution
│   │   ├── src/filesystem.rs       # read/write files
│   │   └── src/lib.rs              # re-exports
│   ├── nocelium-memory/            # Nomen client (search, store, consolidate)
│   │   └── src/lib.rs
│   ├── nocelium-channels/          # input/output channels
│   │   ├── src/stdio.rs            # interactive terminal
│   │   └── src/lib.rs              # re-exports (future: telegram.rs, nostr.rs)
│   └── nocelium-providers/         # (future) custom LLM providers
├── scripts/
│   └── check.sh                    # one-command CI: check + clippy + test
├── justfile                        # task runner (just check, just test, just run)
├── ARCHITECTURE.md                 # this file
├── AGENTS.md                       # conventions for AI agents working on this repo
└── CLAUDE.md                       # Claude Code specific instructions
```

## Crate Dependency Graph

```
nocelium (binary)
  ├── nocelium-core (agent loop, config, identity, service)
  ├── nocelium-tools (shell, filesystem, http, web_search)
  ├── nocelium-memory (Nomen HTTP client)
  └── nocelium-channels (stdio, telegram, nostr)
```

No circular dependencies. Each crate is independently testable via `cargo test -p <crate>`.

## Key Patterns

- **Agent loop**: `receive → think → act → remember` in `nocelium-core/src/agent.rs`
- **Tools**: Implement Rig's `Tool` trait, one tool per file
- **Config**: Single `nocelium.toml`, deserialized into typed structs
- **Identity**: Agent = Nostr keypair (secp256k1). Stored in config-specified path.
- **Service**: `nocelium service {install,start,stop,restart,status,logs}` wraps systemd

## Build

```bash
just check    # cargo check (fast, catches most errors)
just test     # cargo nextest run (or cargo test)
just lint     # cargo clippy
just ci       # all of the above
just run      # cargo run
```
