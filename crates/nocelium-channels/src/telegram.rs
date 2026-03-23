use anyhow::Result;
use async_trait::async_trait;
use teloxide::prelude::*;
use teloxide::types::{BotCommand, ChatKind, MessageId};
use tokio::sync::mpsc;

use crate::{
    Channel, ChannelCapabilities, ChatType, Event, Message, OutboundMessage, Payload, SendResult,
    Source,
};

/// Telegram channel using teloxide.
pub struct TelegramChannel {
    bot: Bot,
    /// If non-empty, only accept messages from these Telegram user IDs.
    allow_from: Vec<u64>,
}

impl TelegramChannel {
    pub fn new(token: &str, allow_from: Vec<u64>) -> Self {
        if !allow_from.is_empty() {
            tracing::info!(?allow_from, "Telegram: restricting to allowed user IDs");
        }
        Self {
            bot: Bot::new(token),
            allow_from,
        }
    }
}

#[async_trait]
impl Channel for TelegramChannel {
    fn name(&self) -> &str {
        "telegram"
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
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

    async fn listen(&self, tx: mpsc::Sender<Event>) -> Result<()> {
        // Register bot commands for Telegram's command menu
        let commands = vec![
            BotCommand::new("reset", "Reset conversation history"),
            BotCommand::new("reload", "Reload configuration"),
            BotCommand::new("stop", "Stop active processing"),
            BotCommand::new("status", "Show agent status"),
        ];
        if let Err(e) = self.bot.set_my_commands(commands).await {
            tracing::warn!(error = %e, "Failed to register bot commands");
        } else {
            tracing::info!("Registered bot commands");
        }

        let allow_from = self.allow_from.clone();
        let handler = Update::filter_message().endpoint(
            move |msg: teloxide::types::Message, _bot: Bot| {
                let tx = tx.clone();
                let allow_from = allow_from.clone();
                async move {
                    let text = msg.text().unwrap_or("").to_string();
                    if text.is_empty() {
                        return respond(());
                    }

                    // Filter by allowed user IDs if configured
                    if !allow_from.is_empty() {
                        let sender_uid = msg.from.as_ref().map(|u| u.id.0).unwrap_or(0);
                        if !allow_from.contains(&sender_uid) {
                            tracing::debug!(sender_uid, "Ignoring message from non-allowed user");
                            return respond(());
                        }
                    }

                    let chat_id = msg.chat.id.0.to_string();
                    let sender_id = msg
                        .from
                        .as_ref()
                        .map(|u| u.id.0.to_string())
                        .unwrap_or_default();
                    let sender_name = msg.from.as_ref().and_then(|u| u.last_name.as_ref().map(
                        |last| format!("{} {}", u.first_name, last),
                    )).or_else(|| msg.from.as_ref().map(|u| u.first_name.clone()));
                    let sender_handle = msg
                        .from
                        .as_ref()
                        .and_then(|u| u.username.clone());

                    let chat_type = match &msg.chat.kind {
                        ChatKind::Private(_) => ChatType::Direct,
                        _ => ChatType::Group,
                    };

                    let thread_id = msg.thread_id.map(|t| t.to_string());

                    let reply_to = msg.reply_to_message().map(|r| {
                        crate::event::ReplyContext {
                            message_id: r.id.0.to_string(),
                            text: r.text().map(|t| t.to_string()),
                            sender: r.from.as_ref().map(|u| u.id.0.to_string()),
                        }
                    });

                    let event = Event::new(
                        Source::Channel {
                            name: "telegram".into(),
                            chat_id,
                            sender_id,
                        },
                        Payload::Message(Box::new(Message {
                            id: msg.id.0.to_string(),
                            text,
                            sender_name,
                            sender_handle,
                            chat_type,
                            thread_id,
                            reply_to,
                            ..Default::default()
                        })),
                    );

                    let _ = tx.send(event).await;
                    respond(())
                }
            },
        );

        // Drop pending updates from before this boot
        if let Err(e) = self.bot.delete_webhook().drop_pending_updates(true).await {
            tracing::warn!(error = %e, "Failed to drop pending updates");
        } else {
            tracing::info!("Dropped pending Telegram updates");
        }

        let mut dispatcher = teloxide::dispatching::Dispatcher::builder(self.bot.clone(), handler)
            .enable_ctrlc_handler()
            .build();

        dispatcher.dispatch().await;
        Ok(())
    }

    async fn send(&self, message: &OutboundMessage) -> Result<SendResult> {
        let chat_id: i64 = message.chat_id.parse().map_err(|e| {
            anyhow::anyhow!("Invalid chat_id '{}': {}", message.chat_id, e)
        })?;

        let mut req = self.bot.send_message(ChatId(chat_id), &message.text);

        if let Some(ref reply_id) = message.reply_to_id {
            if let Ok(id) = reply_id.parse::<i32>() {
                req = req.reply_parameters(teloxide::types::ReplyParameters::new(MessageId(id)));
            }
        }

        if message.silent {
            req = req.disable_notification(true);
        }

        let sent = req.await?;
        Ok(SendResult {
            message_id: sent.id.0.to_string(),
        })
    }

    async fn edit(&self, chat_id: &str, message_id: &str, text: &str) -> Result<()> {
        let chat: i64 = chat_id.parse()?;
        let msg_id: i32 = message_id.parse()?;
        self.bot
            .edit_message_text(ChatId(chat), MessageId(msg_id), text)
            .await?;
        Ok(())
    }

    async fn delete(&self, chat_id: &str, message_id: &str) -> Result<()> {
        let chat: i64 = chat_id.parse()?;
        let msg_id: i32 = message_id.parse()?;
        self.bot
            .delete_message(ChatId(chat), MessageId(msg_id))
            .await?;
        Ok(())
    }

    async fn react(&self, chat_id: &str, message_id: &str, emoji: &str) -> Result<()> {
        use teloxide::types::ReactionType;
        let chat: i64 = chat_id.parse()?;
        let msg_id: i32 = message_id.parse()?;
        self.bot
            .set_message_reaction(ChatId(chat), MessageId(msg_id))
            .reaction(vec![ReactionType::Emoji { emoji: emoji.to_string() }])
            .await?;
        Ok(())
    }

    async fn start_typing(&self, chat_id: &str) -> Result<()> {
        let chat: i64 = chat_id.parse()?;
        self.bot
            .send_chat_action(ChatId(chat), teloxide::types::ChatAction::Typing)
            .await?;
        Ok(())
    }
}
