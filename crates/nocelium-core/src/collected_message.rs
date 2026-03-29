use nocelium_channels::{AttachmentKind, ChatType, Event, Payload, Source};
use serde_json::{json, Value};

/// Build a kind 30100 event JSON from an inbound channel event.
///
/// Returns `None` if the event is not a channel message.
pub fn build_inbound_30100(event: &Event) -> Option<Value> {
    let msg = match &event.payload {
        Payload::Message(msg) => msg,
        _ => return None,
    };
    let (platform, chat_id, sender_id) = match &event.source {
        Source::Channel {
            name,
            chat_id,
            sender_id,
        } => (name.as_str(), chat_id.as_str(), sender_id.as_str()),
        _ => return None,
    };

    let d_tag = format!("{platform}:{chat_id}:{}", msg.id);
    let mut tags: Vec<Value> = vec![
        json!(["d", &d_tag]),
        json!(["platform", platform]),
        json!(["proxy", &d_tag, platform]),
        json!([
            "chat",
            chat_id,
            msg.group_subject.as_deref().unwrap_or(""),
            chat_type_str(&msg.chat_type),
        ]),
        json!([
            "sender",
            sender_id,
            msg.sender_name.as_deref().unwrap_or(""),
            msg.sender_handle.as_deref().unwrap_or(""),
        ]),
    ];

    if let Some(ref thread_id) = msg.thread_id {
        tags.push(json!([
            "thread",
            thread_id,
            msg.thread_name.as_deref().unwrap_or(""),
        ]));
    }

    if let Some(ref reply) = msg.reply_to {
        let reply_d = format!("{platform}:{chat_id}:{}", reply.message_id);
        tags.push(json!([
            "reply",
            reply_d,
            reply.text.as_deref().unwrap_or(""),
            reply.sender.as_deref().unwrap_or(""),
        ]));
    }

    if let Some(ref fwd) = msg.forward_from {
        tags.push(json!([
            "forward",
            &fwd.source_id,
            fwd.source_name.as_deref().unwrap_or(""),
            fwd.source_type.as_deref().unwrap_or(""),
        ]));
    }

    if let Some(edit_ts) = msg.edit_date {
        tags.push(json!(["edited", edit_ts.to_string()]));
    }

    for att in &msg.attachments {
        let mut parts = vec!["imeta".to_string()];
        if let Some(ref path) = att.file_path {
            parts.push(format!("url {path}"));
        }
        if let Some(ref mime) = att.mime_type {
            parts.push(format!("m {mime}"));
        }
        if let Some(size) = att.file_size {
            parts.push(format!("size {size}"));
        }
        parts.push(format!("alt {}", attachment_kind_str(&att.kind)));
        if let Some(ref fid) = att.file_id {
            parts.push(format!("file_id {fid}"));
        }
        tags.push(Value::Array(parts.into_iter().map(Value::String).collect()));
    }

    if let Some(ref loc) = msg.location {
        tags.push(json!([
            "location",
            loc.latitude.to_string(),
            loc.longitude.to_string()
        ]));
    }

    // Attachment captions go into content (after message text)
    let content = build_content(&msg.text, &msg.attachments);

    Some(json!({
        "kind": 30100,
        "pubkey": "",
        "content": content,
        "tags": tags,
        "created_at": event.timestamp,
    }))
}

/// Build a kind 30100 event JSON for an outbound agent response.
pub fn build_outbound_30100(
    platform: &str,
    chat_id: &str,
    text: &str,
    message_id: &str,
    identity_npub: &str,
) -> Value {
    let d_tag = format!("{platform}:{chat_id}:{message_id}");
    json!({
        "kind": 30100,
        "pubkey": identity_npub,
        "content": text,
        "tags": [
            ["d", d_tag],
            ["platform", platform],
            ["proxy", d_tag, platform],
            ["chat", chat_id, "", ""],
            ["sender", identity_npub, "agent", ""],
            ["direction", "outbound"],
        ],
        "created_at": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    })
}

/// Build content string: message text + attachment captions (if any).
fn build_content(text: &str, attachments: &[nocelium_channels::Attachment]) -> String {
    let captions: Vec<&str> = attachments
        .iter()
        .filter_map(|a| a.caption.as_deref())
        .collect();
    if captions.is_empty() {
        return text.to_string();
    }
    if text.is_empty() {
        return captions.join("\n");
    }
    format!("{text}\n\n{}", captions.join("\n"))
}

fn chat_type_str(ct: &ChatType) -> &'static str {
    match ct {
        ChatType::Direct => "direct",
        ChatType::Group => "group",
        ChatType::Thread => "thread",
    }
}

fn attachment_kind_str(k: &AttachmentKind) -> &'static str {
    match k {
        AttachmentKind::Photo => "photo",
        AttachmentKind::Video => "video",
        AttachmentKind::Audio => "audio",
        AttachmentKind::Voice => "voice",
        AttachmentKind::Document => "document",
        AttachmentKind::Sticker => "sticker",
        AttachmentKind::Animation => "animation",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nocelium_channels::{Attachment, ForwardInfo, Location, Message, ReplyContext};

    fn make_channel_event(msg: Message) -> Event {
        Event::new(
            Source::Channel {
                name: "telegram".into(),
                chat_id: "-1001234".into(),
                sender_id: "user42".into(),
            },
            Payload::Message(Box::new(msg)),
        )
    }

    #[test]
    fn basic_message() {
        let event = make_channel_event(Message {
            id: "99".into(),
            text: "hello world".into(),
            sender_name: Some("Alice".into()),
            sender_handle: Some("alice".into()),
            chat_type: ChatType::Group,
            group_subject: Some("Test Group".into()),
            ..Default::default()
        });
        let json = build_inbound_30100(&event).unwrap();
        assert_eq!(json["kind"], 30100);
        assert_eq!(json["content"], "hello world");

        let tags = json["tags"].as_array().unwrap();
        let d = tags[0].as_array().unwrap();
        assert_eq!(d[1], "telegram:-1001234:99");

        // platform tag
        let plat = tags[1].as_array().unwrap();
        assert_eq!(plat[0], "platform");
        assert_eq!(plat[1], "telegram");

        // proxy tag: NIP-48 format ["proxy", d_tag, platform]
        let proxy = tags[2].as_array().unwrap();
        assert_eq!(proxy[0], "proxy");
        assert_eq!(proxy[1], "telegram:-1001234:99");
        assert_eq!(proxy[2], "telegram");

        let sender = tags[4].as_array().unwrap();
        assert_eq!(sender[1], "user42");
        assert_eq!(sender[2], "Alice");
        assert_eq!(sender[3], "alice");

        let chat = tags[3].as_array().unwrap();
        assert_eq!(chat[3], "group");
    }

    #[test]
    fn skips_non_message() {
        let event = Event::new(
            Source::Channel {
                name: "stdio".into(),
                chat_id: "local".into(),
                sender_id: "user".into(),
            },
            Payload::Raw(json!({"foo": "bar"})),
        );
        assert!(build_inbound_30100(&event).is_none());
    }

    #[test]
    fn skips_non_channel() {
        let event = Event::new(
            Source::Cron("task1".into()),
            Payload::Message(Box::new(Message {
                text: "cron fired".into(),
                ..Default::default()
            })),
        );
        assert!(build_inbound_30100(&event).is_none());
    }

    #[test]
    fn with_thread_and_reply() {
        let event = make_channel_event(Message {
            id: "50".into(),
            text: "reply text".into(),
            thread_id: Some("100".into()),
            thread_name: Some("General".into()),
            reply_to: Some(ReplyContext {
                message_id: "49".into(),
                text: Some("original".into()),
                sender: Some("Bob".into()),
            }),
            ..Default::default()
        });
        let json = build_inbound_30100(&event).unwrap();
        let tags = json["tags"].as_array().unwrap();

        let thread = tags.iter().find(|t| t[0] == "thread").unwrap();
        assert_eq!(thread[1], "100");
        assert_eq!(thread[2], "General");

        let reply = tags.iter().find(|t| t[0] == "reply").unwrap();
        assert_eq!(reply[1], "telegram:-1001234:49");
        assert_eq!(reply[2], "original");
        assert_eq!(reply[3], "Bob");
    }

    #[test]
    fn with_attachment_nip92() {
        let event = make_channel_event(Message {
            id: "60".into(),
            text: "".into(),
            attachments: vec![Attachment {
                kind: AttachmentKind::Photo,
                file_path: Some("https://example.com/photo.jpg".into()),
                file_id: Some("abc123".into()),
                mime_type: Some("image/jpeg".into()),
                file_size: Some(54321),
                caption: Some("a photo".into()),
            }],
            ..Default::default()
        });
        let json = build_inbound_30100(&event).unwrap();

        // Caption moves to content
        assert_eq!(json["content"], "a photo");

        let tags = json["tags"].as_array().unwrap();
        let imeta = tags.iter().find(|t| t[0] == "imeta").unwrap();
        let parts: Vec<&str> = imeta
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        // NIP-92 fields
        assert!(parts.contains(&"url https://example.com/photo.jpg"));
        assert!(parts.contains(&"m image/jpeg"));
        assert!(parts.contains(&"size 54321"));
        assert!(parts.contains(&"alt photo"));
        // Extension field for platform re-fetch
        assert!(parts.contains(&"file_id abc123"));
        // `kind photo` should NOT be present (was non-standard)
        assert!(!parts.iter().any(|p| p.starts_with("kind ")));
        // `caption` should NOT be in imeta
        assert!(!parts.iter().any(|p| p.starts_with("caption ")));
    }

    #[test]
    fn caption_appended_to_content() {
        let event = make_channel_event(Message {
            id: "61".into(),
            text: "some text".into(),
            attachments: vec![Attachment {
                kind: AttachmentKind::Document,
                file_path: None,
                file_id: None,
                mime_type: None,
                file_size: None,
                caption: Some("doc caption".into()),
            }],
            ..Default::default()
        });
        let json = build_inbound_30100(&event).unwrap();
        assert_eq!(json["content"], "some text\n\ndoc caption");
    }

    #[test]
    fn with_location() {
        let event = make_channel_event(Message {
            id: "70".into(),
            text: "I'm here".into(),
            location: Some(Location {
                latitude: 37.7749,
                longitude: -122.4194,
                live_period: None,
            }),
            ..Default::default()
        });
        let json = build_inbound_30100(&event).unwrap();
        let tags = json["tags"].as_array().unwrap();
        let loc = tags.iter().find(|t| t[0] == "location").unwrap();
        assert_eq!(loc[1], "37.7749");
        assert_eq!(loc[2], "-122.4194");
    }

    #[test]
    fn with_forward() {
        let event = make_channel_event(Message {
            id: "80".into(),
            text: "forwarded msg".into(),
            forward_from: Some(ForwardInfo {
                source_id: "user99".into(),
                source_name: Some("Charlie".into()),
                source_type: Some("user".into()),
            }),
            ..Default::default()
        });
        let json = build_inbound_30100(&event).unwrap();
        let tags = json["tags"].as_array().unwrap();
        let fwd = tags.iter().find(|t| t[0] == "forward").unwrap();
        assert_eq!(fwd[1], "user99");
        assert_eq!(fwd[2], "Charlie");
        assert_eq!(fwd[3], "user");
    }

    #[test]
    fn with_edit_date() {
        let event = make_channel_event(Message {
            id: "90".into(),
            text: "edited msg".into(),
            edit_date: Some(1711100000),
            ..Default::default()
        });
        let json = build_inbound_30100(&event).unwrap();
        let tags = json["tags"].as_array().unwrap();
        let edited = tags.iter().find(|t| t[0] == "edited").unwrap();
        assert_eq!(edited[1], "1711100000");
    }

    #[test]
    fn outbound_event() {
        let json = build_outbound_30100("telegram", "-1001234", "hi back", "101", "npub1abc");
        assert_eq!(json["kind"], 30100);
        assert_eq!(json["content"], "hi back");

        let tags = json["tags"].as_array().unwrap();
        let d = tags[0].as_array().unwrap();
        assert_eq!(d[1], "telegram:-1001234:101");

        // platform tag
        let plat = tags[1].as_array().unwrap();
        assert_eq!(plat[0], "platform");
        assert_eq!(plat[1], "telegram");

        // proxy uses d_tag, not chat_id
        let proxy = tags[2].as_array().unwrap();
        assert_eq!(proxy[1], "telegram:-1001234:101");
        assert_eq!(proxy[2], "telegram");

        let sender = tags.iter().find(|t| t[0] == "sender").unwrap();
        assert_eq!(sender[1], "npub1abc");
        assert_eq!(sender[2], "agent");

        let dir = tags.iter().find(|t| t[0] == "direction").unwrap();
        assert_eq!(dir[1], "outbound");
    }
}
