# Nocelium

A Nostr-native AI agent runtime with collective memory and integrated payments.

**No**(str) + my**celium** — distributed intelligence growing through a shared substrate.

## Features

- **Nostr-native identity** — agent = secp256k1 keypair (npub)
- **Collective memory** — shared semantic search via Nomen
- **Tool use** — shell, filesystem, memory, scheduling
- **Multi-channel** — stdio, Telegram, Nostr (NIP-29/DMs)
- **Bitcoin payments** — Cashu eCash for LLM inference and tools
- **Scheduling** — cron, interval, one-shot, event-driven triggers

## Quick Start

```bash
# Build
cargo build

# Generate identity
cargo run -- --gen-identity

# Run (interactive stdio)
OPENROUTER_API_KEY=sk-... cargo run

# Or with custom config
cargo run -- --config path/to/nocelium.toml
```

## Service Management

```bash
cargo run -- service install    # install systemd user service
cargo run -- service start
cargo run -- service stop
cargo run -- service status
cargo run -- service logs -f
```

## Project Structure

```
nocelium/
├── crates/
│   ├── nocelium-core/       # agent loop, config, identity
│   ├── nocelium-tools/      # shell, filesystem tools
│   ├── nocelium-memory/     # Nomen memory client
│   └── nocelium-channels/   # I/O channels (stdio, telegram, nostr)
├── docs/                    # specs and reference
└── config/nocelium.toml     # default config
```

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md) — component map, interfaces, data flow
- [docs/config.md](docs/config.md) — configuration reference
- [docs/channels.md](docs/channels.md) — channel system
- [docs/tools.md](docs/tools.md) — tool system
- [docs/memory.md](docs/memory.md) — Nomen integration
- [docs/nomen-contract.md](docs/nomen-contract.md) — Nomen API contract & version tracking
- [docs/event-sources.md](docs/event-sources.md) — event sources (cron, webhooks, Nostr)
- [docs/payments.md](docs/payments.md) — payment integration
