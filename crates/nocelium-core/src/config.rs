use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub identity: IdentityConfig,
    pub agent: AgentConfig,
    pub provider: ProviderConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub channels: ChannelsConfig,
    #[serde(default)]
    pub tools: ToolsConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct IdentityConfig {
    pub key_path: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentConfig {
    pub preamble: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u64,
    #[serde(default = "default_true")]
    pub streaming: bool,
}

fn default_max_tokens() -> u64 {
    4096
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProviderConfig {
    #[serde(rename = "type")]
    pub provider_type: String,
    pub model: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub routstr: Option<RoutstrConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RoutstrConfig {
    pub base_url: String,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct MemoryConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_socket_path")]
    pub socket_path: String,
}

fn default_socket_path() -> String {
    if let Ok(xdg) = std::env::var("XDG_RUNTIME_DIR") {
        format!("{xdg}/nomen/nomen.sock")
    } else if let Ok(user) = std::env::var("USER") {
        format!("/tmp/nomen-{user}/nomen.sock")
    } else {
        "/tmp/nomen.sock".to_string()
    }
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct ChannelsConfig {
    #[serde(default)]
    pub stdio: bool,
    #[serde(default)]
    pub telegram: Option<TelegramConfig>,
    #[serde(default)]
    pub nostr: Option<NostrConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TelegramConfig {
    #[serde(default)]
    pub enabled: bool,
    pub token: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct NostrConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub relays: Vec<String>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct ToolsConfig {
    #[serde(default = "default_true")]
    pub shell: bool,
    #[serde(default = "default_true")]
    pub filesystem: bool,
    #[serde(default = "default_true")]
    pub http: bool,
    #[serde(default)]
    pub web_search: bool,
}

fn default_true() -> bool {
    true
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Load config from default locations, falling back to defaults
    pub fn load_default() -> anyhow::Result<Self> {
        let candidates = [
            PathBuf::from("nocelium.toml"),
            dirs_or_home().join(".config/nocelium/config.toml"),
        ];

        for path in &candidates {
            if path.exists() {
                tracing::info!("Loading config from {}", path.display());
                return Self::load(path);
            }
        }

        anyhow::bail!(
            "No config file found. Create ~/.config/nocelium/config.toml or ./nocelium.toml"
        )
    }
}

fn dirs_or_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

impl IdentityConfig {
    /// Expand ~ in key_path
    pub fn expanded_key_path(&self) -> PathBuf {
        if self.key_path.starts_with("~/") {
            dirs_or_home().join(&self.key_path[2..])
        } else {
            PathBuf::from(&self.key_path)
        }
    }
}
