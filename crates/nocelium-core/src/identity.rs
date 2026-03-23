use anyhow::Result;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::config::IdentityConfig;

#[derive(Debug, Clone)]
pub struct Identity {
    pub keys: Keys,
}

#[derive(Serialize, Deserialize)]
struct StoredIdentity {
    secret_key_hex: String,
}

impl Identity {
    /// Load existing identity or generate a new one
    pub fn load_or_generate(config: &IdentityConfig) -> Result<Self> {
        let key_path = config.expanded_key_path();

        if key_path.exists() {
            Self::load(&key_path)
        } else {
            let identity = Self::generate()?;
            identity.save(&key_path)?;
            Ok(identity)
        }
    }

    /// Generate a fresh keypair
    pub fn generate() -> Result<Self> {
        let keys = Keys::generate();
        tracing::info!(
            npub = %keys.public_key().to_bech32()?,
            "Generated new Nostr identity"
        );
        Ok(Self { keys })
    }

    /// Load identity from a JSON file
    fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let stored: StoredIdentity = serde_json::from_str(&content)?;
        let keys = Keys::parse(&stored.secret_key_hex)?;
        tracing::info!(
            npub = %keys.public_key().to_bech32()?,
            "Loaded Nostr identity"
        );
        Ok(Self { keys })
    }

    /// Save identity to a JSON file
    fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let stored = StoredIdentity {
            secret_key_hex: self.keys.secret_key().to_secret_hex(),
        };
        let content = serde_json::to_string_pretty(&stored)?;
        std::fs::write(path, content)?;
        // Restrict permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        tracing::info!(path = %path.display(), "Saved identity");
        Ok(())
    }

    pub fn npub(&self) -> String {
        self.keys
            .public_key()
            .to_bech32()
            .unwrap_or_else(|_| format!("{}", self.keys.public_key()))
    }

    pub fn nsec(&self) -> String {
        self.keys
            .secret_key()
            .to_bech32()
            .unwrap_or_else(|_| "invalid".to_string())
    }
}
