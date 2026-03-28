use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

fn deserialize_timestamp<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNum {
        Str(String),
        Num(u64),
        Null,
    }
    match StringOrNum::deserialize(deserializer)? {
        StringOrNum::Str(s) => Ok(Some(s)),
        StringOrNum::Num(n) => Ok(Some(n.to_string())),
        StringOrNum::Null => Ok(None),
    }
}

/// A memory record returned by Nomen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    #[serde(default)]
    pub topic: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_timestamp")]
    pub created_at: Option<String>,
    /// Search match type (hybrid, vector, text, etc.)
    #[serde(default)]
    pub match_type: Option<String>,
}

/// Visibility level for memory storage.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    #[default]
    Public,
    Group,
    Circle,
    Personal,
    Internal,
}

impl Visibility {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Group => "group",
            Self::Circle => "circle",
            Self::Personal => "personal",
            Self::Internal => "internal",
        }
    }
}

/// Query filters for collected messages.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageQueryParams {
    #[serde(rename = "#platform", skip_serializing_if = "Option::is_none")]
    pub platform: Option<Vec<String>>,
    #[serde(rename = "#community", skip_serializing_if = "Option::is_none")]
    pub community: Option<Vec<String>>,
    #[serde(rename = "#chat", skip_serializing_if = "Option::is_none")]
    pub chat: Option<Vec<String>>,
    #[serde(rename = "#thread", skip_serializing_if = "Option::is_none")]
    pub thread: Option<Vec<String>>,
    #[serde(rename = "#sender", skip_serializing_if = "Option::is_none")]
    pub sender: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// Context window request for collected messages.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageContextParams {
    #[serde(rename = "#platform", skip_serializing_if = "Option::is_none")]
    pub platform: Option<Vec<String>>,
    #[serde(rename = "#community", skip_serializing_if = "Option::is_none")]
    pub community: Option<Vec<String>>,
    #[serde(rename = "#chat", skip_serializing_if = "Option::is_none")]
    pub chat: Option<Vec<String>>,
    #[serde(rename = "#thread", skip_serializing_if = "Option::is_none")]
    pub thread: Option<Vec<String>>,
    #[serde(rename = "#sender", skip_serializing_if = "Option::is_none")]
    pub sender: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// Minimal tolerant representation of a collected message event.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CollectedMessageEvent {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub pubkey: Option<String>,
    #[serde(default)]
    pub kind: Option<u64>,
    #[serde(default)]
    pub created_at: Option<u64>,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub tags: Vec<Vec<String>>,
    #[serde(default)]
    pub score: Option<f64>,
}

impl CollectedMessageEvent {
    fn first_tag_value(&self, tag_name: &str) -> Option<&str> {
        self.tags
            .iter()
            .find(|tag| tag.first().map(|v| v.as_str()) == Some(tag_name))
            .and_then(|tag| tag.get(1).map(|v| v.as_str()))
    }

    pub fn platform(&self) -> Option<&str> {
        // Prefer dedicated platform tag, fall back to proxy tag for compat
        self.first_tag_value("platform")
            .or_else(|| {
                self.tags
                    .iter()
                    .find(|tag| tag.first().map(|v| v.as_str()) == Some("proxy"))
                    .and_then(|tag| tag.get(2).map(|v| v.as_str()))
            })
    }

    pub fn chat_id(&self) -> Option<&str> {
        self.first_tag_value("chat")
    }

    pub fn thread_id(&self) -> Option<&str> {
        self.first_tag_value("thread")
    }

    pub fn sender_id(&self) -> Option<&str> {
        self.first_tag_value("sender")
    }
}

/// Response payload for collected-message queries/context requests.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CollectedMessageQueryResult {
    #[serde(default)]
    pub count: usize,
    #[serde(default)]
    pub events: Vec<CollectedMessageEvent>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn message_query_params_serialize_canonical_filters() {
        let params = MessageQueryParams {
            platform: Some(vec!["telegram".into()]),
            chat: Some(vec!["-1003821690204".into()]),
            thread: Some(vec!["12983".into()]),
            since: Some(json!(1711540200)),
            limit: Some(50),
            ..Default::default()
        };

        let value = serde_json::to_value(&params).unwrap();
        assert_eq!(value["#platform"], json!(["telegram"]));
        assert_eq!(value["#chat"], json!(["-1003821690204"]));
        assert_eq!(value["#thread"], json!(["12983"]));
        assert_eq!(value["since"], json!(1711540200));
        assert_eq!(value["limit"], json!(50));
        assert!(value.get("channel").is_none());
    }

    #[test]
    fn collected_message_query_result_deserializes_tolerantly() {
        let value = json!({
            "count": 1,
            "events": [{
                "kind": 30100,
                "created_at": 1711540200,
                "content": "hello",
                "tags": [
                    ["platform", "telegram"],
                    ["proxy", "telegram:-1003821690204:1", "telegram"],
                    ["chat", "-1003821690204"],
                    ["thread", "12983"]
                ],
                "score": 4.2
            }]
        });

        let result: CollectedMessageQueryResult = serde_json::from_value(value).unwrap();
        assert_eq!(result.count, 1);
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].kind, Some(30100));
        assert_eq!(result.events[0].content, "hello");
        assert_eq!(result.events[0].tags[0][0], "platform");
        assert_eq!(result.events[0].platform(), Some("telegram"));
        assert_eq!(result.events[0].chat_id(), Some("-1003821690204"));
        assert_eq!(result.events[0].thread_id(), Some("12983"));
        assert_eq!(result.events[0].score, Some(4.2));
    }
}
