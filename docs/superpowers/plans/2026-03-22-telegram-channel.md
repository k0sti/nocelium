# Telegram Channel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement TelegramChannel as a push-based channel with full capabilities (send, edit, delete, reactions, typing, pins, polls, buttons, location) following the spec in `docs/channels.md`.

**Architecture:** Evolve the Channel trait from pull-based (`receive()`) to push-based (`listen(tx)` with mpsc queue). TelegramChannel uses teloxide 0.17 with long polling via `Dispatcher`. Agent loop changes to `select!` on inbound message queue. StdioChannel updated for the new trait. Telegram behind a cargo feature flag.

**Tech Stack:** teloxide 0.17, tokio mpsc, thiserror, async-trait

---

## File Structure

| Action | File | Responsibility |
|--------|------|---------------|
| Create | `crates/nocelium-channels/src/error.rs` | Channel error types via thiserror |
| Create | `crates/nocelium-channels/src/types.rs` | InboundMessage, OutboundMessage, ChannelCapabilities, SendResult, ChatType, Attachment, Button, Poll, Location, ChatInfo, TopicInfo, MemberInfo |
| Rewrite | `crates/nocelium-channels/src/lib.rs` | Channel + ChannelInfo traits, re-exports |
| Rewrite | `crates/nocelium-channels/src/stdio.rs` | StdioChannel updated for new trait |
| Create | `crates/nocelium-channels/src/telegram.rs` | TelegramChannel with teloxide |
| Modify | `crates/nocelium-channels/Cargo.toml` | Add teloxide, thiserror deps behind feature flag |
| Modify | `Cargo.toml` (workspace root) | Add teloxide workspace dep, enable telegram feature |
| Modify | `crates/nocelium-core/src/config.rs` | Add `allowed_senders` to TelegramConfig |
| Rewrite | `crates/nocelium-core/src/agent.rs` | Push-based agent loop with mpsc + channel routing |
| Modify | `src/main.rs` | Start telegram channel if configured |

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

Run: `just check`
Expected: compiles (no new code yet, just deps)

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/nocelium-channels/Cargo.toml
git commit -m "feat(channels): add teloxide dependency behind telegram feature flag"
```

---

### Task 2: Error Types

**Files:**
- Create: `crates/nocelium-channels/src/error.rs`
- Modify: `crates/nocelium-channels/src/lib.rs` (add `pub mod error;`)

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

- [ ] **Step 2: Add `pub mod error;` to lib.rs temporarily**

Just add `pub mod error;` at the top of `lib.rs`. We'll rewrite `lib.rs` fully in Task 3.

- [ ] **Step 3: Run `just check`**

Expected: compiles

- [ ] **Step 4: Commit**

```bash
git add crates/nocelium-channels/src/error.rs crates/nocelium-channels/src/lib.rs
git commit -m "feat(channels): add channel error types"
```

---

### Task 3: Message Types

**Files:**
- Create: `crates/nocelium-channels/src/types.rs`

- [ ] **Step 1: Create types.rs with all message/capability types**

```rust
use serde::{Deserialize, Serialize};

/// Direction of a message into the agent loop
#[derive(Debug, Clone)]
pub struct InboundMessage {
    // Identity
    pub id: String,
    pub channel: String,
    pub chat_id: String,
    pub sender_id: String,
    pub sender_name: Option<String>,
    pub sender_handle: Option<String>,

    // Content
    pub text: String,
    pub raw_text: Option<String>,

    // Context
    pub chat_type: ChatType,
    pub group_subject: Option<String>,
    pub thread_id: Option<String>,
    pub timestamp: u64,

    // Reply context
    pub reply_to_id: Option<String>,
    pub reply_to_text: Option<String>,
    pub reply_to_sender: Option<String>,

    // Mentions
    pub mentions: Vec<String>,
    pub was_mentioned: bool,

    // Media
    pub attachments: Vec<Attachment>,

    // Location
    pub location: Option<Location>,

    // Callback (button press)
    pub callback_data: Option<String>,
    pub callback_query_id: Option<String>,
}

/// Direction of a message out of the agent loop
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

/// Result from sending a message
#[derive(Debug, Clone)]
pub struct SendResult {
    pub message_id: String,
    pub chat_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatType {
    Direct,
    Group,
    Thread,
}

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

Expected: compiles (warns about unused, that's fine)

- [ ] **Step 4: Commit**

```bash
git add crates/nocelium-channels/src/types.rs crates/nocelium-channels/src/lib.rs
git commit -m "feat(channels): add channel message and capability types"
```

---

### Task 4: Rewrite Channel Trait

**Files:**
- Rewrite: `crates/nocelium-channels/src/lib.rs`

- [ ] **Step 1: Rewrite lib.rs with new Channel + ChannelInfo traits**

```rust
//! Nocelium Channels — Message I/O abstraction
//!
//! Channels provide the bidirectional interface between external messaging
//! systems (Telegram, Nostr, stdio) and the agent loop. Push-based: channels
//! listen in background tasks and push to a shared mpsc queue.

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
/// Push-based: `listen()` spawns a background task that pushes inbound messages
/// to the provided mpsc sender. The agent loop selects on the receiver.
///
/// All methods except `name()`, `capabilities()`, `listen()`, `send()`, and
/// `health_check()` have default no-op/error implementations.
#[async_trait]
pub trait Channel: Send + Sync {
    fn name(&self) -> &str;
    fn capabilities(&self) -> ChannelCapabilities;

    // Inbound
    async fn listen(&self, tx: mpsc::Sender<InboundMessage>) -> Result<()>;

    // Outbound
    async fn send(&self, message: &OutboundMessage) -> Result<SendResult>;

    async fn edit(&self, _chat_id: &str, _message_id: &str, _text: &str) -> Result<()> {
        Err(ChannelError::NotSupported("edit".into()))
    }

    async fn delete(&self, _chat_id: &str, _message_id: &str) -> Result<()> {
        Err(ChannelError::NotSupported("delete".into()))
    }

    // Reactions
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

    // Typing
    async fn start_typing(&self, _chat_id: &str) -> Result<()> {
        Err(ChannelError::NotSupported("start_typing".into()))
    }

    async fn stop_typing(&self, _chat_id: &str) -> Result<()> {
        Ok(()) // no-op by default
    }

    // Pins
    async fn pin_message(&self, _chat_id: &str, _message_id: &str) -> Result<()> {
        Err(ChannelError::NotSupported("pin_message".into()))
    }

    async fn unpin_message(&self, _chat_id: &str, _message_id: &str) -> Result<()> {
        Err(ChannelError::NotSupported("unpin_message".into()))
    }

    // Polls
    async fn send_poll(&self, _chat_id: &str, _poll: &Poll) -> Result<SendResult> {
        Err(ChannelError::NotSupported("send_poll".into()))
    }

    // Location
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

    // Health
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

- [ ] **Step 2: Run `just check`**

Expected: fails — StdioChannel doesn't implement new trait yet. That's expected, we fix it next task.

- [ ] **Step 3: Commit**

```bash
git add crates/nocelium-channels/src/lib.rs
git commit -m "feat(channels): rewrite Channel trait to push-based architecture"
```

---

### Task 5: Update StdioChannel

**Files:**
- Rewrite: `crates/nocelium-channels/src/stdio.rs`

- [ ] **Step 1: Rewrite stdio.rs for new Channel trait**

StdioChannel spawns a tokio task in `listen()` that reads stdin lines and pushes `InboundMessage` to the mpsc sender. `send()` writes to stdout. Capabilities: `{ chat_types: [Direct] }`, everything else false.

```rust
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, Mutex};

use crate::{
    Channel, ChannelCapabilities, ChannelError, ChatType, InboundMessage,
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

    async fn listen(&self, tx: mpsc::Sender<InboundMessage>) -> Result<()> {
        let writer = self.writer.clone();

        tokio::spawn(async move {
            let mut reader = BufReader::new(tokio::io::stdin());
            let mut counter: u64 = 0;

            loop {
                // Print prompt
                {
                    let mut w = writer.lock().await;
                    let _ = w.write_all(b"\n> ").await;
                    let _ = w.flush().await;
                }

                let mut line = String::new();
                match reader.read_line(&mut line).await {
                    Ok(0) => break, // EOF
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

                        let msg = InboundMessage {
                            id: counter.to_string(),
                            channel: "stdio".into(),
                            chat_id: "stdio".into(),
                            sender_id: "local".into(),
                            sender_name: None,
                            sender_handle: None,
                            text,
                            raw_text: None,
                            chat_type: ChatType::Direct,
                            group_subject: None,
                            thread_id: None,
                            timestamp: now,
                            reply_to_id: None,
                            reply_to_text: None,
                            reply_to_sender: None,
                            mentions: vec![],
                            was_mentioned: false,
                            attachments: vec![],
                            location: None,
                            callback_data: None,
                            callback_query_id: None,
                        };

                        if tx.send(msg).await.is_err() {
                            break; // receiver dropped
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
        w.write_all(b"\n").await.map_err(|e| ChannelError::SendFailed(e.to_string()))?;
        w.write_all(message.text.as_bytes()).await.map_err(|e| ChannelError::SendFailed(e.to_string()))?;
        w.write_all(b"\n").await.map_err(|e| ChannelError::SendFailed(e.to_string()))?;
        w.flush().await.map_err(|e| ChannelError::SendFailed(e.to_string()))?;

        Ok(SendResult {
            message_id: "0".into(),
            chat_id: message.chat_id.clone(),
        })
    }
}
```

- [ ] **Step 2: Run `just check`**

Expected: channels crate compiles. Core crate will fail (agent.rs uses old trait). That's expected.

- [ ] **Step 3: Commit**

```bash
git add crates/nocelium-channels/src/stdio.rs
git commit -m "feat(channels): update StdioChannel for push-based Channel trait"
```

---

### Task 6: Implement TelegramChannel

**Files:**
- Create: `crates/nocelium-channels/src/telegram.rs`

- [ ] **Step 1: Create telegram.rs**

This is the core implementation. TelegramChannel wraps a `teloxide::Bot`, starts long polling in `listen()`, and implements full capabilities.

```rust
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use teloxide::prelude::*;
use teloxide::types::{
    ChatAction, InlineKeyboardButton, InlineKeyboardMarkup, InputPollOption,
    MessageId, ParseMode, ReactionType, ReactionEmoji,
};
use tokio::sync::mpsc;
use tracing;

use crate::{
    Channel, ChannelCapabilities, ChannelError, ChannelInfo, ChatInfo, ChatType,
    InboundMessage, Location, MemberInfo, OutboundMessage, Poll as ChannelPoll,
    Result, SendResult, TopicInfo,
};

pub struct TelegramChannel {
    bot: Bot,
    allowed_senders: Vec<String>,
}

impl TelegramChannel {
    /// Create from explicit token.
    pub fn new(token: &str, allowed_senders: Vec<String>) -> Self {
        Self {
            bot: Bot::new(token),
            allowed_senders,
        }
    }

    /// Create from TELEGRAM_BOT_TOKEN env var.
    pub fn from_env(allowed_senders: Vec<String>) -> Self {
        Self {
            bot: Bot::from_env(),
            allowed_senders,
        }
    }

    fn is_sender_allowed(&self, user: &teloxide::types::User) -> bool {
        if self.allowed_senders.is_empty() {
            return true; // no allowlist = allow all
        }
        let uid = user.id.0.to_string();
        if self.allowed_senders.contains(&uid) {
            return true;
        }
        if let Some(ref username) = user.username {
            if self.allowed_senders.contains(username) {
                return true;
            }
        }
        false
    }

    fn bot_clone(&self) -> Bot {
        self.bot.clone()
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

fn msg_to_inbound(msg: &Message) -> Option<InboundMessage> {
    let text = msg.text().or_else(|| msg.caption()).unwrap_or("").to_string();
    let from = msg.from.as_ref()?;

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

    let reply_to_id = msg.reply_to_message().map(|r| r.id.0.to_string());
    let reply_to_text = msg
        .reply_to_message()
        .and_then(|r| r.text().map(|t| t.to_string()));
    let reply_to_sender = msg
        .reply_to_message()
        .and_then(|r| r.from.as_ref().map(|u| u.id.0.to_string()));

    let thread_id = msg.thread_id.map(|t| t.0.to_string());

    Some(InboundMessage {
        id: msg.id.0.to_string(),
        channel: "telegram".into(),
        chat_id: msg.chat.id.0.to_string(),
        sender_id: from.id.0.to_string(),
        sender_name,
        sender_handle: from.username.clone(),
        text,
        raw_text: msg.text().map(|t| t.to_string()),
        chat_type: tg_chat_type(&msg.chat),
        group_subject: msg.chat.title().map(|t| t.to_string()),
        thread_id,
        timestamp: now_secs(),
        reply_to_id,
        reply_to_text,
        reply_to_sender,
        mentions: vec![],
        was_mentioned: false,
        attachments: vec![],
        location,
        callback_data: None,
        callback_query_id: None,
    })
}

fn callback_to_inbound(q: &CallbackQuery) -> Option<InboundMessage> {
    let from = &q.from;
    let msg = q.message.as_ref()?;
    let (chat_id, message_id) = match msg {
        teloxide::types::MaybeInaccessibleMessage::Regular(m) => {
            (m.chat.id.0.to_string(), m.id.0.to_string())
        }
        teloxide::types::MaybeInaccessibleMessage::Inaccessible(m) => {
            (m.chat.id.0.to_string(), m.id.0.to_string())
        }
    };

    let sender_name = {
        let first = &from.first_name;
        match &from.last_name {
            Some(last) => Some(format!("{first} {last}")),
            None => Some(first.clone()),
        }
    };

    Some(InboundMessage {
        id: message_id,
        channel: "telegram".into(),
        chat_id,
        sender_id: from.id.0.to_string(),
        sender_name,
        sender_handle: from.username.clone(),
        text: String::new(),
        raw_text: None,
        chat_type: ChatType::Direct,
        group_subject: None,
        thread_id: None,
        timestamp: now_secs(),
        reply_to_id: None,
        reply_to_text: None,
        reply_to_sender: None,
        mentions: vec![],
        was_mentioned: false,
        attachments: vec![],
        location: None,
        callback_data: q.data.clone(),
        callback_query_id: Some(q.id.clone()),
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

    async fn listen(&self, tx: mpsc::Sender<InboundMessage>) -> Result<()> {
        let bot = self.bot_clone();
        let allowed = self.allowed_senders.clone();
        let tx_msg = tx.clone();
        let tx_cb = tx;

        let allowed_cb = allowed.clone();

        let handler = dptree::entry()
            .branch(
                Update::filter_message().endpoint(
                    move |msg: Message| {
                        let tx = tx_msg.clone();
                        let allowed = allowed.clone();
                        async move {
                            if let Some(from) = msg.from.as_ref() {
                                // Check allowlist
                                if !allowed.is_empty() {
                                    let uid = from.id.0.to_string();
                                    let allowed_by_id = allowed.contains(&uid);
                                    let allowed_by_name = from
                                        .username
                                        .as_ref()
                                        .map(|u| allowed.contains(u))
                                        .unwrap_or(false);
                                    if !allowed_by_id && !allowed_by_name {
                                        tracing::debug!(
                                            sender = %from.id,
                                            "Ignoring message from non-allowed sender"
                                        );
                                        return respond(());
                                    }
                                }
                            }

                            if let Some(inbound) = msg_to_inbound(&msg) {
                                let _ = tx.send(inbound).await;
                            }
                            respond(())
                        }
                    },
                ),
            )
            .branch(
                Update::filter_callback_query().endpoint(
                    move |bot: Bot, q: CallbackQuery| {
                        let tx = tx_cb.clone();
                        let allowed = allowed_cb.clone();
                        async move {
                            // Check allowlist
                            if !allowed.is_empty() {
                                let uid = q.from.id.0.to_string();
                                let allowed_by_id = allowed.contains(&uid);
                                let allowed_by_name = q
                                    .from
                                    .username
                                    .as_ref()
                                    .map(|u| allowed.contains(u))
                                    .unwrap_or(false);
                                if !allowed_by_id && !allowed_by_name {
                                    return respond(());
                                }
                            }

                            // Auto-answer callback query
                            let _ = bot.answer_callback_query(&q.id).await;

                            if let Some(inbound) = callback_to_inbound(&q) {
                                let _ = tx.send(inbound).await;
                            }
                            respond(())
                        }
                    },
                ),
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
            message
                .chat_id
                .parse::<i64>()
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

        let sent = req
            .await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;

        Ok(SendResult {
            message_id: sent.id.0.to_string(),
            chat_id: message.chat_id.clone(),
        })
    }

    async fn edit(&self, chat_id: &str, message_id: &str, text: &str) -> Result<()> {
        let cid = ChatId(
            chat_id.parse::<i64>().map_err(|e| ChannelError::SendFailed(e.to_string()))?,
        );
        let mid = MessageId(
            message_id.parse::<i32>().map_err(|e| ChannelError::SendFailed(e.to_string()))?,
        );

        self.bot
            .edit_message_text(cid, mid, text)
            .parse_mode(ParseMode::MarkdownV2)
            .await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;

        Ok(())
    }

    async fn delete(&self, chat_id: &str, message_id: &str) -> Result<()> {
        let cid = ChatId(
            chat_id.parse::<i64>().map_err(|e| ChannelError::SendFailed(e.to_string()))?,
        );
        let mid = MessageId(
            message_id.parse::<i32>().map_err(|e| ChannelError::SendFailed(e.to_string()))?,
        );

        self.bot
            .delete_message(cid, mid)
            .await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;

        Ok(())
    }

    async fn add_reaction(
        &self, chat_id: &str, message_id: &str, emoji: &str,
    ) -> Result<()> {
        let cid = ChatId(
            chat_id.parse::<i64>().map_err(|e| ChannelError::SendFailed(e.to_string()))?,
        );
        let mid = MessageId(
            message_id.parse::<i32>().map_err(|e| ChannelError::SendFailed(e.to_string()))?,
        );

        let reaction = ReactionType::Emoji {
            emoji: ReactionEmoji::from(emoji.to_string()),
        };

        self.bot
            .set_message_reaction(cid, mid)
            .reaction(vec![reaction])
            .await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;

        Ok(())
    }

    async fn start_typing(&self, chat_id: &str) -> Result<()> {
        let cid = ChatId(
            chat_id.parse::<i64>().map_err(|e| ChannelError::SendFailed(e.to_string()))?,
        );

        self.bot
            .send_chat_action(cid, ChatAction::Typing)
            .await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;

        Ok(())
    }

    async fn stop_typing(&self, _chat_id: &str) -> Result<()> {
        Ok(()) // Telegram typing auto-expires
    }

    async fn pin_message(&self, chat_id: &str, message_id: &str) -> Result<()> {
        let cid = ChatId(
            chat_id.parse::<i64>().map_err(|e| ChannelError::SendFailed(e.to_string()))?,
        );
        let mid = MessageId(
            message_id.parse::<i32>().map_err(|e| ChannelError::SendFailed(e.to_string()))?,
        );

        self.bot
            .pin_chat_message(cid, mid)
            .await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;

        Ok(())
    }

    async fn unpin_message(&self, chat_id: &str, message_id: &str) -> Result<()> {
        let cid = ChatId(
            chat_id.parse::<i64>().map_err(|e| ChannelError::SendFailed(e.to_string()))?,
        );
        let mid = MessageId(
            message_id.parse::<i32>().map_err(|e| ChannelError::SendFailed(e.to_string()))?,
        );

        self.bot
            .unpin_chat_message(cid)
            .message_id(mid)
            .await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;

        Ok(())
    }

    async fn send_poll(&self, chat_id: &str, poll: &ChannelPoll) -> Result<SendResult> {
        let cid = ChatId(
            chat_id.parse::<i64>().map_err(|e| ChannelError::SendFailed(e.to_string()))?,
        );

        let options: Vec<InputPollOption> =
            poll.options.iter().map(|o| InputPollOption::from(o.as_str())).collect();

        let sent = self
            .bot
            .send_poll(cid, &poll.question, options)
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
        let cid = ChatId(
            chat_id.parse::<i64>().map_err(|e| ChannelError::SendFailed(e.to_string()))?,
        );

        let mut req = self.bot.send_location(cid, lat, lon);

        if let Some(period) = live_period {
            req = req.live_period(period);
        }

        if let Some(reply_id) = reply_to {
            if let Ok(id) = reply_id.parse::<i32>() {
                req = req.reply_to_message_id(MessageId(id));
            }
        }

        let sent = req
            .await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;

        Ok(SendResult {
            message_id: sent.id.0.to_string(),
            chat_id: chat_id.into(),
        })
    }

    async fn edit_location(
        &self, chat_id: &str, message_id: &str, lat: f64, lon: f64,
    ) -> Result<()> {
        let cid = ChatId(
            chat_id.parse::<i64>().map_err(|e| ChannelError::SendFailed(e.to_string()))?,
        );
        let mid = MessageId(
            message_id.parse::<i32>().map_err(|e| ChannelError::SendFailed(e.to_string()))?,
        );

        self.bot
            .edit_message_live_location(cid, mid, lat, lon)
            .await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;

        Ok(())
    }

    async fn stop_location(&self, chat_id: &str, message_id: &str) -> Result<()> {
        let cid = ChatId(
            chat_id.parse::<i64>().map_err(|e| ChannelError::SendFailed(e.to_string()))?,
        );
        let mid = MessageId(
            message_id.parse::<i32>().map_err(|e| ChannelError::SendFailed(e.to_string()))?,
        );

        self.bot
            .stop_message_live_location(cid, mid)
            .await
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
        // Telegram Bot API doesn't support listing all chats
        Err(ChannelError::NotSupported(
            "Telegram bots cannot list chats".into(),
        ))
    }

    async fn list_topics(&self, chat_id: &str) -> Result<Vec<TopicInfo>> {
        // Would need getForumTopicList which isn't in Bot API yet
        Err(ChannelError::NotSupported("list_topics".into()))
    }

    async fn get_chat(&self, chat_id: &str) -> Result<ChatInfo> {
        let cid = ChatId(
            chat_id.parse::<i64>().map_err(|e| ChannelError::SendFailed(e.to_string()))?,
        );

        let chat = self
            .bot
            .get_chat(cid)
            .await
            .map_err(|e| ChannelError::SendFailed(e.to_string()))?;

        let member_count = self
            .bot
            .get_chat_member_count(cid)
            .await
            .ok()
            .map(|c| c as u64);

        Ok(ChatInfo {
            id: chat_id.into(),
            title: chat.title().map(|t| t.to_string()),
            chat_type: if chat.is_private() {
                ChatType::Direct
            } else {
                ChatType::Group
            },
            member_count,
        })
    }

    async fn get_member(&self, chat_id: &str, user_id: &str) -> Result<MemberInfo> {
        let cid = ChatId(
            chat_id.parse::<i64>().map_err(|e| ChannelError::SendFailed(e.to_string()))?,
        );
        let uid = UserId(
            user_id
                .parse::<u64>()
                .map_err(|e| ChannelError::SendFailed(e.to_string()))?,
        );

        let member = self
            .bot
            .get_chat_member(cid, uid)
            .await
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

Expected: compiles. Some API mismatches may need fixing — teloxide types can be tricky. Fix any compilation errors.

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

In `config.rs`, update `TelegramConfig`:
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

- [ ] **Step 2: Update nocelium.toml with allowed_senders**

```toml
[channels.telegram]
enabled = false
# token = ""  # Set via TELEGRAM_BOT_TOKEN env var
# allowed_senders = []  # User IDs or usernames. Empty = allow all.
```

- [ ] **Step 3: Run `just check`**

Expected: compiles

- [ ] **Step 4: Commit**

```bash
git add crates/nocelium-core/src/config.rs config/nocelium.toml
git commit -m "feat(config): add allowed_senders to telegram config"
```

---

### Task 8: Update Agent Loop

**Files:**
- Rewrite: `crates/nocelium-core/src/agent.rs`
- Modify: `crates/nocelium-core/Cargo.toml` (if tokio features needed)

- [ ] **Step 1: Rewrite agent.rs for push-based multi-channel architecture**

The agent loop now:
1. Takes `Vec<Arc<dyn Channel>>` instead of `&mut dyn Channel`
2. Creates an mpsc channel, starts all channels listening
3. Selects on the mpsc receiver
4. Routes outbound responses by matching `InboundMessage.channel` to the right `Arc<dyn Channel>`
5. Supports streaming via send + edit for channels with edit capability

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
use crate::identity::Identity;
use nocelium_channels::{Channel, InboundMessage, OutboundMessage};
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

/// The main agent loop: push-based, multi-channel.
pub async fn run_loop(
    agent: &Agent<openai::completion::CompletionModel>,
    channels: Vec<Arc<dyn Channel>>,
    streaming: bool,
) -> Result<()> {
    let (tx, mut rx) = mpsc::channel::<InboundMessage>(256);

    // Build channel lookup by name
    let channel_map: HashMap<String, Arc<dyn Channel>> = channels
        .iter()
        .map(|ch| (ch.name().to_string(), ch.clone()))
        .collect();

    // Start all channels listening
    for ch in &channels {
        ch.listen(tx.clone())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to start channel {}: {}", ch.name(), e))?;
        tracing::info!(channel = ch.name(), "Channel started");
    }

    // Drop our tx so the loop ends when all channel tasks finish
    drop(tx);

    tracing::info!(streaming = streaming, "Agent loop started");

    while let Some(inbound) = rx.recv().await {
        if inbound.text.trim() == "/quit" || inbound.text.trim() == "/exit" {
            tracing::info!("Exit command received");
            if let Some(ch) = channel_map.get(&inbound.channel) {
                let _ = ch
                    .send(&OutboundMessage {
                        chat_id: inbound.chat_id.clone(),
                        text: "Goodbye!".into(),
                        reply_to_id: None,
                        thread_id: inbound.thread_id.clone(),
                        attachments: vec![],
                        buttons: None,
                        silent: false,
                    })
                    .await;
            }
            break;
        }

        tracing::debug!(
            channel = %inbound.channel,
            sender = %inbound.sender_id,
            message = %inbound.text,
            "Received message"
        );

        let ch = match channel_map.get(&inbound.channel) {
            Some(ch) => ch.clone(),
            None => {
                tracing::error!(channel = %inbound.channel, "Unknown channel");
                continue;
            }
        };

        // Show typing indicator if supported
        if ch.capabilities().typing {
            let _ = ch.start_typing(&inbound.chat_id).await;
        }

        if streaming && ch.capabilities().edit {
            // Stream via send + edit
            use futures::StreamExt;
            use rig::agent::{MultiTurnStreamItem, Text};
            use rig::streaming::StreamedAssistantContent;

            let placeholder = OutboundMessage {
                chat_id: inbound.chat_id.clone(),
                text: "...".into(),
                reply_to_id: Some(inbound.id.clone()),
                thread_id: inbound.thread_id.clone(),
                attachments: vec![],
                buttons: None,
                silent: false,
            };

            match ch.send(&placeholder).await {
                Ok(result) => {
                    let mut buffer = String::new();
                    let mut stream = agent.stream_prompt(&inbound.text).await;
                    let mut last_edit = tokio::time::Instant::now();

                    while let Some(item) = stream.next().await {
                        match item {
                            Ok(MultiTurnStreamItem::StreamAssistantItem(
                                StreamedAssistantContent::Text(Text { text }),
                            )) => {
                                buffer.push_str(&text);
                                // Throttle edits: every 500ms or 50 chars
                                if last_edit.elapsed()
                                    >= tokio::time::Duration::from_millis(500)
                                    || buffer.len() % 50 < text.len()
                                {
                                    let _ = ch
                                        .edit(
                                            &inbound.chat_id,
                                            &result.message_id,
                                            &buffer,
                                        )
                                        .await;
                                    last_edit = tokio::time::Instant::now();
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

                    // Final edit with complete text
                    if !buffer.is_empty() {
                        let _ = ch
                            .edit(&inbound.chat_id, &result.message_id, &buffer)
                            .await;
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to send placeholder");
                }
            }
        } else {
            // Non-streaming: buffer full response, send once
            match agent.prompt(&inbound.text).await {
                Ok(response) => {
                    let out = OutboundMessage {
                        chat_id: inbound.chat_id.clone(),
                        text: response,
                        reply_to_id: Some(inbound.id.clone()),
                        thread_id: inbound.thread_id.clone(),
                        attachments: vec![],
                        buttons: None,
                        silent: false,
                    };
                    if let Err(e) = ch.send(&out).await {
                        tracing::error!(error = %e, "Failed to send response");
                    }
                }
                Err(e) => {
                    let out = OutboundMessage {
                        chat_id: inbound.chat_id.clone(),
                        text: format!("Error: {e}"),
                        reply_to_id: None,
                        thread_id: inbound.thread_id.clone(),
                        attachments: vec![],
                        buttons: None,
                        silent: false,
                    };
                    tracing::error!(error = %e, "Agent prompt failed");
                    let _ = ch.send(&out).await;
                }
            }
        }
    }

    Ok(())
}
```

- [ ] **Step 2: Run `just check`**

Expected: core crate compiles. Main may fail (signature change). That's expected.

- [ ] **Step 3: Commit**

```bash
git add crates/nocelium-core/src/agent.rs
git commit -m "feat(core): rewrite agent loop for push-based multi-channel architecture"
```

---

### Task 9: Wire Up in main.rs

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Update main.rs to start configured channels**

```rust
use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

use nocelium_channels::Channel;
use nocelium_channels::stdio::StdioChannel;
#[cfg(feature = "telegram")]
use nocelium_channels::telegram::TelegramChannel;
use nocelium_core::{Config, Identity};

#[derive(Parser)]
#[command(name = "nocelium", about = "Nostr-native AI agent runtime")]
struct Cli {
    /// Path to config file
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Generate a new identity and exit
    #[arg(long)]
    gen_identity: bool,

    /// Show identity info and exit
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
    println!(
        "Provider: {} ({})",
        config.provider.provider_type, config.provider.model
    );
    println!("Streaming: {}", config.agent.streaming);

    let agent = nocelium_core::agent::build_agent(&config, &identity)?;

    // Build channel list from config
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

    println!("Type /quit to exit\n");

    nocelium_core::agent::run_loop(&agent, channels, config.agent.streaming).await?;

    Ok(())
}
```

- [ ] **Step 2: Run `just check`**

Expected: full project compiles

- [ ] **Step 3: Run `just ci`**

Expected: check + clippy + tests all pass. Fix any warnings/errors.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire up multi-channel startup with telegram support"
```

---

### Task 10: Final Validation

- [ ] **Step 1: Run `just ci`**

Full validation: check + clippy + tests.

- [ ] **Step 2: Verify config works**

Test that running with default config (telegram disabled) still works:
```bash
cargo run -- --show-identity
```

- [ ] **Step 3: Squash commits if desired, or leave as feature branch**
