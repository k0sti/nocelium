use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

/// Inbound event envelope — all sources produce these.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Dispatch key: `{source}:{kind}:{context}` for pattern matching
    pub key: String,
    /// Where this event came from
    pub source: Source,
    /// Unix timestamp (seconds)
    pub timestamp: u64,
    /// Typed payload
    pub payload: Payload,
}

impl Event {
    pub fn new(source: Source, payload: Payload) -> Self {
        let key = Self::compute_key(&source, &payload);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            key,
            source,
            timestamp,
            payload,
        }
    }

    fn compute_key(source: &Source, payload: &Payload) -> String {
        let kind = match payload {
            Payload::Message(_) => "message",
            Payload::Callback(cb) => {
                return format!("{}:callback:{}", source_prefix(source), cb.data)
            }
            Payload::LocationUpdate(_) => "location",
            Payload::Media(_) => "media",
            Payload::Raw(_) => "raw",
        };
        match source {
            Source::Channel {
                name, chat_id, ..
            } => format!("{name}:{kind}:{chat_id}"),
            Source::Cron(id) => format!("cron:{id}"),
            Source::Webhook(name) => format!("webhook:{name}"),
            Source::Nostr(filter) => format!("nostr:{filter}"),
        }
    }
}

fn source_prefix(source: &Source) -> &str {
    match source {
        Source::Channel { name, .. } => name,
        Source::Cron(_) => "cron",
        Source::Webhook(_) => "webhook",
        Source::Nostr(_) => "nostr",
    }
}

/// Where an event originated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Source {
    Channel {
        name: String,
        chat_id: String,
        sender_id: String,
    },
    Cron(String),
    Webhook(String),
    Nostr(String),
}

impl Source {
    /// The channel name, if this is a channel source.
    pub fn channel_name(&self) -> Option<&str> {
        match self {
            Source::Channel { name, .. } => Some(name),
            _ => None,
        }
    }

    pub fn chat_id(&self) -> Option<&str> {
        match self {
            Source::Channel { chat_id, .. } => Some(chat_id),
            _ => None,
        }
    }
}

/// Typed event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Payload {
    Message(Box<Message>),
    Callback(Callback),
    LocationUpdate(Location),
    Media(Vec<Attachment>),
    Raw(Value),
}

/// A text message with metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub text: String,
    pub sender_name: Option<String>,
    pub sender_handle: Option<String>,
    pub chat_type: ChatType,
    pub group_subject: Option<String>,
    pub thread_id: Option<String>,
    pub reply_to: Option<ReplyContext>,
    pub mentions: Vec<String>,
    pub was_mentioned: bool,
    pub attachments: Vec<Attachment>,
    pub location: Option<Location>,
}

/// Context for a reply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyContext {
    pub message_id: String,
    pub text: Option<String>,
    pub sender: Option<String>,
}

/// Button callback data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Callback {
    pub data: String,
    pub query_id: String,
    pub message_id: Option<String>,
}

/// Chat type classification.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub enum ChatType {
    #[default]
    Direct,
    Group,
    Thread,
}

/// Geographic location.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
    pub live_period: Option<u32>,
}

/// File attachment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub kind: AttachmentKind,
    pub file_path: Option<String>,
    pub file_id: Option<String>,
    pub mime_type: Option<String>,
    pub file_size: Option<u64>,
    pub caption: Option<String>,
}

/// Attachment type classification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AttachmentKind {
    Photo,
    Video,
    Audio,
    Voice,
    Document,
    Sticker,
    Animation,
}
