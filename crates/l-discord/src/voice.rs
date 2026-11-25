//! Bot voice connectivity via songbird's standalone driver.
//!
//! The driver needs a `ConnectionInfo` assembled from two gateway dispatches:
//! the bot's own `VOICE_STATE_UPDATE` (session id) and `VOICE_SERVER_UPDATE`
//! (endpoint + token). This module owns the `Driver` and the pending-join
//! handshake state; the app layer feeds raw gateway payloads in and polls
//! for completion.

use std::num::NonZeroU64;
use std::sync::Mutex as StdMutex;

use serde_json::Value;
use songbird::driver::Driver;
use songbird::{Config, ConnectionInfo};

/// Non-zero u64 id wrapper used by songbird.
fn nz(id: &str) -> Result<NonZeroU64, String> {
    id.parse::<u64>()
        .ok()
        .and_then(NonZeroU64::new)
        .ok_or_else(|| format!("bad discord id: {id}"))
}

/// State of the bot's voice connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoicePhase {
    /// Not connected.
    Disconnected,
    /// REST join done; waiting for voice gateway handshake.
    Connecting,
    /// Driver connected to the voice websocket + UDP.
    Connected,
    /// Last join/connect attempt failed.
    Failed(String),
}

/// A pending join waiting for `VOICE_STATE_UPDATE` + `VOICE_SERVER_UPDATE`.
#[derive(Debug, Default)]
struct PendingJoin {
    guild_id: String,
    channel_id: String,
    /// `session_id` from the bot's `VOICE_STATE_UPDATE`.
    session_id: Option<String>,
    /// (endpoint, token) from `VOICE_SERVER_UPDATE`.
    server: Option<(String, String)>,
}

/// Owns the songbird driver and the join handshake.
pub struct VoiceRuntime {
    driver: Option<Driver>,
    /// guild/channel the bot is connected (or connecting) to.
    pub guild_id: String,
    pub channel_id: String,
    /// `self_mute` / `self_deaf` flags mirrored for the UI.
    pub muted: bool,
    pub deafened: bool,
    /// Bot user id (snowflake) needed for `ConnectionInfo`.
    pub bot_user_id: Option<String>,
    phase: VoicePhase,
    pending: Option<PendingJoin>,
}

impl Default for VoiceRuntime {
    fn default() -> Self {
        Self {
            driver: None,
            guild_id: String::new(),
            channel_id: String::new(),
            muted: false,
            deafened: false,
            bot_user_id: None,
            phase: VoicePhase::Disconnected,
            pending: None,
        }
    }
}

impl VoiceRuntime {
    /// Current connection phase.
    #[must_use]
    pub fn phase(&self) -> VoicePhase {
        self.phase.clone()
    }

    /// Begin a join: records what we're waiting for. REST join must be done
    /// by the caller (it triggers the gateway events).
    pub fn begin_join(&mut self, guild_id: &str, channel_id: &str) {
        self.guild_id = guild_id.to_string();
        self.channel_id = channel_id.to_string();
        self.phase = VoicePhase::Connecting;
        self.pending = Some(PendingJoin {
            guild_id: guild_id.to_string(),
            channel_id: channel_id.to_string(),
            session_id: None,
            server: None,
        });
    }

    /// Feed a raw gateway dispatch payload (`VOICE_STATE_UPDATE` or
    /// `VOICE_SERVER_UPDATE`). Returns `Some(info)` once both halves of the
    /// handshake have arrived for the pending join.
    pub fn feed_gateway(&mut self, event: &str, data: &Value) -> Option<ConnectionInfo> {
        let pending = self.pending.as_mut()?;
        match event {
            "VOICE_STATE_UPDATE" => {
                // Only the bot's own voice state carries our session id.
                let gid = data.get("guild_id").and_then(Value::as_str).unwrap_or("");
                let cid = data.get("channel_id").and_then(Value::as_str).unwrap_or("");
                if gid != pending.guild_id || cid != pending.channel_id {
                    return None;
                }
                if let Some(sid) = data.get("session_id").and_then(Value::as_str) {
                    pending.session_id = Some(sid.to_string());
                }
            }
            "VOICE_SERVER_UPDATE" => {
                let gid = data.get("guild_id").and_then(Value::as_str).unwrap_or("");
                if gid != pending.guild_id {
                    return None;
                }
                let endpoint = data.get("endpoint").and_then(Value::as_str)?;
                let token = data.get("token").and_then(Value::as_str)?;
                pending.server = Some((endpoint.to_string(), token.to_string()));
            }
            _ => return None,
        }
        // Only consume the pending join once both halves arrived.
        let ready = self
            .pending
            .as_ref()
            .is_some_and(|p| p.session_id.is_some() && p.server.is_some());
        if !ready {
            return None;
        }
        let pending = self.pending.take()?;
        let (endpoint, token) = pending.server?;
        let session_id = pending.session_id?;
        let user_id = self.bot_user_id.clone()?;
        Some(ConnectionInfo {
            channel_id: songbird::id::ChannelId(nz(&pending.channel_id).ok()?),
            guild_id: songbird::id::GuildId(nz(&pending.guild_id).ok()?),
            user_id: songbird::id::UserId(nz(&user_id).ok()?),
            session_id,
            endpoint,
            token,
        })
    }

    /// Start connecting the driver; returns a future that resolves when the
    /// voice handshake completes. Call `mark_connected`/`fail` afterwards.
    /// The returned future owns no borrow of `self`, so it can be awaited
    /// outside the runtime lock.
    pub fn start_connect(
        &mut self,
        info: ConnectionInfo,
    ) -> Result<songbird::driver::Connect, String> {
        if self.driver.is_none() {
            self.driver = Some(Driver::new(Config::default()));
        }
        Ok(self.driver.as_mut().expect("driver").connect(info))
    }

    /// Mark the connection as established.
    pub fn mark_connected(&mut self) {
        self.phase = VoicePhase::Connected;
    }

    /// Whether a join handshake is in flight.
    #[must_use]
    pub fn is_connecting(&self) -> bool {
        self.phase == VoicePhase::Connecting
    }

    /// Leave the current voice channel (if any).
    pub fn disconnect(&mut self) {
        if let Some(d) = &mut self.driver {
            d.leave();
        }
        self.phase = VoicePhase::Disconnected;
        self.pending = None;
        self.guild_id.clear();
        self.channel_id.clear();
        self.muted = false;
        self.deafened = false;
    }

    /// Set local mute on the driver.
    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        if let Some(d) = &mut self.driver {
            d.mute(muted);
        }
    }

    /// Mark join as failed.
    pub fn fail(&mut self, error: String) {
        self.phase = VoicePhase::Failed(error);
        self.pending = None;
    }
}

/// Thread-safe wrapper used by the backend.
pub type SharedVoice = std::sync::Arc<StdMutex<VoiceRuntime>>;

/// Create a shared voice runtime.
#[must_use]
pub fn shared() -> SharedVoice {
    std::sync::Arc::new(StdMutex::new(VoiceRuntime::default()))
}
