//! Side effects the reducer asks services to perform.

use crate::models::{
    Layout, LayoutId, Note, Notification, NotificationId, PlacedWidget, VoiceInfo, WidgetInstanceId,
};
use crate::state::{AuditEntry, Preferences};

/// Effects executed by the app/runtime layer.
#[derive(Debug, Clone)]
pub enum Effect {
    /// Persist the full preferences blob.
    SavePreferences(Preferences),
    /// Persist workspace state (layouts, notes, notifications, audit).
    SaveWorkspace,
    /// Persist a single layout.
    SaveLayout(Layout),
    /// Delete a layout row.
    DeleteLayoutRow(LayoutId),
    /// Initial hydration request (load everything).
    Hydrate,
    /// Start Discord OAuth flow.
    StartOAuth {
        /// Client id.
        client_id: String,
        /// Redirect port.
        port: u16,
        /// Optional redirect URI override (must match Developer Portal).
        redirect_uri: Option<String>,
    },
    /// Open (or focus) the embedded Discord web client window.
    OpenDiscordClient,
    /// Restart the embedded Discord web client (re-reads mod files).
    RestartDiscordClient,
    /// Cancel OAuth listener.
    CancelOAuth,
    /// Disconnect: revoke token, clear keychain entries.
    DisconnectDiscord,
    /// Store bot token in keychain (write-only).
    StoreBotToken(String),
    /// Delete bot token from keychain.
    DeleteBotToken,
    /// Verify bot token via Discord `/users/@me`.
    VerifyBot,
    /// Fetch Discord account + guilds after OAuth.
    FetchDiscordData,
    /// Perform a Discord REST call described by the runtime.
    DiscordRest(DiscordRestRequest),
    /// Join a guild via an invite link/code (user OAuth token required).
    JoinInvite {
        /// Raw user input: invite URL or bare code.
        input: String,
    },
    /// Fetch message history for a channel (bot token).
    FetchMessages {
        /// Channel id.
        channel_id: String,
    },
    /// Send a chat message as the bot.
    ChatSend {
        /// Channel id.
        channel_id: String,
        /// Message text.
        content: String,
    },
    /// Edit a bot message.
    ChatEdit {
        /// Channel id.
        channel_id: String,
        /// Message id.
        message_id: String,
        /// New text.
        content: String,
    },
    /// Delete a message.
    ChatDelete {
        /// Channel id.
        channel_id: String,
        /// Message id.
        message_id: String,
    },
    /// Toggle the bot's reaction on a message.
    ChatReact {
        /// Channel id.
        channel_id: String,
        /// Message id.
        message_id: String,
        /// Emoji.
        emoji: String,
        /// Add vs remove.
        add: bool,
    },
    /// Trigger the typing indicator.
    ChatTyping {
        /// Channel id.
        channel_id: String,
    },
    /// Connect the bot to a voice channel.
    VoiceJoin {
        /// Guild id.
        guild_id: String,
        /// Voice channel id.
        channel_id: String,
    },
    /// Disconnect the bot from voice.
    VoiceLeave,
    /// Update voice self-mute/self-deafen.
    VoiceFlags {
        /// New mute value.
        muted: Option<bool>,
        /// New deafen value.
        deafened: Option<bool>,
    },
    /// Push the current voice state to the UI.
    VoiceStatus(VoiceInfo),
    /// Push an OS notification (if enabled).
    OsNotification {
        /// Title.
        title: String,
        /// Body.
        body: String,
    },
    /// Open a URL in the system browser (https only, validated).
    OpenUrl(String),
    /// Show a native file-open dialog for import; result returns as `AppEvent::FileChosen`.
    PickImportFile,
    /// Show a native file-save dialog for export.
    PickExportFile,
    /// Write export payload to chosen path (app layer handles dialog result).
    WriteExportFile {
        /// Suggested file name.
        name: String,
        /// JSON contents.
        contents: String,
    },
    /// Request widget runtime to (re)subscribe data needs for all placed widgets.
    SyncWidgetSubscriptions,
    /// Spawn/refresh polling for a specific widget instance.
    RefreshWidget(WidgetInstanceId),
    /// Persist a single widget instance config.
    SaveWidgetInstance(PlacedWidget),
    /// Delete widget instance row.
    DeleteWidgetInstance(WidgetInstanceId),
    /// Save a note.
    SaveNote(Note),
    /// Delete a note.
    DeleteNoteRow(uuid::Uuid),
    /// Save a notification.
    SaveNotification(Notification),
    /// Delete a notification row.
    DeleteNotificationRow(NotificationId),
    /// Clear all notifications.
    ClearNotificationsRow,
    /// Push a transient toast message.
    ServiceToast {
        /// Message text.
        text: String,
        /// Severity word (`info`, `success`, `warning`, `error`).
        kind: String,
    },
    /// Append audit entry.
    AppendAudit(AuditEntry),
    /// Invoke a plugin command (JSON-RPC `command` method).
    PluginInvoke {
        /// Plugin id.
        plugin: String,
        /// Command id.
        command: String,
    },
}

/// A typed Discord REST request the runtime should execute.
#[derive(Debug, Clone)]
pub enum DiscordRestRequest {
    /// GET a Discord API v10 path (e.g. `/users/@me/guilds`).
    Get(String),
    /// GET with query already encoded.
    GetQuery {
        /// Path.
        path: String,
        /// Raw query string.
        query: String,
    },
    /// Bot-authenticated GET.
    BotGet(String),
    /// Bot-authenticated PATCH/POST JSON.
    BotPost {
        /// Path.
        path: String,
        /// JSON body.
        body: serde_json::Value,
    },
}
