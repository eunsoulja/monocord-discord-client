//! Shared domain models.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable identifier for a placed widget instance.
pub type WidgetInstanceId = Uuid;

/// Stable identifier for a saved layout.
pub type LayoutId = Uuid;

/// Stable identifier for a notification.
pub type NotificationId = Uuid;

/// Pages reachable from the navigation sidebar and palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Page {
    /// Active Discord server workspace.
    ServerWorkspace,
    /// Channel detail view.
    ChannelInfo,
    /// Browse and manage widgets.
    WidgetLibrary,
    /// Activity timeline.
    Activity,
    /// Notification center.
    Notifications,
    /// Bot integration management.
    BotManagement,
    /// Application settings.
    Settings,
}

impl Page {
    /// All pages, in navigation order.
    pub const ALL: [Page; 7] = [
        Page::ServerWorkspace,
        Page::ChannelInfo,
        Page::WidgetLibrary,
        Page::Activity,
        Page::Notifications,
        Page::BotManagement,
        Page::Settings,
    ];

    /// Human-readable title.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Page::ServerWorkspace => "SERVER WORKSPACE",
            Page::ChannelInfo => "CHANNEL INFO",
            Page::WidgetLibrary => "WIDGET LIBRARY",
            Page::Activity => "ACTIVITY",
            Page::Notifications => "NOTIFICATIONS",
            Page::BotManagement => "BOT MANAGEMENT",
            Page::Settings => "SETTINGS",
        }
    }
}

/// Which surface a widget instance lives on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Surface {
    /// The main overview/workspace grid.
    Main,
    /// The right-side dock.
    Dock,
}

/// Integer grid cell size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridSize {
    /// Width in cells.
    pub w: u32,
    /// Height in cells.
    pub h: u32,
}

impl GridSize {
    /// A size with the given dimensions.
    #[must_use]
    pub const fn new(w: u32, h: u32) -> Self {
        Self { w, h }
    }
}

/// Grid cell position (top-left origin).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridPos {
    /// Column index.
    pub x: u32,
    /// Row index.
    pub y: u32,
}

/// A placed widget on a surface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacedWidget {
    /// Unique instance id.
    pub id: WidgetInstanceId,
    /// Registry id of the widget type (e.g. `l.server_stats`).
    pub widget_type: String,
    /// Display name override (empty = manifest default).
    pub name: String,
    /// Whether the instance is enabled.
    pub enabled: bool,
    /// Grid position.
    pub pos: GridPos,
    /// Grid size.
    pub size: GridSize,
    /// Per-instance configuration (schema depends on widget type).
    pub config: serde_json::Value,
    /// Surface this instance is placed on.
    pub surface: Surface,
}

/// A saved widget layout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layout {
    /// Unique layout id.
    pub id: LayoutId,
    /// Display name.
    pub name: String,
    /// Whether this is the default layout.
    pub is_default: bool,
    /// Built-in preset name, if any (`minimal`, `developer`, ...).
    pub preset: Option<String>,
    /// Widgets on the main surface.
    pub main: Vec<PlacedWidget>,
    /// Widgets on the dock surface.
    pub dock: Vec<PlacedWidget>,
    /// Column count for the main grid.
    pub main_columns: u32,
    /// Column count for the dock grid.
    pub dock_columns: u32,
}

/// Discord account profile obtained via `OAuth2`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordAccount {
    /// Discord user id.
    pub id: String,
    /// Username (discriminator-era handle or new username).
    pub username: String,
    /// Display name / global name.
    pub global_name: Option<String>,
    /// Avatar hash, if any.
    pub avatar: Option<String>,
    /// Granted OAuth scopes.
    pub scopes: Vec<String>,
}

/// A guild the user belongs to (OAuth `guilds` scope data).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildInfo {
    /// Discord guild id.
    pub id: String,
    /// Guild name.
    pub name: String,
    /// Icon hash, if any.
    pub icon: Option<String>,
    /// Whether the user owns this guild.
    pub owner: bool,
    /// Hex permission bitfield the user has.
    pub permissions: String,
    /// Whether the bot is present in this guild.
    pub bot_present: bool,
    /// Approximate member count when known.
    pub member_count: Option<u64>,
    /// Approximate online count when known.
    pub online_count: Option<u64>,
}

/// A Discord channel entry (text/voice/category) for navigation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelEntry {
    /// Channel id.
    pub id: String,
    /// Channel name.
    pub name: String,
    /// Parent guild id.
    pub guild_id: String,
    /// Channel type: 0 = text/guild text, 2 = voice, 4 = category.
    pub kind: u8,
}

/// A file attachment on a chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatAttachment {
    /// CDN URL.
    pub url: String,
    /// Original filename.
    pub filename: String,
    /// MIME type when known.
    pub content_type: Option<String>,
}

/// A reaction aggregate on a chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageReaction {
    /// Emoji identifier (`name` or `name:id` for custom emoji).
    pub emoji: String,
    /// Total reaction count.
    pub count: u64,
    /// Whether the bot account reacted.
    pub me: bool,
}

/// A Discord chat message visible to the bot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Message snowflake id.
    pub id: String,
    /// Channel the message belongs to.
    pub channel_id: String,
    /// Parent guild id (empty for DMs).
    pub guild_id: String,
    /// Author user id.
    pub author_id: String,
    /// Author display name (global name or username).
    pub author_name: String,
    /// Message text content.
    pub content: String,
    /// Unix timestamp seconds (derived from the snowflake).
    pub timestamp: i64,
    /// Whether the message was edited.
    pub edited: bool,
    /// File attachments.
    pub attachments: Vec<ChatAttachment>,
    /// Reaction aggregates.
    pub reactions: Vec<MessageReaction>,
    /// Whether the author is the bot account itself.
    pub is_self: bool,
}

/// Bot voice connection state (songbird driver).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceInfo {
    /// Guild the voice channel belongs to.
    pub guild_id: String,
    /// Voice channel the bot is connected to.
    pub channel_id: String,
    /// Voice channel display name when known.
    pub channel_name: String,
    /// Fully connected to the voice gateway.
    pub connected: bool,
    /// Join/handshake in progress.
    pub connecting: bool,
    /// `self_mute` flag.
    pub muted: bool,
    /// `self_deaf` flag.
    pub deafened: bool,
    /// Last error, if the join failed.
    pub error: Option<String>,
}

/// Bot integration status.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BotStatus {
    /// No bot token configured.
    #[default]
    NotConfigured,
    /// Token saved, not yet verified.
    Configured,
    /// Bot authenticated successfully.
    Online,
    /// Bot token rejected or error.
    Error(String),
}

/// A notification stored locally.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    /// Unique id.
    pub id: NotificationId,
    /// Source integration (e.g. `discord`, `system`, `widget`).
    pub source: String,
    /// Category for filtering.
    pub category: String,
    /// Title.
    pub title: String,
    /// Body text.
    pub body: String,
    /// Unix timestamp (seconds).
    pub created_at: i64,
    /// Whether it has been read.
    pub read: bool,
}

/// A local personal note.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    /// Unique id.
    pub id: uuid::Uuid,
    /// Note title.
    pub title: String,
    /// Markdown-ish body (plain text).
    pub body: String,
    /// Workspace scope (`None` = global).
    pub workspace: Option<String>,
    /// Last modified unix seconds.
    pub updated_at: i64,
}

/// Connection status of the Discord integration as a whole.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    /// Never attempted / signed out.
    #[default]
    Disconnected,
    /// OAuth flow in progress.
    Connecting,
    /// Signed in via `OAuth2`.
    Connected,
    /// Signed in but offline / API unreachable.
    Degraded(String),
    /// Demo mode active.
    Demo,
}
