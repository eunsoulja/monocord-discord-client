//! OS keychain wrapper. Secrets are `SecretString`s end to end.

use keyring::Entry;
use secrecy::{ExposeSecret, SecretString};

/// Well-known secret slots.
pub mod slot {
    /// Discord `OAuth2` access token.
    pub const DISCORD_ACCESS_TOKEN: &str = "discord.access_token";
    /// Discord `OAuth2` refresh token.
    pub const DISCORD_REFRESH_TOKEN: &str = "discord.refresh_token";
    /// Discord bot token.
    pub const DISCORD_BOT_TOKEN: &str = "discord.bot_token";
}

/// Errors from the secret store.
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// Keychain backend failure.
    #[error("keychain error: {0}")]
    Backend(String),
    /// Secret not present.
    #[error("secret not found")]
    NotFound,
}

/// Thin wrapper over the platform credential store.
///
/// Service name is fixed; account is the slot constant.
pub struct SecretStore {
    service: String,
}

impl SecretStore {
    /// Create a store bound to the `l` service name.
    #[must_use]
    pub fn new() -> Self {
        Self {
            service: "l.desktop".to_string(),
        }
    }

    fn entry(&self, account: &str) -> Result<Entry, SecretError> {
        Entry::new(&self.service, account).map_err(|e| SecretError::Backend(e.to_string()))
    }

    /// Store a secret (write-only from the UI's perspective).
    pub fn set(&self, account: &str, secret: &SecretString) -> Result<(), SecretError> {
        self.entry(account)?
            .set_password(secret.expose_secret())
            .map_err(|e| SecretError::Backend(e.to_string()))
    }

    /// Read a secret. Returns `None` if absent.
    pub fn get(&self, account: &str) -> Result<Option<SecretString>, SecretError> {
        match self.entry(account)?.get_password() {
            Ok(s) => Ok(Some(SecretString::from(s))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SecretError::Backend(e.to_string())),
        }
    }

    /// Delete a secret. Missing entries are not an error.
    pub fn delete(&self, account: &str) -> Result<(), SecretError> {
        match self.entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(SecretError::Backend(e.to_string())),
        }
    }
}

impl Default for SecretStore {
    fn default() -> Self {
        Self::new()
    }
}
