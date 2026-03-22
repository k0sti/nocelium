use serde::{Deserialize, Serialize};

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
    #[serde(default)]
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
