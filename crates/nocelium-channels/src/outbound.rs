use serde::{Deserialize, Serialize};

/// Outbound message to send via a channel.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OutboundMessage {
    pub chat_id: String,
    pub text: String,
    pub reply_to_id: Option<String>,
    pub thread_id: Option<String>,
    pub attachments: Vec<OutboundAttachment>,
    pub buttons: Option<Vec<Vec<Button>>>,
    pub silent: bool,
}

/// Result of sending a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendResult {
    pub message_id: String,
}

/// Inline keyboard button.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Button {
    pub text: String,
    pub callback_data: Option<String>,
    pub url: Option<String>,
}

/// Outbound file attachment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundAttachment {
    pub file_path: String,
    pub caption: Option<String>,
}
