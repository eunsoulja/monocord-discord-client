//! Central Discord REST client with rate-limit handling.
//!
//! All REST traffic for user-OAuth and bot tokens flows through this client:
//! it honors `X-RateLimit-*` headers, retries 429s with `retry_after` + jitter,
//! and applies exponential backoff on transport errors.

use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::time::sleep;

use crate::oauth;

/// Discord API base.
pub const API_BASE: &str = "https://discord.com/api/v10";

/// Which credential authenticates a request.
#[derive(Debug, Clone)]
pub enum TokenKind {
    /// `OAuth2` user access token.
    User(SecretString),
    /// Bot token.
    Bot(SecretString),
}

impl TokenKind {
    fn header(&self) -> String {
        match self {
            Self::User(t) | Self::Bot(t) => format!("Bearer {}", t.expose_secret()),
        }
    }
}

/// REST errors.
#[derive(Debug, Error)]
pub enum RestError {
    /// HTTP error status.
    #[error("discord api {status}: {body}")]
    Status {
        /// HTTP status.
        status: u16,
        /// Truncated body.
        body: String,
    },
    /// Transport/timeout error.
    #[error("request failed: {0}")]
    Transport(String),
    /// Rate limited beyond retry budget.
    #[error("rate limited")]
    RateLimited,
    /// JSON decode error.
    #[error("bad json: {0}")]
    Json(String),
}

/// Shared rate-limit state per route bucket.
struct Bucket {
    remaining: i64,
    reset_at: std::time::Instant,
    global: bool,
}

impl Default for Bucket {
    fn default() -> Self {
        Self {
            remaining: i64::MAX,
            reset_at: std::time::Instant::now(),
            global: false,
        }
    }
}

/// The Discord REST client.
pub struct DiscordRest {
    http: reqwest::Client,
    buckets: Mutex<std::collections::HashMap<String, Bucket>>,
}

impl DiscordRest {
    /// Create a client with sane timeouts.
    #[must_use]
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(8))
            .user_agent("L (https://github.com/l, 0.1)")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            http,
            buckets: Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// GET a Discord API path with a user token.
    pub async fn user_get(&self, token: &SecretString, path: &str) -> Result<Value, RestError> {
        self.request(&TokenKind::User(token.clone()), "GET", path, None)
            .await
    }

    /// GET with bot token.
    pub async fn bot_get(&self, token: &SecretString, path: &str) -> Result<Value, RestError> {
        self.request(&TokenKind::Bot(token.clone()), "GET", path, None)
            .await
    }

    /// POST with user token and JSON body.
    pub async fn user_post(
        &self,
        token: &SecretString,
        path: &str,
        body: &Value,
    ) -> Result<Value, RestError> {
        self.request(&TokenKind::User(token.clone()), "POST", path, Some(body))
            .await
    }

    /// PATCH/POST with bot token and JSON body.
    pub async fn bot_post(
        &self,
        token: &SecretString,
        path: &str,
        body: &Value,
    ) -> Result<Value, RestError> {
        self.request(&TokenKind::Bot(token.clone()), "POST", path, Some(body))
            .await
    }

    /// PATCH with bot token and JSON body.
    pub async fn bot_patch(
        &self,
        token: &SecretString,
        path: &str,
        body: &Value,
    ) -> Result<Value, RestError> {
        self.request(&TokenKind::Bot(token.clone()), "PATCH", path, Some(body))
            .await
    }

    /// PUT with bot token and JSON body.
    pub async fn bot_put(
        &self,
        token: &SecretString,
        path: &str,
        body: &Value,
    ) -> Result<Value, RestError> {
        self.request(&TokenKind::Bot(token.clone()), "PUT", path, Some(body))
            .await
    }

    /// DELETE with bot token.
    pub async fn bot_delete(&self, token: &SecretString, path: &str) -> Result<Value, RestError> {
        self.request(&TokenKind::Bot(token.clone()), "DELETE", path, None)
            .await
    }

    /// GET with bot token and query string already encoded.
    pub async fn bot_get_query(
        &self,
        token: &SecretString,
        path: &str,
        query: &str,
    ) -> Result<Value, RestError> {
        let full = if query.is_empty() {
            path.to_string()
        } else {
            format!("{path}?{query}")
        };
        self.bot_get(token, &full).await
    }

    /// Generic request with retries and rate-limit handling.
    async fn request(
        &self,
        auth: &TokenKind,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, RestError> {
        let url = format!("{API_BASE}{path}");
        let bucket_key = format!("{method} {path}");
        let mut attempt = 0u32;

        loop {
            attempt += 1;
            if attempt > 6 {
                return Err(RestError::RateLimited);
            }

            // Respect stored bucket state.
            {
                let buckets = self.buckets.lock().await;
                if let Some(b) = buckets.get(&bucket_key) {
                    if b.remaining <= 0 && std::time::Instant::now() < b.reset_at {
                        let wait = b.reset_at.duration_since(std::time::Instant::now());
                        drop(buckets);
                        sleep(wait + Duration::from_millis(50)).await;
                        continue;
                    }
                    if b.global {
                        drop(buckets);
                        sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                }
            }

            let mut req = self
                .http
                .request(method.parse().unwrap_or(reqwest::Method::GET), &url)
                .header(reqwest::header::AUTHORIZATION, auth.header())
                .header(reqwest::header::CONTENT_TYPE, "application/json");
            if let Some(b) = body {
                req = req.json(b);
            }

            let resp = match req.send().await {
                Ok(r) => r,
                Err(e) => {
                    // Exponential backoff with jitter.
                    let base = 500u64 * 2u64.pow(attempt.min(4));
                    let jitter = rand_jitter(base);
                    sleep(Duration::from_millis(jitter)).await;
                    if attempt >= 6 {
                        return Err(RestError::Transport(e.to_string()));
                    }
                    continue;
                }
            };

            let status = resp.status().as_u16();

            // Update bucket state from headers.
            {
                let remaining = resp
                    .headers()
                    .get("x-ratelimit-remaining")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<i64>().ok());
                let reset_after = resp
                    .headers()
                    .get("x-ratelimit-reset-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<f64>().ok());
                let global = resp
                    .headers()
                    .get("x-ratelimit-global")
                    .is_some_and(|v| v == "true");
                let mut buckets = self.buckets.lock().await;
                let b = buckets.entry(bucket_key.clone()).or_default();
                if let Some(r) = remaining {
                    b.remaining = r;
                }
                if let Some(rs) = reset_after {
                    b.reset_at = std::time::Instant::now() + Duration::from_secs_f64(rs);
                }
                b.global = global;
            }

            if status == 429 {
                let retry_after = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or(1.0);
                sleep(
                    Duration::from_secs_f64(retry_after) + Duration::from_millis(rand_jitter(100)),
                )
                .await;
                continue;
            }

            if status == 204 {
                return Ok(Value::Null);
            }

            let text = resp
                .text()
                .await
                .map_err(|e| RestError::Transport(e.to_string()))?;
            if !(200..300).contains(&status) {
                let snippet: String = text.chars().take(300).collect();
                return Err(RestError::Status {
                    status,
                    body: snippet,
                });
            }
            return serde_json::from_str(&text).map_err(|e| RestError::Json(e.to_string()));
        }
    }
}

impl Default for DiscordRest {
    fn default() -> Self {
        Self::new()
    }
}

fn rand_jitter(base: u64) -> u64 {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    base + rng.gen_range(0..=base / 4)
}

/// Fetch the user profile for an OAuth token (`/users/@me`).
pub async fn fetch_user(rest: &DiscordRest, token: &SecretString) -> Result<Value, RestError> {
    rest.user_get(token, "/users/@me").await
}

/// Fetch guilds for an OAuth token (`/users/@me/guilds`).
pub async fn fetch_user_guilds(
    rest: &DiscordRest,
    token: &SecretString,
) -> Result<Value, RestError> {
    rest.user_get(token, "/users/@me/guilds").await
}

/// Fetch a guild with approximate counts (`with_counts=true`).
pub async fn fetch_guild_counts(
    rest: &DiscordRest,
    token: &SecretString,
    guild_id: &str,
) -> Result<Value, RestError> {
    rest.user_get(token, &format!("/guilds/{guild_id}?with_counts=true"))
        .await
}

/// Verify a bot token (`/users/@me` with bot auth).
pub async fn verify_bot(rest: &DiscordRest, token: &SecretString) -> Result<Value, RestError> {
    rest.bot_get(token, "/users/@me").await
}

/// Get gateway info (`/gateway/bot`) — bot token required.
pub async fn gateway_info(rest: &DiscordRest, token: &SecretString) -> Result<Value, RestError> {
    rest.bot_get(token, "/gateway/bot").await
}

/// Refresh tokens if needed and persist via callback.
pub async fn ensure_fresh(
    rest: &DiscordRest,
    client_id: &str,
    tokens: &mut oauth::OAuthTokens,
) -> Result<(), oauth::OAuthError> {
    // Caller tracks expiry; refresh proactively when <60s remain.
    if tokens.expires_in < 60
        && let Some(rt) = &tokens.refresh_token
    {
        *tokens = oauth::refresh_token(client_id, rt).await?;
        return Ok(());
    }
    let _ = rest;
    Ok(())
}

/// Wrap a `SecretString` clone for storage.
#[must_use]
pub fn secret(s: String) -> SecretString {
    SecretString::from(s)
}

// ---- Messaging (bot token) -----------------------------------------------

/// Fetch recent messages for a channel (`limit` up to 100).
pub async fn channel_messages(
    rest: &DiscordRest,
    token: &SecretString,
    channel_id: &str,
    limit: u32,
) -> Result<Value, RestError> {
    rest.bot_get_query(
        token,
        &format!("/channels/{channel_id}/messages"),
        &format!("limit={}", limit.clamp(1, 100)),
    )
    .await
}

/// Send a text message to a channel.
pub async fn send_message(
    rest: &DiscordRest,
    token: &SecretString,
    channel_id: &str,
    content: &str,
) -> Result<Value, RestError> {
    rest.bot_post(
        token,
        &format!("/channels/{channel_id}/messages"),
        &serde_json::json!({"content": content}),
    )
    .await
}

/// Edit a message's content.
pub async fn edit_message(
    rest: &DiscordRest,
    token: &SecretString,
    channel_id: &str,
    message_id: &str,
    content: &str,
) -> Result<Value, RestError> {
    rest.bot_patch(
        token,
        &format!("/channels/{channel_id}/messages/{message_id}"),
        &serde_json::json!({"content": content}),
    )
    .await
}

/// Delete a message.
pub async fn delete_message(
    rest: &DiscordRest,
    token: &SecretString,
    channel_id: &str,
    message_id: &str,
) -> Result<Value, RestError> {
    rest.bot_delete(
        token,
        &format!("/channels/{channel_id}/messages/{message_id}"),
    )
    .await
}

/// Add (`@me`) or remove the bot's reaction on a message.
pub async fn react(
    rest: &DiscordRest,
    token: &SecretString,
    channel_id: &str,
    message_id: &str,
    emoji: &str,
    add: bool,
) -> Result<Value, RestError> {
    let enc = urlencoding_bot(emoji);
    let path = format!("/channels/{channel_id}/messages/{message_id}/reactions/{enc}/@me");
    if add {
        rest.bot_put(token, &path, &serde_json::json!({})).await
    } else {
        rest.bot_delete(token, &path).await
    }
}

/// Fetch a single message.
pub async fn get_message(
    rest: &DiscordRest,
    token: &SecretString,
    channel_id: &str,
    message_id: &str,
) -> Result<Value, RestError> {
    rest.bot_get(
        token,
        &format!("/channels/{channel_id}/messages/{message_id}"),
    )
    .await
}

/// Trigger the typing indicator in a channel.
pub async fn trigger_typing(
    rest: &DiscordRest,
    token: &SecretString,
    channel_id: &str,
) -> Result<Value, RestError> {
    rest.bot_post(
        token,
        &format!("/channels/{channel_id}/typing"),
        &serde_json::json!({}),
    )
    .await
}

/// Join a voice channel: `PUT /guilds/{guild}/voice-states/{user}`.
pub async fn join_voice(
    rest: &DiscordRest,
    token: &SecretString,
    guild_id: &str,
    user_id: &str,
    channel_id: &str,
) -> Result<Value, RestError> {
    rest.bot_put(
        token,
        &format!("/guilds/{guild_id}/voice-states/{user_id}"),
        &serde_json::json!({"channel_id": channel_id}),
    )
    .await
}

/// Leave voice: `DELETE /guilds/{guild}/voice-states/{user}`.
pub async fn leave_voice(
    rest: &DiscordRest,
    token: &SecretString,
    guild_id: &str,
    user_id: &str,
) -> Result<Value, RestError> {
    rest.bot_delete(token, &format!("/guilds/{guild_id}/voice-states/{user_id}"))
        .await
}

/// Update self-mute/self-deafen while connected to voice.
pub async fn set_voice_flags(
    rest: &DiscordRest,
    token: &SecretString,
    guild_id: &str,
    user_id: &str,
    channel_id: &str,
    self_mute: bool,
    self_deaf: bool,
) -> Result<Value, RestError> {
    rest.bot_put(
        token,
        &format!("/guilds/{guild_id}/voice-states/{user_id}"),
        &serde_json::json!({
            "channel_id": channel_id,
            "self_mute": self_mute,
            "self_deaf": self_deaf,
        }),
    )
    .await
}

/// Fetch voice-server connection info for a guild channel
/// (`GET /guilds/{guild}/voice-server/{channel}`).
pub async fn voice_server_info(
    rest: &DiscordRest,
    token: &SecretString,
    guild_id: &str,
    channel_id: &str,
) -> Result<Value, RestError> {
    rest.bot_get(
        token,
        &format!("/guilds/{guild_id}/voice-server/{channel_id}"),
    )
    .await
}

/// Percent-encode an emoji for use in a reaction path segment.
fn urlencoding_bot(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                use std::fmt::Write as _;
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}
