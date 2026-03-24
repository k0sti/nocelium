//! Telegram tools — allow the agent to perform Telegram actions
//! (send messages, react, edit, delete, etc.)

use std::sync::Arc;
use tokio::sync::RwLock;

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use schemars::JsonSchema;
use serde::Deserialize;

use nocelium_channels::{Channel, OutboundMessage};
use crate::error::NomenToolError;

/// Shared context set by the agent loop before each turn.
/// Gives tools access to the current channel + chat.
#[derive(Clone)]
pub struct TelegramContext {
    inner: Arc<RwLock<Option<TelegramContextInner>>>,
}

struct TelegramContextInner {
    channel: Arc<dyn Channel>,
    chat_id: String,
    /// The inbound message ID (for replying/reacting)
    message_id: Option<String>,
    thread_id: Option<String>,
}

impl Default for TelegramContext {
    fn default() -> Self {
        Self::new()
    }
}

impl TelegramContext {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
        }
    }

    /// Called by the agent loop before processing each message.
    pub async fn set(
        &self,
        channel: Arc<dyn Channel>,
        chat_id: String,
        message_id: Option<String>,
        thread_id: Option<String>,
    ) {
        *self.inner.write().await = Some(TelegramContextInner {
            channel,
            chat_id,
            message_id,
            thread_id,
        });
    }

    pub async fn clear(&self) {
        *self.inner.write().await = None;
    }

    async fn get(&self) -> Result<(Arc<dyn Channel>, String, Option<String>, Option<String>), NomenToolError> {
        let guard = self.inner.read().await;
        match guard.as_ref() {
            Some(ctx) => Ok((
                Arc::clone(&ctx.channel),
                ctx.chat_id.clone(),
                ctx.message_id.clone(),
                ctx.thread_id.clone(),
            )),
            None => Err(NomenToolError::Memory("No active Telegram context".into())),
        }
    }
}

// ──────────────────────────────────────────────
// Tool: telegram_send
// ──────────────────────────────────────────────

#[derive(Deserialize, JsonSchema)]
pub struct TelegramSendInput {
    /// Text to send
    pub text: String,
    /// Reply to a specific message ID (optional)
    pub reply_to: Option<String>,
    /// Send silently (no notification)
    #[serde(default)]
    pub silent: bool,
}

pub struct TelegramSendTool {
    ctx: TelegramContext,
}

impl TelegramSendTool {
    pub fn new(ctx: TelegramContext) -> Self {
        Self { ctx }
    }
}

impl Tool for TelegramSendTool {
    const NAME: &'static str = "telegram_send";
    type Error = NomenToolError;
    type Args = TelegramSendInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "telegram_send".into(),
            description: "Send a message in the current Telegram chat. Returns the sent message ID.".into(),
            parameters: serde_json::to_value(schemars::schema_for!(TelegramSendInput)).unwrap_or_default(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let (channel, chat_id, _, thread_id) = self.ctx.get().await?;
        let msg = OutboundMessage {
            chat_id,
            text: args.text,
            reply_to_id: args.reply_to,
            thread_id,
            silent: args.silent,
            ..Default::default()
        };
        let result = channel.send(&msg).await
            .map_err(|e| NomenToolError::Memory(e.to_string()))?;
        Ok(format!("Sent message_id={}", result.message_id))
    }
}

// ──────────────────────────────────────────────
// Tool: telegram_edit
// ──────────────────────────────────────────────

#[derive(Deserialize, JsonSchema)]
pub struct TelegramEditInput {
    /// ID of the message to edit
    pub message_id: String,
    /// New text for the message
    pub text: String,
}

pub struct TelegramEditTool {
    ctx: TelegramContext,
}

impl TelegramEditTool {
    pub fn new(ctx: TelegramContext) -> Self {
        Self { ctx }
    }
}

impl Tool for TelegramEditTool {
    const NAME: &'static str = "telegram_edit";
    type Error = NomenToolError;
    type Args = TelegramEditInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "telegram_edit".into(),
            description: "Edit a previously sent message in the current Telegram chat.".into(),
            parameters: serde_json::to_value(schemars::schema_for!(TelegramEditInput)).unwrap_or_default(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let (channel, chat_id, _, _) = self.ctx.get().await?;
        channel.edit(&chat_id, &args.message_id, &args.text).await
            .map_err(|e| NomenToolError::Memory(e.to_string()))?;
        Ok(format!("Edited message_id={}", args.message_id))
    }
}

// ──────────────────────────────────────────────
// Tool: telegram_delete
// ──────────────────────────────────────────────

#[derive(Deserialize, JsonSchema)]
pub struct TelegramDeleteInput {
    /// ID of the message to delete
    pub message_id: String,
}

pub struct TelegramDeleteTool {
    ctx: TelegramContext,
}

impl TelegramDeleteTool {
    pub fn new(ctx: TelegramContext) -> Self {
        Self { ctx }
    }
}

impl Tool for TelegramDeleteTool {
    const NAME: &'static str = "telegram_delete";
    type Error = NomenToolError;
    type Args = TelegramDeleteInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "telegram_delete".into(),
            description: "Delete a message in the current Telegram chat.".into(),
            parameters: serde_json::to_value(schemars::schema_for!(TelegramDeleteInput)).unwrap_or_default(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let (channel, chat_id, _, _) = self.ctx.get().await?;
        channel.delete(&chat_id, &args.message_id).await
            .map_err(|e| NomenToolError::Memory(e.to_string()))?;
        Ok(format!("Deleted message_id={}", args.message_id))
    }
}

// ──────────────────────────────────────────────
// Tool: telegram_react
// ──────────────────────────────────────────────

#[derive(Deserialize, JsonSchema)]
pub struct TelegramReactInput {
    /// Emoji to react with (e.g. "👍", "❤️")
    pub emoji: String,
    /// Message ID to react to. If omitted, reacts to the current inbound message.
    pub message_id: Option<String>,
}

pub struct TelegramReactTool {
    ctx: TelegramContext,
}

impl TelegramReactTool {
    pub fn new(ctx: TelegramContext) -> Self {
        Self { ctx }
    }
}

impl Tool for TelegramReactTool {
    const NAME: &'static str = "telegram_react";
    type Error = NomenToolError;
    type Args = TelegramReactInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "telegram_react".into(),
            description: "React to a message with an emoji in the current Telegram chat.".into(),
            parameters: serde_json::to_value(schemars::schema_for!(TelegramReactInput)).unwrap_or_default(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let (channel, chat_id, current_msg_id, _) = self.ctx.get().await?;
        let msg_id = args.message_id.or(current_msg_id)
            .ok_or_else(|| NomenToolError::Memory("No message_id to react to".into()))?;
        channel.react(&chat_id, &msg_id, &args.emoji).await
            .map_err(|e| NomenToolError::Memory(e.to_string()))?;
        Ok(format!("Reacted {} to message_id={}", args.emoji, msg_id))
    }
}
