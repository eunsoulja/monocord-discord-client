//! Capability model: what the app can actually do right now.
//!
//! Computed from OAuth scopes, bot presence per guild, enabled intents, and
//! Gateway state. Widgets and pages consult this before showing data or
//! enabling actions — never display fabricated data.

use serde::{Deserialize, Serialize};

/// Snapshot of granted capabilities.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Capabilities {
    /// OAuth `identify` granted (user profile + avatar).
    pub identify: bool,
    /// OAuth `guilds` granted (guild list).
    pub guilds: bool,
    /// Bot token configured.
    pub bot_configured: bool,
    /// Bot token verified online.
    pub bot_online: bool,
    /// Guild ids where the bot is present.
    pub bot_guilds: Vec<String>,
    /// Guild-members privileged intent enabled.
    pub intent_members: bool,
    /// Presences privileged intent enabled.
    pub intent_presences: bool,
    /// Message-content privileged intent enabled.
    pub intent_message_content: bool,
    /// Gateway websocket session active.
    pub gateway_connected: bool,
    /// Voice-states intent available (non-privileged, needs gateway).
    pub voice_states: bool,
    /// Demo mode active — data is sample data.
    pub demo: bool,
}

impl Capabilities {
    /// Whether we can show stats for `guild_id` (bot present or demo).
    #[must_use]
    pub fn can_guild_stats(&self, guild_id: &str) -> bool {
        self.demo || self.bot_guilds.iter().any(|g| g == guild_id) || guild_id == "@selected"
    }

    /// Whether member join/leave events can be shown.
    #[must_use]
    pub fn can_member_activity(&self) -> bool {
        self.demo || (self.bot_online && self.intent_members && self.gateway_connected)
    }

    /// Whether voice participant data can be shown.
    #[must_use]
    pub fn can_voice(&self) -> bool {
        self.demo || (self.bot_online && self.gateway_connected && self.voice_states)
    }

    /// Whether bot latency is measurable.
    #[must_use]
    pub fn can_bot_latency(&self) -> bool {
        self.bot_online && self.gateway_connected
    }

    /// Human-readable missing-permission explanation for a feature.
    #[must_use]
    pub fn explain(&self, feature: &str) -> Option<String> {
        match feature {
            "guild_stats" if !self.guilds && !self.demo => {
                Some("requires Discord sign-in (guilds scope)".into())
            }
            "member_activity" if !self.can_member_activity() => {
                Some("requires bot Gateway with the privileged guild-members intent".into())
            }
            "voice" if !self.can_voice() => {
                Some("requires bot Gateway session with voice-states intent".into())
            }
            "bot_monitor" if !self.bot_configured => {
                Some("requires a bot token in Settings → Discord".into())
            }
            _ => None,
        }
    }
}

/// Build capabilities from raw inputs.
#[must_use]
pub fn compute(
    scopes: &[String],
    bot_status: &str,
    bot_guilds: Vec<String>,
    intents: (bool, bool, bool),
    gateway_connected: bool,
    demo: bool,
) -> Capabilities {
    Capabilities {
        identify: scopes.iter().any(|s| s == "identify"),
        guilds: scopes.iter().any(|s| s == "guilds"),
        bot_configured: bot_status != "not_configured",
        bot_online: bot_status == "online",
        bot_guilds,
        intent_members: intents.0,
        intent_presences: intents.1,
        intent_message_content: intents.2,
        gateway_connected,
        voice_states: true, // voice-states is non-privileged
        demo,
    }
}
