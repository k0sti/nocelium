# Architecture

## Overview

Nocelium is a Nostr-native AI agent runtime. Single Rust binary, workspace of focused crates. Agents are Nostr identities with collective memory, autonomous scheduling, and Bitcoin payments.

## System Map

```mermaid
graph TD
    TOML["nocelium.toml<br/>(bootstrap: identity + nomen)"]

    subgraph core["nocelium-core"]
        AGENT["Agent Loop<br/>select! on channels + events"]
        IDENTITY["Identity<br/>(Nostr keypair)"]
        EVENTS["Event Queue<br/>(single mpsc)"]
    end

    subgraph sources["Event Sources"]
        CRON["CronSource<br/>(timers, cron)"]
        WH["WebhookSource<br/>(HTTP POST)"]
        NSRC["NostrSource<br/>(relay filters)"]
    end

    subgraph crates["Supporting Crates"]
        TOOLS["nocelium-tools<br/>Tool impls"]
        MEM["nocelium-memory<br/>NomenClient"]
        CHANLIB["nocelium-channels<br/>Channel trait"]
    end

    subgraph external["External Services"]
        LLM["LLM Provider<br/>(OpenRouter / Routstr)"]
        NOMEN["Nomen<br/>(config + memory + cron)"]
        NOSTR["Nostr Relays"]
        TG["Telegram API"]
    end

    TOML --> IDENTITY
    TOML --> MEM
    MEM --> NOMEN
    NOMEN --> AGENT
    NOMEN --> CRON
    NOMEN --> WH
    NOMEN --> NSRC
    CRON --> EVENTS
    WH --> EVENTS
    NSRC --> EVENTS
    EVENTS --> AGENT
    CHANLIB --> AGENT
    AGENT --> TOOLS
    AGENT --> MEM
    AGENT --> LLM
    CHANLIB --> TG
    CHANLIB --> NOSTR
    IDENTITY --> NOSTR
```

## Input / Output Model

```
Inbound:
  Channels (conversations)     → Agent Loop (receive)
  EventSources (cron, webhooks → Event Queue → Agent Loop (select!)
    Nostr filters, polls)

Outbound:
  Agent → Tools (shell, http, nostr_publish, nomen_store, etc.)
  No hardcoded routing — the LLM decides how to act.

Config + State:
  Bootstrap TOML → identity + Nomen connection only
  Everything else → Nomen (config/*, cron/*, memories)
```

## Interfaces

| Boundary | Trait / Type | Called by | Methods | Status |
|---|---|---|---|---|
| core → channels | `Channel` trait | agent loop | `receive()`, `send()`, `send_chunk()`, `flush()` | ✅ impl: stdio |
| core → tools | rig `Tool` trait | LLM via rig | `definition()`, `call()` | ✅ impl: shell, read, write |
| core → memory | `MemoryClient` (wraps `nomen-wire::ReconnectingClient`) | agent loop + tools + event sources | `search()`, `store()`, `get()`, `list()`, `delete()`, `subscribe()` | 🔲 stub |
| core → LLM | rig `Agent` | agent loop | `prompt()`, `stream_prompt()` | ✅ OpenRouter |
| core → events | `EventSource` trait | tokio::spawn | `start(tx)` | 🔲 planned |
| binary → core | `Identity`, `build_agent()` | main.rs | direct calls | ✅ |

## Data Flow (Current)

```mermaid
sequenceDiagram
    participant Ch as Channel (stdio)
    participant Core as Agent Loop
    participant LLM as LLM (rig)
    participant Tools as Tools

    Note over Core: Build prompt: preamble + npub
    Ch->>Core: receive() → message
    Core->>LLM: prompt(message)
    LLM->>Tools: tool call (shell/read/write)
    Tools->>LLM: tool result
    LLM->>Core: response text
    Core->>Ch: send(response)
```

## Data Flow (Planned)

```mermaid
sequenceDiagram
    participant Ch as Channel
    participant Src as EventSource (cron/webhook/nostr)
    participant Core as Agent Loop
    participant Mem as NomenClient
    participant LLM as LLM
    participant Tools as Tools

    Note over Core: Startup: read config/* from Nomen, load pinned memories, start event sources

    alt Conversation
        Ch->>Core: receive() → message
    else Reactive event
        Src->>Core: IncomingEvent via mpsc
    end

    Core->>Mem: search(context)
    Mem->>Core: relevant memories
    Core->>LLM: prompt(message + context)
    LLM->>Tools: tool calls (shell, http, nomen_store, etc.)
    Tools->>LLM: results
    LLM->>Core: response
    Note over Core: Agent uses tools for all outbound actions
```

## Startup Sequence

```mermaid
sequenceDiagram
    participant Bin as Binary
    participant TOML as nocelium.toml
    participant Nomen as Nomen
    participant Sources as Event Sources
    participant Agent as Agent Loop

    Bin->>TOML: load identity + nomen connection
    Bin->>Nomen: connect (socket)
    Bin->>Nomen: read config/* topics
    Bin->>Nomen: read cron/* topics
    Bin->>Nomen: load pinned memories → preamble
    Bin->>Sources: start CronSource, WebhookSource, NostrSource
    Bin->>Agent: start loop (select! on channels + event queue)
```

## Prompt Assembly

Currently in `build_agent()`:
```
config.agent.preamble + "\n\nYour Nostr identity (npub): " + identity.npub()
```

Planned: preamble from `config/agent` in Nomen + pinned memories + npub. Per-message: message + `search()` results.

## Config Model

**Bootstrap (nocelium.toml):**

| Field | Purpose |
|---|---|
| `identity.key_path` | Nostr keypair file |
| `nomen.socket_path` | Unix socket to Nomen |

**Everything else in Nomen:**

| Topic | Contents | Status |
|---|---|---|
| `config/agent` | preamble, max_tokens, streaming | 🔲 |
| `config/provider` | model, base_url, api_key | 🔲 |
| `config/channels/*` | telegram, nostr settings | 🔲 |
| `config/tools` | tool toggles | 🔲 |
| `config/events/*` | webhook, nostr filter settings | 🔲 |
| `cron/*` | scheduled tasks | 🔲 |

## Directory Map

```
nocelium/
├── src/main.rs                     # binary entrypoint, CLI
├── config/nocelium.toml            # bootstrap config (identity + nomen only)
├── crates/
│   ├── nocelium-core/              # agent loop, identity, event sources
│   ├── nocelium-tools/             # tool implementations (one per file)
│   ├── nocelium-memory/            # Nomen client
│   └── nocelium-channels/          # I/O channels
├── docs/
│   ├── event-sources.md            # unified event source spec (incl. cron)
│   ├── memory.md                   # Nomen integration spec
│   ├── nomen-contract.md           # Nomen API contract + version tracking
│   ├── channels.md                 # channel system spec
│   ├── tools.md                    # tool system spec
│   ├── payments.md                 # payment integration spec
│   └── config.md                   # configuration reference
├── scripts/check.sh
├── justfile
├── ARCHITECTURE.md                 # this file
├── AGENTS.md                       # agent conventions
└── CLAUDE.md                       # Claude Code instructions
```

## Build

```bash
just check    # cargo check
just test     # cargo nextest run
just lint     # cargo clippy
just ci       # all of the above
just run      # cargo run
```

---
*Agents: update this file when adding/removing crates, changing public traits, or altering data flow. Update relevant docs/ specs when changing subsystem behavior.*
