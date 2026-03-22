# Telegram Channel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement TelegramChannel as a push-based channel producing `EventEnvelope` events through the unified dispatch system, with full Telegram capabilities (send, edit, delete, reactions, typing, pins, polls, buttons, location).

**Architecture:** Channels push `EventEnvelope` to a shared mpsc queue (same queue used by all event sources). Each envelope has a hierarchical dispatch key (`telegram:message:direct:12345`). The agent loop receives envelopes, matches dispatch rules, and routes to handlers or LLM agent turns. For this implementation, we use a simplified dispatcher (no Nomen yet — hardcode default AgentTurn for all events). TelegramChannel uses teloxide 0.17 with long polling. Telegram behind a cargo feature flag.

**Dependency note:** The spec places `EventEnvelope` in `nocelium-core/src/dispatch.rs`, but core already depends on channels. To avoid circular deps, `EventEnvelope` and `EventSource` live in `nocelium-channels/src/types.rs`. Core imports them from channels. Dispatch logic (rules, matching) lives in core.

**Tech Stack:** teloxide 0.17, tokio mpsc, thiserror, async-trait, glob pattern matching

---

## File Structure

| Action | File | Responsibility |
|--------|------|---------------|
| Create | `crates/nocelium-channels/src/error.rs` | Channel error types (thiserror) |
| Create | `crates/nocelium-channels/src/types.rs` | EventEnvelope, EventSource, OutboundMessage, ChannelCapabilities, SendResult, ChatType, Attachment, Button, Poll, Location, ChatInfo, TopicInfo, MemberInfo |
| Rewrite | `crates/nocelium-channels/src/lib.rs` | Channel + ChannelInfo traits, re-exports |
| Rewrite | `crates/nocelium-channels/src/stdio.rs` | StdioChannel (push-based, produces EventEnvelope) |
| Create | `crates/nocelium-channels/src/telegram.rs` | TelegramChannel with teloxide |
| Create | `crates/nocelium-core/src/dispatch.rs` | DispatchRule, DispatchAction, Dispatcher (pattern matching) |
| Modify | `crates/nocelium-channels/Cargo.toml` | Add teloxide, thiserror deps behind feature flag |
| Modify | `Cargo.toml` (workspace root) | Add teloxide workspace dep, enable telegram feature |
| Modify | `crates/nocelium-core/src/config.rs` | Add `allowed_senders` to TelegramConfig |
| Rewrite | `crates/nocelium-core/src/agent.rs` | Dispatch-based agent loop |
| Modify | `crates/nocelium-core/src/lib.rs` | Add `pub mod dispatch;` |
| Modify | `src/main.rs` | Start telegram channel, build dispatcher |

---

### Task 1: Add Dependencies

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/nocelium-channels/Cargo.toml`

- [ ] **Step 1: Add teloxide to workspace dependencies**

In root `Cargo.toml`, add to `[workspace.dependencies]`:
```toml
teloxide = { version = "0.17", features = ["macros"] }
```

- [ ] **Step 2: Update nocelium-channels Cargo.toml with feature-gated deps**

```toml
[dependencies]
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
anyhow = { workspace = true }
tracing = { workspace = true }
async-trait = { workspace = true }
thiserror = { workspace = true }

[dependencies.teloxide]
workspace = true
optional = true

[features]
default = []
telegram = ["teloxide"]
```

- [ ] **Step 3: Enable telegram feature in root binary**

In root `Cargo.toml` `[dependencies]` section, change:
```toml
nocelium-channels = { workspace = true, features = ["telegram"] }
```

- [ ] **Step 4: Run `just check`**

Expected: compiles

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/nocelium-channels/Cargo.toml
git commit -m "feat(channels): add teloxide dependency behind telegram feature flag"
```

---

### Task 2: Error Types

**Files:**
- Create: `crates/nocelium-channels/src/error.rs`
- Modify: `crates/nocelium-channels/src/lib.rs` (add module)

- [ ] **Step 1: Create error.rs**

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ChannelError {
    #[error("send failed: {0}")]
    SendFailed(String),

    #[error("not supported: {0}")]
    NotSupported(String),

    #[error("auth failed: {0}")]
    AuthFailed(String),

    #[error("rate limited, retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error("channel closed")]
    ChannelClosed,

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, ChannelError>;
```

- [ ] **Step 2: Add `pub mod error;` to lib.rs**

- [ ] **Step 3: Run `just check`**

- [ ] **Step 4: Commit**

```bash
git add crates/nocelium-channels/src/error.rs crates/nocelium-channels/src/lib.rs
git commit -m "feat(channels): add channel error types"
```

---

### Task 3: Types (EventEnvelope + Outbound)

**Files:**
- Create: `crates/nocelium-channels/src/types.rs`

- [ ] **Step 1: Create types.rs**

Contains `EventEnvelope`, `EventSource`, `OutboundMessage`, `SendResult`, `ChannelCapabilities`, and all supporting types. `EventEnvelope` is the unified event type — channels, cron, webhooks, Nostr subscriptions all produce this. Each envelope has a `dispatch_key` computed at creation time.

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;

// --- EventEnvelope (unified inbound event) ---

#[derive(Debug, Clone)]
pub struct EventEnvelope {
    // Dispatch
    pub dispatch_key: String,

    // Source
    pub source: EventSource,
    pub kind: String, // "message", "callback", "location_update", "voice", "media", "cron", "webhook"

    // Channel context (present for channel-originated events)
    pub channel: Option<String>,   // "telegram", "nostr", "stdio"
    pub chat_id: Option<String>,
    pub thread_id: Option<String>,
    pub sender_id: Option<String>,
    pub sender_name: Option<String>,
    pub sender_handle: Option<String>,
    pub chat_type: Option<ChatType>,
    pub group_subject: Option<String>,

    // Content
    pub text: Option<String>,       // message text (stripped of mentions)
    pub raw_text: Option<String>,   // original with mentions
    pub location: Option<Location>,
    pub callback_data: Option<String>,
    pub callback_query_id: Option<String>,
    pub attachments: Vec<Attachment>,
    pub metadata: Option<Value>,    // structured data (webhook payload, etc.)

    // Reply context
    pub reply_to_id: Option<String>,
    pub reply_to_text: Option<String>,
    pub reply_to_sender: Option<String>,

    // Mentions
    pub mentions: Vec<String>,
    pub was_mentioned: bool,

    // Timing
    pub timestamp: u64,
    pub message_id: Option<String>, // platform message ID (for reactions, replies)
}

impl EventEnvelope {
    /// Compute the hierarchical dispatch key from envelope fields.
    pub fn compute_dispatch_key(
        source: &EventSource,
        kind: &str,
        chat_type: Option<&ChatType>,
        chat_id: Option<&str>,
        sender_id: Option<&str>,
        thread_id: Option<&str>,
        callback_data: Option<&str>,
    ) -> String {
        match source {
            EventSource::Channel(ch) => match (chat_type, chat_id, sender_id) {
                (Some(ChatType::Direct), _, Some(sender)) => {
                    format!("{ch}:{kind}:direct:{sender}")
                }
                (_, Some(chat), _) => match thread_id {
                    Some(thread) => format!("{ch}:{kind}:{chat}:{thread}"),
                    None => format!("{ch}:{kind}:{chat}"),
                },
                _ => {
                    // For callbacks, include the callback data prefix
                    if kind == "callback" {
                        if let Some(data) = callback_data {
                            return format!("{ch}:callback:{data}");
                        }
                    }
                    format!("{ch}:{kind}")
                }
            },
            EventSource::Cron(id) => format!("cron:{id}"),
            EventSource::Webhook(name) => format!("webhook:{name}"),
            EventSource::Nostr(filter) => format!("nostr:{filter}"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum EventSource {
    Channel(String),  // "telegram", "nostr", "stdio"
    Cron(String),     // task id
    Webhook(String),  // source name
    Nostr(String),    // filter id
}

// --- Outbound ---

#[derive(Debug, Clone)]
pub struct OutboundMessage {
    pub chat_id: String,
    pub text: String,
    pub reply_to_id: Option<String>,
    pub thread_id: Option<String>,
    pub attachments: Vec<OutboundAttachment>,
    pub buttons: Option<Vec<Vec<Button>>>,
    pub silent: bool,
}

#[derive(Debug, Clone)]
pub struct SendResult {
    pub message_id: String,
    pub chat_id: String,
}

// --- Capabilities ---

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatType {
    Direct,
    Group,
    Thread,
}

#[derive(Debug, Clone)]
pub struct ChannelCapabilities {
    pub chat_types: Vec<ChatType>,
    pub media: bool,
    pub reactions: bool,
    pub reply: bool,
    pub edit: bool,
    pub delete: bool,
    pub threads: bool,
    pub buttons: bool,
    pub polls: bool,
    pub typing: bool,
    pub pins: bool,
    pub voice: bool,
    pub location: bool,
    pub live_location: bool,
}

// --- Attachments ---

#[derive(Debug, Clone)]
pub struct Attachment {
    pub kind: AttachmentKind,
    pub url: Option<String>,
    pub file_id: Option<String>,
    pub mime_type: Option<String>,
    pub file_name: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone)]
pub enum AttachmentKind {
    Photo,
    Video,
    Audio,
    Voice,
    Document,
    Sticker,
    Animation,
}

#[derive(Debug, Clone)]
pub struct OutboundAttachment {
    pub kind: AttachmentKind,
    pub data: AttachmentData,
    pub caption: Option<String>,
}

#[derive(Debug, Clone)]
pub enum AttachmentData {
    Url(String),
    FileId(String),
    Bytes { data: Vec<u8>, filename: String },
}

// --- Buttons / Polls / Location ---

#[derive(Debug, Clone)]
pub struct Button {
    pub text: String,
    pub callback_data: String,
}

#[derive(Debug, Clone)]
pub struct Poll {
    pub question: String,
    pub options: Vec<String>,
    pub is_anonymous: bool,
    pub allows_multiple: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
}

// --- ChannelInfo types ---

#[derive(Debug, Clone)]
pub struct ChatInfo {
    pub id: String,
    pub title: Option<String>,
    pub chat_type: ChatType,
    pub member_count: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct TopicInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct MemberInfo {
    pub user_id: String,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub is_admin: bool,
}
```

- [ ] **Step 2: Add `pub mod types;` to lib.rs**

- [ ] **Step 3: Run `just check`**

- [ ] **Step 4: Commit**

```bash
git add crates/nocelium-channels/src/types.rs crates/nocelium-channels/src/lib.rs
git commit -m "feat(channels): add EventEnvelope and channel types"
```

---

### Task 4: Rewrite Channel Trait

**Files:**
- Rewrite: `crates/nocelium-channels/src/lib.rs`

- [ ] **Step 1: Rewrite lib.rs with new Channel + ChannelInfo traits**

Key change from previous: `listen()` takes `mpsc::Sender<EventEnvelope>` (not InboundMessage). All optional methods have default no-op/error implementations.

```rust
//! Nocelium Channels — Message I/O abstraction
//!
//! Channels are the bidirectional I/O layer between messaging platforms and the
//! agent. Push-based: channels listen in background tasks and push EventEnvelope
//! to a shared mpsc queue — the same queue used by all event sources.

pub mod error;
pub mod stdio;
#[cfg(feature = "telegram")]
pub mod telegram;
pub mod types;

pub use error::{ChannelError, Result};
pub use types::*;

use async_trait::async_trait;
use tokio::sync::mpsc;

/// Bidirectional message channel.
///
/// Push-based: `listen()` spawns a background task that pushes `EventEnvelope`
/// to the shared queue. The dispatcher routes events by dispatch key.
#[async_trait]
pub trait Channel: Send + Sync {
    fn name(&self) -> &str;
    fn capabilities(&self) -> ChannelCapabilities;

    /// Start listening. Spawns a background task that pushes EventEnvelope
    /// to the shared queue (same queue used by cron, webhooks, etc.).
    async fn listen(&self, tx: mpsc::Sender<EventEnvelope>) -> Result<()>;

    /// Send an outbound message.
    async fn send(&self, message: &OutboundMessage) -> Result<SendResult>;

    async fn edit(&self, _chat_id: &str, _message_id: &str, _text: &str) -> Result<()> {
        Err(ChannelError::NotSupported("edit".into()))
    }

    async fn delete(&self, _chat_id: &str, _message_id: &str) -> Result<()> {
        Err(ChannelError::NotSupported("delete".into()))
    }

    async fn add_reaction(
        &self, _chat_id: &str, _message_id: &str, _emoji: &str,
    ) -> Result<()> {
        Err(ChannelError::NotSupported("add_reaction".into()))
    }

    async fn remove_reaction(
        &self, _chat_id: &str, _message_id: &str, _emoji: &str,
    ) -> Result<()> {
        Err(ChannelError::NotSupported("remove_reaction".into()))
    }

    async fn start_typing(&self, _chat_id: &str) -> Result<()> {
        Err(ChannelError::NotSupported("start_typing".into()))
    }

    async fn stop_typing(&self, _chat_id: &str) -> Result<()> {
        Ok(())
    }

    async fn pin_message(&self, _chat_id: &str, _message_id: &str) -> Result<()> {
        Err(ChannelError::NotSupported("pin_message".into()))
    }

    async fn unpin_message(&self, _chat_id: &str, _message_id: &str) -> Result<()> {
        Err(ChannelError::NotSupported("unpin_message".into()))
    }

    async fn send_poll(&self, _chat_id: &str, _poll: &Poll) -> Result<SendResult> {
        Err(ChannelError::NotSupported("send_poll".into()))
    }

    async fn send_location(
        &self, _chat_id: &str, _lat: f64, _lon: f64,
        _live_period: Option<u32>, _reply_to: Option<&str>,
    ) -> Result<SendResult> {
        Err(ChannelError::NotSupported("send_location".into()))
    }

    async fn edit_location(
        &self, _chat_id: &str, _message_id: &str, _lat: f64, _lon: f64,
    ) -> Result<()> {
        Err(ChannelError::NotSupported("edit_location".into()))
    }

    async fn stop_location(&self, _chat_id: &str, _message_id: &str) -> Result<()> {
        Err(ChannelError::NotSupported("stop_location".into()))
    }

    async fn health_check(&self) -> bool {
        true
    }
}

/// Optional extension for channels that support metadata queries.
#[async_trait]
pub trait ChannelInfo: Channel {
    async fn list_chats(&self) -> Result<Vec<ChatInfo>>;
    async fn list_topics(&self, chat_id: &str) -> Result<Vec<TopicInfo>>;
    async fn get_chat(&self, chat_id: &str) -> Result<ChatInfo>;
    async fn get_member(&self, chat_id: &str, user_id: &str) -> Result<MemberInfo>;
}
```

- [ ] **Step 2: Run `cargo check -p nocelium-channels`**

Expected: channels crate compiles. StdioChannel will fail (old trait). Fix in next task.

- [ ] **Step 3: Commit**

```bash
git add crates/nocelium-channels/src/lib.rs
git commit -m "feat(channels): rewrite Channel trait for EventEnvelope dispatch"
```

---

### Task 5: Update StdioChannel

**Files:**
- Rewrite: `crates/nocelium-channels/src/stdio.rs`

- [ ] **Step 1: Rewrite stdio.rs**

StdioChannel now produces `EventEnvelope` with `dispatch_key: "stdio:message"`, `kind: "message"`, `source: EventSource::Channel("stdio")`.

```rust
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, Mutex};

use crate::{
    Channel, ChannelCapabilities, ChannelError, ChatType, EventEnvelope, EventSource,
    OutboundMessage, Result, SendResult,
};

pub struct StdioChannel {
    writer: Arc<Mutex<tokio::io::Stdout>>,
}

impl StdioChannel {
    pub fn new() -> Self {
        Self {
            writer: Arc::new(Mutex::new(tokio::io::stdout())),
        }
    }
}

#[async_trait]
impl Channel for StdioChannel {
    fn name(&self) -> &str {
        "stdio"
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            chat_types: vec![ChatType::Direct],
            media: false,
            reactions: false,
            reply: false,
            edit: false,
            delete: false,
            threads: false,
            buttons: false,
            polls: false,
            typing: false,
            pins: false,
            voice: false,
            location: false,
            live_location: false,
        }
    }

    async fn listen(&self, tx: mpsc::Sender<EventEnvelope>) -> Result<()> {
        let writer = self.writer.clone();

        tokio::spawn(async move {
            let mut reader = BufReader::new(tokio::io::stdin());
            let mut counter: u64 = 0;

            loop {
                {
                    let mut w = writer.lock().await;
                    let _ = w.write_all(b"\n> ").await;
                    let _ = w.flush().await;
                }

                let mut line = String::new();
                match reader.read_line(&mut line).await {
                    Ok(0) => break,
                    Ok(_) => {
                        let text = line.trim().to_string();
                        if text.is_empty() {
                            continue;
                        }
                        counter += 1;
                        let now = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();

                        let source = EventSource::Channel("stdio".into());
                        let dispatch_key = EventEnvelope::compute_dispatch_key(
                            &source, "message", None, None, None, None, None,
                        );

                        let envelope = EventEnvelope {
                            dispatch_key,
                            source,
                            kind: "message".into(),
                            channel: Some("stdio".into()),
                            chat_id: Some("stdio".into()),
                            thread_id: None,
                            sender_id: Some("local".into()),
                            sender_name: None,
                            sender_handle: None,
                            chat_type: Some(ChatType::Direct),
                            group_subject: None,
                            text: Some(text),
                            raw_text: None,
                            location: None,
                            callback_data: None,
                            callback_query_id: None,
                            attachments: vec![],
                            metadata: None,
                            reply_to_id: None,
                            reply_to_text: None,
                            reply_to_sender: None,
                            mentions: vec![],
                            was_mentioned: false,
                            timestamp: now,
                            message_id: Some(counter.to_string()),
                        };

                        if tx.send(envelope).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(())
    }

    async fn send(&self, message: &OutboundMessage) -> Result<SendResult> {
        let mut w = self.writer.lock().await;
        w.write_all(b"\n")
            .await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;
        w.write_all(message.text.as_bytes())
            .await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;
        w.write_all(b"\n")
            .await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;
        w.flush()
            .await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;

        Ok(SendResult {
            message_id: "0".into(),
            chat_id: message.chat_id.clone(),
        })
    }
}
```

- [ ] **Step 2: Run `cargo check -p nocelium-channels`**

Expected: channels crate compiles fully.

- [ ] **Step 3: Commit**

```bash
git add crates/nocelium-channels/src/stdio.rs
git commit -m "feat(channels): update StdioChannel for EventEnvelope dispatch"
```

---

### Task 6: Implement TelegramChannel

**Files:**
- Create: `crates/nocelium-channels/src/telegram.rs`

- [ ] **Step 1: Create telegram.rs**

TelegramChannel produces different `EventEnvelope.kind` values per Telegram update type: `"message"`, `"callback"`, `"location_update"`, `"voice"`, `"media"`. Dispatch keys follow the spec format (e.g., `telegram:message:direct:60996061`, `telegram:message:-1001234:42`). Includes sender allowlisting, callback auto-answer, mention detection, and full Channel trait + ChannelInfo implementation.

```rust
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use teloxide::prelude::*;
use teloxide::types::{
    ChatAction, InlineKeyboardButton, InlineKeyboardMarkup, InputPollOption,
    MessageId, ParseMode, ReactionEmoji, ReactionType,
};
use tokio::sync::mpsc;

use crate::{
    Channel, ChannelCapabilities, ChannelError, ChannelInfo, ChatInfo, ChatType,
    EventEnvelope, EventSource, Location, MemberInfo, OutboundMessage,
    Poll as ChannelPoll, Result, SendResult, TopicInfo,
};

pub struct TelegramChannel {
    bot: Bot,
    allowed_senders: Vec<String>,
}

impl TelegramChannel {
    pub fn new(token: &str, allowed_senders: Vec<String>) -> Self {
        Self {
            bot: Bot::new(token),
            allowed_senders,
        }
    }

    pub fn from_env(allowed_senders: Vec<String>) -> Self {
        Self {
            bot: Bot::from_env(),
            allowed_senders,
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn tg_chat_type(chat: &teloxide::types::Chat) -> ChatType {
    if chat.is_private() {
        ChatType::Direct
    } else {
        ChatType::Group
    }
}

fn is_allowed(user: &teloxide::types::User, allowed: &[String]) -> bool {
    if allowed.is_empty() {
        return true;
    }
    let uid = user.id.0.to_string();
    if allowed.contains(&uid) {
        return true;
    }
    if let Some(ref username) = user.username {
        if allowed.contains(username) {
            return true;
        }
    }
    false
}

/// Detect if the bot was mentioned in message entities.
fn detect_bot_mention(msg: &Message, bot_username: &Option<String>) -> (Vec<String>, bool) {
    let mut mentions = Vec::new();
    let mut was_mentioned = false;

    if let Some(entities) = msg.entities() {
        let text = msg.text().unwrap_or("");
        for entity in entities {
            if entity.kind == teloxide::types::MessageEntityKind::Mention {
                let start = entity.offset;
                let end = start + entity.length;
                if let Some(mention) = text.get(start..end) {
                    let clean = mention.trim_start_matches('@').to_string();
                    if let Some(ref bot_name) = bot_username {
                        if clean.eq_ignore_ascii_case(bot_name) {
                            was_mentioned = true;
                        }
                    }
                    mentions.push(clean);
                }
            }
        }
    }

    (mentions, was_mentioned)
}

/// Strip bot mention from message text.
fn strip_bot_mention(text: &str, bot_username: &Option<String>) -> String {
    if let Some(ref name) = bot_username {
        let pattern = format!("@{name}");
        text.replace(&pattern, "")
            .replace(&pattern.to_lowercase(), "")
            .trim()
            .to_string()
    } else {
        text.to_string()
    }
}

/// Determine event kind from a Telegram message.
fn event_kind_for_msg(msg: &Message) -> &'static str {
    if msg.location().is_some() {
        "location_update"
    } else if msg.voice().is_some() {
        "voice"
    } else if msg.photo().is_some()
        || msg.video().is_some()
        || msg.document().is_some()
        || msg.animation().is_some()
    {
        "media"
    } else {
        "message"
    }
}

fn msg_to_envelope(msg: &Message, bot_username: &Option<String>) -> Option<EventEnvelope> {
    let from = msg.from.as_ref()?;
    let raw_text = msg.text().or_else(|| msg.caption()).map(|t| t.to_string());
    let kind = event_kind_for_msg(msg);

    let chat_type = tg_chat_type(&msg.chat);
    let chat_id = msg.chat.id.0.to_string();
    let sender_id = from.id.0.to_string();
    let thread_id = msg.thread_id.map(|t| t.0.to_string());

    let (mentions, was_mentioned) = detect_bot_mention(msg, bot_username);
    let text = raw_text
        .as_deref()
        .map(|t| strip_bot_mention(t, bot_username));

    let source = EventSource::Channel("telegram".into());
    let dispatch_key = EventEnvelope::compute_dispatch_key(
        &source,
        kind,
        Some(&chat_type),
        Some(&chat_id),
        Some(&sender_id),
        thread_id.as_deref(),
        None,
    );

    let sender_name = {
        let first = &from.first_name;
        match &from.last_name {
            Some(last) => Some(format!("{first} {last}")),
            None => Some(first.clone()),
        }
    };

    let location = msg.location().map(|loc| Location {
        latitude: loc.latitude,
        longitude: loc.longitude,
    });

    Some(EventEnvelope {
        dispatch_key,
        source,
        kind: kind.into(),
        channel: Some("telegram".into()),
        chat_id: Some(chat_id),
        thread_id,
        sender_id: Some(sender_id),
        sender_name,
        sender_handle: from.username.clone(),
        chat_type: Some(chat_type),
        group_subject: msg.chat.title().map(|t| t.to_string()),
        text,
        raw_text,
        location,
        callback_data: None,
        callback_query_id: None,
        attachments: vec![],
        metadata: None,
        reply_to_id: msg.reply_to_message().map(|r| r.id.0.to_string()),
        reply_to_text: msg
            .reply_to_message()
            .and_then(|r| r.text().map(|t| t.to_string())),
        reply_to_sender: msg
            .reply_to_message()
            .and_then(|r| r.from.as_ref().map(|u| u.id.0.to_string())),
        mentions,
        was_mentioned,
        timestamp: now_secs(),
        message_id: Some(msg.id.0.to_string()),
    })
}

fn callback_to_envelope(q: &CallbackQuery) -> Option<EventEnvelope> {
    let from = &q.from;
    let (chat_id, message_id) = match q.message.as_ref()? {
        teloxide::types::MaybeInaccessibleMessage::Regular(m) => {
            (m.chat.id.0.to_string(), m.id.0.to_string())
        }
        teloxide::types::MaybeInaccessibleMessage::Inaccessible(m) => {
            (m.chat.id.0.to_string(), m.id.0.to_string())
        }
    };

    let source = EventSource::Channel("telegram".into());
    let dispatch_key = EventEnvelope::compute_dispatch_key(
        &source,
        "callback",
        None,
        Some(&chat_id),
        Some(&from.id.0.to_string()),
        None,
        q.data.as_deref(),
    );

    let sender_name = {
        let first = &from.first_name;
        match &from.last_name {
            Some(last) => Some(format!("{first} {last}")),
            None => Some(first.clone()),
        }
    };

    Some(EventEnvelope {
        dispatch_key,
        source,
        kind: "callback".into(),
        channel: Some("telegram".into()),
        chat_id: Some(chat_id),
        thread_id: None,
        sender_id: Some(from.id.0.to_string()),
        sender_name,
        sender_handle: from.username.clone(),
        chat_type: None,
        group_subject: None,
        text: None,
        raw_text: None,
        location: None,
        callback_data: q.data.clone(),
        callback_query_id: Some(q.id.clone()),
        attachments: vec![],
        metadata: None,
        reply_to_id: None,
        reply_to_text: None,
        reply_to_sender: None,
        mentions: vec![],
        was_mentioned: false,
        timestamp: now_secs(),
        message_id: Some(message_id),
    })
}

#[async_trait]
impl Channel for TelegramChannel {
    fn name(&self) -> &str {
        "telegram"
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            chat_types: vec![ChatType::Direct, ChatType::Group],
            media: true,
            reactions: true,
            reply: true,
            edit: true,
            delete: true,
            threads: true,
            buttons: true,
            polls: true,
            typing: true,
            pins: true,
            voice: true,
            location: true,
            live_location: true,
        }
    }

    async fn listen(&self, tx: mpsc::Sender<EventEnvelope>) -> Result<()> {
        let bot = self.bot.clone();
        let allowed = self.allowed_senders.clone();

        // Get bot username for mention detection
        let me = bot.get_me().await.map_err(|e| {
            ChannelError::AuthFailed(format!("Failed to get bot info: {e}"))
        })?;
        let bot_username = me.username.clone();

        let tx_msg = tx.clone();
        let tx_cb = tx;
        let allowed_cb = allowed.clone();
        let bot_username_cb = bot_username.clone();

        let handler = dptree::entry()
            .branch(
                Update::filter_message().endpoint(move |msg: Message| {
                    let tx = tx_msg.clone();
                    let allowed = allowed.clone();
                    let bot_username = bot_username.clone();
                    async move {
                        if let Some(from) = msg.from.as_ref() {
                            if !is_allowed(from, &allowed) {
                                tracing::debug!(sender = %from.id, "Ignoring non-allowed sender");
                                return respond(());
                            }
                        }
                        if let Some(envelope) = msg_to_envelope(&msg, &bot_username) {
                            let _ = tx.send(envelope).await;
                        }
                        respond(())
                    }
                }),
            )
            .branch(
                Update::filter_callback_query().endpoint(move |bot: Bot, q: CallbackQuery| {
                    let tx = tx_cb.clone();
                    let allowed = allowed_cb.clone();
                    async move {
                        if !is_allowed(&q.from, &allowed) {
                            return respond(());
                        }
                        // Auto-answer callback query
                        let _ = bot.answer_callback_query(&q.id).await;
                        if let Some(envelope) = callback_to_envelope(&q) {
                            let _ = tx.send(envelope).await;
                        }
                        respond(())
                    }
                }),
            );

        tokio::spawn(async move {
            Dispatcher::builder(bot, handler)
                .default_handler(|upd| async move {
                    tracing::trace!("Unhandled telegram update: {:?}", upd.kind);
                })
                .enable_ctrlc_handler()
                .build()
                .dispatch()
                .await;
        });

        tracing::info!("Telegram channel listening (long polling)");
        Ok(())
    }

    async fn send(&self, message: &OutboundMessage) -> Result<SendResult> {
        let chat_id = ChatId(
            message.chat_id.parse::<i64>()
                .map_err(|e| ChannelError::SendFailed(format!("invalid chat_id: {e}")))?,
        );

        let mut req = self.bot.send_message(chat_id, &message.text);
        req = req.parse_mode(ParseMode::MarkdownV2);

        if let Some(ref reply_id) = message.reply_to_id {
            if let Ok(id) = reply_id.parse::<i32>() {
                req = req.reply_to_message_id(MessageId(id));
            }
        }

        if message.silent {
            req = req.disable_notification(true);
        }

        if let Some(ref rows) = message.buttons {
            let kb: Vec<Vec<InlineKeyboardButton>> = rows
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|b| InlineKeyboardButton::callback(&b.text, &b.callback_data))
                        .collect()
                })
                .collect();
            req = req.reply_markup(InlineKeyboardMarkup::new(kb));
        }

        let sent = req.await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;

        Ok(SendResult {
            message_id: sent.id.0.to_string(),
            chat_id: message.chat_id.clone(),
        })
    }

    async fn edit(&self, chat_id: &str, message_id: &str, text: &str) -> Result<()> {
        let cid = ChatId(chat_id.parse::<i64>()
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?);
        let mid = MessageId(message_id.parse::<i32>()
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?);

        self.bot.edit_message_text(cid, mid, text)
            .parse_mode(ParseMode::MarkdownV2)
            .await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, chat_id: &str, message_id: &str) -> Result<()> {
        let cid = ChatId(chat_id.parse::<i64>()
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?);
        let mid = MessageId(message_id.parse::<i32>()
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?);

        self.bot.delete_message(cid, mid).await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;
        Ok(())
    }

    async fn add_reaction(&self, chat_id: &str, message_id: &str, emoji: &str) -> Result<()> {
        let cid = ChatId(chat_id.parse::<i64>()
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?);
        let mid = MessageId(message_id.parse::<i32>()
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?);

        let reaction = ReactionType::Emoji {
            emoji: ReactionEmoji::from(emoji.to_string()),
        };
        self.bot.set_message_reaction(cid, mid)
            .reaction(vec![reaction])
            .await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;
        Ok(())
    }

    async fn start_typing(&self, chat_id: &str) -> Result<()> {
        let cid = ChatId(chat_id.parse::<i64>()
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?);
        self.bot.send_chat_action(cid, ChatAction::Typing).await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;
        Ok(())
    }

    async fn stop_typing(&self, _chat_id: &str) -> Result<()> {
        Ok(()) // Telegram typing auto-expires
    }

    async fn pin_message(&self, chat_id: &str, message_id: &str) -> Result<()> {
        let cid = ChatId(chat_id.parse::<i64>()
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?);
        let mid = MessageId(message_id.parse::<i32>()
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?);
        self.bot.pin_chat_message(cid, mid).await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;
        Ok(())
    }

    async fn unpin_message(&self, chat_id: &str, message_id: &str) -> Result<()> {
        let cid = ChatId(chat_id.parse::<i64>()
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?);
        let mid = MessageId(message_id.parse::<i32>()
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?);
        self.bot.unpin_chat_message(cid)
            .message_id(mid)
            .await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;
        Ok(())
    }

    async fn send_poll(&self, chat_id: &str, poll: &ChannelPoll) -> Result<SendResult> {
        let cid = ChatId(chat_id.parse::<i64>()
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?);
        let options: Vec<InputPollOption> =
            poll.options.iter().map(|o| InputPollOption::from(o.as_str())).collect();
        let sent = self.bot.send_poll(cid, &poll.question, options)
            .is_anonymous(poll.is_anonymous)
            .allows_multiple_answers(poll.allows_multiple)
            .await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;
        Ok(SendResult {
            message_id: sent.id.0.to_string(),
            chat_id: chat_id.into(),
        })
    }

    async fn send_location(
        &self, chat_id: &str, lat: f64, lon: f64,
        live_period: Option<u32>, reply_to: Option<&str>,
    ) -> Result<SendResult> {
        let cid = ChatId(chat_id.parse::<i64>()
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?);
        let mut req = self.bot.send_location(cid, lat, lon);
        if let Some(period) = live_period {
            req = req.live_period(period);
        }
        if let Some(reply_id) = reply_to {
            if let Ok(id) = reply_id.parse::<i32>() {
                req = req.reply_to_message_id(MessageId(id));
            }
        }
        let sent = req.await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;
        Ok(SendResult {
            message_id: sent.id.0.to_string(),
            chat_id: chat_id.into(),
        })
    }

    async fn edit_location(
        &self, chat_id: &str, message_id: &str, lat: f64, lon: f64,
    ) -> Result<()> {
        let cid = ChatId(chat_id.parse::<i64>()
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?);
        let mid = MessageId(message_id.parse::<i32>()
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?);
        self.bot.edit_message_live_location(cid, mid, lat, lon).await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;
        Ok(())
    }

    async fn stop_location(&self, chat_id: &str, message_id: &str) -> Result<()> {
        let cid = ChatId(chat_id.parse::<i64>()
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?);
        let mid = MessageId(message_id.parse::<i32>()
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?);
        self.bot.stop_message_live_location(cid, mid).await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;
        Ok(())
    }

    async fn health_check(&self) -> bool {
        self.bot.get_me().await.is_ok()
    }
}

#[async_trait]
impl ChannelInfo for TelegramChannel {
    async fn list_chats(&self) -> Result<Vec<ChatInfo>> {
        Err(ChannelError::NotSupported("Telegram bots cannot list chats".into()))
    }

    async fn list_topics(&self, _chat_id: &str) -> Result<Vec<TopicInfo>> {
        Err(ChannelError::NotSupported("list_topics".into()))
    }

    async fn get_chat(&self, chat_id: &str) -> Result<ChatInfo> {
        let cid = ChatId(chat_id.parse::<i64>()
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?);
        let chat = self.bot.get_chat(cid).await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;
        let member_count = self.bot.get_chat_member_count(cid).await
            .ok().map(|c| c as u64);
        Ok(ChatInfo {
            id: chat_id.into(),
            title: chat.title().map(|t| t.to_string()),
            chat_type: if chat.is_private() { ChatType::Direct } else { ChatType::Group },
            member_count,
        })
    }

    async fn get_member(&self, chat_id: &str, user_id: &str) -> Result<MemberInfo> {
        let cid = ChatId(chat_id.parse::<i64>()
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?);
        let uid = UserId(user_id.parse::<u64>()
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?);
        let member = self.bot.get_chat_member(cid, uid).await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;
        let is_admin = matches!(
            member.kind,
            teloxide::types::ChatMemberKind::Administrator { .. }
                | teloxide::types::ChatMemberKind::Owner { .. }
        );
        Ok(MemberInfo {
            user_id: user_id.into(),
            username: member.user.username.clone(),
            display_name: Some(member.user.first_name.clone()),
            is_admin,
        })
    }
}
```

- [ ] **Step 2: Run `cargo check -p nocelium-channels --features telegram`**

Expected: compiles. Fix any teloxide API mismatches — types can be tricky across versions. Common issues: `thread_id` field type, `ReactionEmoji::from` signature, `live_period` type. Check compiler errors and adjust.

- [ ] **Step 3: Commit**

```bash
git add crates/nocelium-channels/src/telegram.rs
git commit -m "feat(channels): implement TelegramChannel with teloxide"
```

---

### Task 7: Update Config

**Files:**
- Modify: `crates/nocelium-core/src/config.rs`
- Modify: `config/nocelium.toml`

- [ ] **Step 1: Add allowed_senders to TelegramConfig**

In `config.rs`, update:
```rust
#[derive(Debug, Deserialize, Clone)]
pub struct TelegramConfig {
    #[serde(default)]
    pub enabled: bool,
    pub token: Option<String>,
    #[serde(default)]
    pub allowed_senders: Vec<String>,
}
```

- [ ] **Step 2: Update nocelium.toml**

```toml
[channels.telegram]
enabled = false
# token = ""  # Set via TELEGRAM_BOT_TOKEN env var
# allowed_senders = []  # User IDs or usernames. Empty = allow all.
```

- [ ] **Step 3: Run `just check`**

- [ ] **Step 4: Commit**

```bash
git add crates/nocelium-core/src/config.rs config/nocelium.toml
git commit -m "feat(config): add allowed_senders to telegram config"
```

---

### Task 8: Simplified Dispatcher

**Files:**
- Create: `crates/nocelium-core/src/dispatch.rs`
- Modify: `crates/nocelium-core/src/lib.rs` (add module)

- [ ] **Step 1: Create dispatch.rs**

Simplified dispatcher — no Nomen yet. Matches dispatch keys against glob patterns, defaults to AgentTurn for everything. Full Nomen-based rules/PromptBuilder is future work.

```rust
use nocelium_channels::EventEnvelope;

/// What to do with a matched event.
#[derive(Debug, Clone)]
pub enum DispatchAction {
    /// Full LLM agent turn.
    AgentTurn,
    /// Direct handler, no LLM.
    Handler(String),
    /// Ignore the event.
    Drop,
}

/// A dispatch rule: glob pattern → action.
#[derive(Debug, Clone)]
pub struct DispatchRule {
    pub pattern: String,
    pub action: DispatchAction,
}

/// Routes events by matching dispatch keys against rules.
pub struct Dispatcher {
    rules: Vec<DispatchRule>,
}

impl Dispatcher {
    pub fn new(rules: Vec<DispatchRule>) -> Self {
        Self { rules }
    }

    /// Default dispatcher: everything goes to AgentTurn.
    pub fn default_agent_turn() -> Self {
        Self { rules: vec![] }
    }

    /// Match an event's dispatch key against rules. First match wins.
    /// Returns AgentTurn if no rule matches.
    pub fn match_rule(&self, event: &EventEnvelope) -> DispatchAction {
        for rule in &self.rules {
            if glob_match(&rule.pattern, &event.dispatch_key) {
                return rule.action.clone();
            }
        }
        DispatchAction::AgentTurn
    }
}

/// Simple glob matching: `*` matches any segment, `**` isn't needed for
/// colon-separated keys. Supports trailing `*` and exact matches.
fn glob_match(pattern: &str, key: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    let pattern_parts: Vec<&str> = pattern.split(':').collect();
    let key_parts: Vec<&str> = key.split(':').collect();

    if pattern_parts.len() > key_parts.len() {
        return false;
    }

    for (i, pp) in pattern_parts.iter().enumerate() {
        if *pp == "*" {
            // If last pattern part is *, match rest
            if i == pattern_parts.len() - 1 {
                return true;
            }
            // Otherwise match any single segment, continue
            continue;
        }
        if i >= key_parts.len() || *pp != key_parts[i] {
            return false;
        }
    }

    // Pattern consumed — only match if key has same length (no trailing segments)
    // unless pattern ended with *
    pattern_parts.len() == key_parts.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_exact() {
        assert!(glob_match("telegram:message:-1001234", "telegram:message:-1001234"));
        assert!(!glob_match("telegram:message:-1001234", "telegram:message:-9999"));
    }

    #[test]
    fn test_glob_trailing_wildcard() {
        assert!(glob_match("telegram:message:*", "telegram:message:-1001234"));
        assert!(glob_match("telegram:message:*", "telegram:message:-1001234:42"));
        assert!(glob_match("telegram:*", "telegram:message:-1001234"));
        assert!(!glob_match("telegram:message:*", "nostr:message:foo"));
    }

    #[test]
    fn test_glob_middle_wildcard() {
        assert!(glob_match("telegram:*:-1001234", "telegram:message:-1001234"));
        assert!(!glob_match("telegram:*:-1001234", "telegram:message:-9999"));
    }

    #[test]
    fn test_glob_star_all() {
        assert!(glob_match("*", "anything:at:all"));
    }

    #[test]
    fn test_glob_no_partial_match() {
        assert!(!glob_match("telegram:message", "telegram:message:-1001234"));
    }
}
```

- [ ] **Step 2: Add `pub mod dispatch;` to `crates/nocelium-core/src/lib.rs`**

- [ ] **Step 3: Run `just check`**

- [ ] **Step 4: Commit**

```bash
git add crates/nocelium-core/src/dispatch.rs crates/nocelium-core/src/lib.rs
git commit -m "feat(core): add simplified event dispatcher with glob matching"
```

---

### Task 9: Rewrite Agent Loop

**Files:**
- Rewrite: `crates/nocelium-core/src/agent.rs`

- [ ] **Step 1: Rewrite agent.rs for dispatch-based architecture**

The agent loop now:
1. Takes `Vec<Arc<dyn Channel>>` and a `Dispatcher`
2. Creates shared mpsc queue, starts all channels listening
3. Receives `EventEnvelope` from queue
4. Matches dispatch rules
5. For `AgentTurn`: sends to LLM, routes response back via channel name
6. For `Drop`: ignores
7. `Handler`: logged as unimplemented (future Nomen work)
8. Streams via send+edit for channels with edit capability

```rust
use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use rig::agent::Agent;
use rig::client::CompletionClient;
use rig::completion::Prompt;
use rig::providers::openai;
use rig::streaming::StreamingPrompt;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::dispatch::{DispatchAction, Dispatcher};
use crate::identity::Identity;
use nocelium_channels::{Channel, EventEnvelope, OutboundMessage};
use nocelium_tools::{ReadFileTool, ShellTool, WriteFileTool};

pub fn build_agent(
    config: &Config,
    identity: &Identity,
) -> Result<Agent<openai::completion::CompletionModel>> {
    let api_key = config
        .provider
        .api_key
        .clone()
        .or_else(|| std::env::var("OPENROUTER_API_KEY").ok())
        .ok_or_else(|| {
            anyhow::anyhow!("No API key. Set OPENROUTER_API_KEY or provider.api_key in config")
        })?;

    let base_url = config
        .provider
        .base_url
        .as_deref()
        .unwrap_or("https://openrouter.ai/api/v1");

    let client = openai::CompletionsClient::builder()
        .api_key(&api_key)
        .base_url(base_url)
        .build()?;

    let preamble = format!(
        "{}\n\nYour Nostr identity (npub): {}",
        config.agent.preamble,
        identity.npub()
    );

    let agent = client
        .agent(&config.provider.model)
        .preamble(&preamble)
        .max_tokens(config.agent.max_tokens)
        .tool(ShellTool)
        .tool(ReadFileTool)
        .tool(WriteFileTool)
        .build();

    Ok(agent)
}

/// Dispatch-based agent loop.
///
/// All channels and event sources push to the same mpsc queue. The dispatcher
/// routes events by dispatch key to handlers or LLM agent turns.
pub async fn run_loop(
    agent: &Agent<openai::completion::CompletionModel>,
    channels: Vec<Arc<dyn Channel>>,
    dispatcher: &Dispatcher,
    streaming: bool,
) -> Result<()> {
    let (tx, mut rx) = mpsc::channel::<EventEnvelope>(256);

    // Build channel lookup by name
    let channel_map: HashMap<String, Arc<dyn Channel>> = channels
        .iter()
        .map(|ch| (ch.name().to_string(), ch.clone()))
        .collect();

    // Start all channels listening on the shared queue
    for ch in &channels {
        ch.listen(tx.clone())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to start channel {}: {}", ch.name(), e))?;
        tracing::info!(channel = ch.name(), "Channel started");
    }

    drop(tx);

    tracing::info!(streaming = streaming, "Agent loop started (dispatch mode)");

    while let Some(event) = rx.recv().await {
        // Check for exit commands
        if let Some(ref text) = event.text {
            if text.trim() == "/quit" || text.trim() == "/exit" {
                tracing::info!("Exit command received");
                if let Some(ch_name) = &event.channel {
                    if let Some(ch) = channel_map.get(ch_name) {
                        if let Some(ref chat_id) = event.chat_id {
                            let _ = ch.send(&OutboundMessage {
                                chat_id: chat_id.clone(),
                                text: "Goodbye!".into(),
                                reply_to_id: None,
                                thread_id: event.thread_id.clone(),
                                attachments: vec![],
                                buttons: None,
                                silent: false,
                            }).await;
                        }
                    }
                }
                break;
            }
        }

        tracing::debug!(
            dispatch_key = %event.dispatch_key,
            kind = %event.kind,
            "Event received"
        );

        // Match dispatch rule
        let action = dispatcher.match_rule(&event);

        match action {
            DispatchAction::Drop => {
                tracing::trace!(key = %event.dispatch_key, "Dropped event");
                continue;
            }
            DispatchAction::Handler(name) => {
                tracing::warn!(
                    handler = %name,
                    key = %event.dispatch_key,
                    "Handler dispatch not yet implemented"
                );
                continue;
            }
            DispatchAction::AgentTurn => {
                handle_agent_turn(agent, &event, &channel_map, streaming).await;
            }
        }
    }

    Ok(())
}

async fn handle_agent_turn(
    agent: &Agent<openai::completion::CompletionModel>,
    event: &EventEnvelope,
    channel_map: &HashMap<String, Arc<dyn Channel>>,
    streaming: bool,
) {
    let message_text = match &event.text {
        Some(t) if !t.trim().is_empty() => t.clone(),
        _ => return, // no text to process
    };

    let ch = match event.channel.as_ref().and_then(|name| channel_map.get(name)) {
        Some(ch) => ch.clone(),
        None => return,
    };

    let chat_id = match &event.chat_id {
        Some(id) => id.clone(),
        None => return,
    };

    // Typing indicator
    if ch.capabilities().typing {
        let _ = ch.start_typing(&chat_id).await;
    }

    if streaming && ch.capabilities().edit {
        stream_via_edit(agent, &message_text, &ch, &chat_id, event).await;
    } else {
        send_buffered(agent, &message_text, &ch, &chat_id, event).await;
    }
}

async fn stream_via_edit(
    agent: &Agent<openai::completion::CompletionModel>,
    message_text: &str,
    ch: &Arc<dyn Channel>,
    chat_id: &str,
    event: &EventEnvelope,
) {
    use futures::StreamExt;
    use rig::agent::{MultiTurnStreamItem, Text};
    use rig::streaming::StreamedAssistantContent;

    let placeholder = OutboundMessage {
        chat_id: chat_id.into(),
        text: "...".into(),
        reply_to_id: event.message_id.clone(),
        thread_id: event.thread_id.clone(),
        attachments: vec![],
        buttons: None,
        silent: false,
    };

    let result = match ch.send(&placeholder).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "Failed to send placeholder");
            return;
        }
    };

    let mut buffer = String::new();
    let mut stream = agent.stream_prompt(message_text).await;
    let mut last_edit = tokio::time::Instant::now();
    let mut last_edit_len: usize = 0;

    while let Some(item) = stream.next().await {
        match item {
            Ok(MultiTurnStreamItem::StreamAssistantItem(
                StreamedAssistantContent::Text(Text { text }),
            )) => {
                buffer.push_str(&text);
                let elapsed = last_edit.elapsed() >= tokio::time::Duration::from_millis(500);
                let chars_added = buffer.len() - last_edit_len >= 50;
                if elapsed || chars_added {
                    let _ = ch.edit(chat_id, &result.message_id, &buffer).await;
                    last_edit = tokio::time::Instant::now();
                    last_edit_len = buffer.len();
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "Streaming error");
                buffer = format!("Error: {e}");
                break;
            }
            _ => {}
        }
    }

    if !buffer.is_empty() {
        let _ = ch.edit(chat_id, &result.message_id, &buffer).await;
    }
}

async fn send_buffered(
    agent: &Agent<openai::completion::CompletionModel>,
    message_text: &str,
    ch: &Arc<dyn Channel>,
    chat_id: &str,
    event: &EventEnvelope,
) {
    let (text, is_error) = match agent.prompt(message_text).await {
        Ok(response) => (response, false),
        Err(e) => {
            tracing::error!(error = %e, "Agent prompt failed");
            (format!("Error: {e}"), true)
        }
    };

    let out = OutboundMessage {
        chat_id: chat_id.into(),
        text,
        reply_to_id: if is_error { None } else { event.message_id.clone() },
        thread_id: event.thread_id.clone(),
        attachments: vec![],
        buttons: None,
        silent: false,
    };

    if let Err(e) = ch.send(&out).await {
        tracing::error!(error = %e, "Failed to send response");
    }
}
```

- [ ] **Step 2: Run `just check`**

Expected: compiles. `main.rs` will fail (signature change).

- [ ] **Step 3: Commit**

```bash
git add crates/nocelium-core/src/agent.rs
git commit -m "feat(core): dispatch-based agent loop with EventEnvelope routing"
```

---

### Task 10: Wire Up in main.rs

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Update main.rs**

```rust
use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

use nocelium_channels::Channel;
use nocelium_channels::stdio::StdioChannel;
#[cfg(feature = "telegram")]
use nocelium_channels::telegram::TelegramChannel;
use nocelium_core::dispatch::Dispatcher;
use nocelium_core::{Config, Identity};

#[derive(Parser)]
#[command(name = "nocelium", about = "Nostr-native AI agent runtime")]
struct Cli {
    #[arg(short, long)]
    config: Option<PathBuf>,

    #[arg(long)]
    gen_identity: bool,

    #[arg(long)]
    show_identity: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("nocelium=info".parse()?),
        )
        .init();

    let cli = Cli::parse();

    let config = match &cli.config {
        Some(path) => Config::load(path)?,
        None => Config::load_default()?,
    };

    let identity = Identity::load_or_generate(&config.identity)?;

    if cli.gen_identity {
        println!("Identity generated: {}", identity.npub());
        return Ok(());
    }

    if cli.show_identity {
        println!("npub: {}", identity.npub());
        return Ok(());
    }

    println!("Nocelium v{}", env!("CARGO_PKG_VERSION"));
    println!("Identity: {}", identity.npub());
    println!("Provider: {} ({})", config.provider.provider_type, config.provider.model);
    println!("Streaming: {}", config.agent.streaming);

    let agent = nocelium_core::agent::build_agent(&config, &identity)?;

    // Build channel list
    let mut channels: Vec<Arc<dyn Channel>> = Vec::new();

    if config.channels.stdio {
        channels.push(Arc::new(StdioChannel::new()));
        println!("Channel: stdio");
    }

    #[cfg(feature = "telegram")]
    if let Some(ref tg_config) = config.channels.telegram {
        if tg_config.enabled {
            let token = tg_config
                .token
                .clone()
                .or_else(|| std::env::var("TELEGRAM_BOT_TOKEN").ok())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Telegram enabled but no token. Set TELEGRAM_BOT_TOKEN or channels.telegram.token"
                    )
                })?;
            let tg = TelegramChannel::new(&token, tg_config.allowed_senders.clone());
            channels.push(Arc::new(tg));
            println!("Channel: telegram");
        }
    }

    if channels.is_empty() {
        anyhow::bail!("No channels enabled. Enable at least one in config.");
    }

    // Simplified dispatcher — all events go to AgentTurn (no Nomen rules yet)
    let dispatcher = Dispatcher::default_agent_turn();

    println!("Type /quit to exit\n");

    nocelium_core::agent::run_loop(&agent, channels, &dispatcher, config.agent.streaming).await?;

    Ok(())
}
```

- [ ] **Step 2: Run `just ci`**

Full validation: check + clippy + tests.

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire up dispatch-based multi-channel startup"
```

---

### Task 11: Final Validation

- [ ] **Step 1: Run `just ci`**

- [ ] **Step 2: Verify default config still works**

```bash
cargo run -- --show-identity
```

- [ ] **Step 3: Run dispatch tests**

```bash
cargo test -p nocelium-core -- dispatch
```
