//! Application state, split into UI / persistent / integration concerns.

use serde::{Deserialize, Serialize};

use crate::models::{
    BotStatus, ChannelEntry, ChatMessage, ConnectionStatus, DiscordAccount, GuildInfo, Layout,
    LayoutId, Note, Notification, Page, VoiceInfo, WidgetInstanceId,
};

/// Animation intensity setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AnimationIntensity {
    /// All transitions instant.
    Off,
    /// Subtle, fast transitions.
    #[default]
    Subtle,
    /// Full transitions.
    Full,
}

/// Font choices (bundled families).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FontChoice {
    /// `JetBrains` Mono (default).
    JetBrainsMono,
    /// System monospace fallback.
    SystemMono,
}

/// Theme preset (mirrors `l_ui::theme::ThemePreset`; kept in core so prefs
/// stay UI-framework agnostic).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeChoice {
    /// Monochrome terminal theme.
    #[default]
    Mono,
    /// Discord-inspired dark theme.
    DiscordDark,
    /// Discord-inspired light theme.
    Light,
    /// Pure-black OLED theme.
    Amoled,
}

/// A configurable keyboard shortcut binding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keybind {
    /// Action id from the app's keybind catalog (e.g. `palette`).
    pub action: String,
    /// Combo string like `Ctrl+K` or `Alt+1`.
    pub combo: String,
}

fn default_true() -> bool {
    true
}

/// Persistent user preferences (single source of truth: SQLite).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preferences {
    /// UI scale factor percent (80–150).
    pub ui_scale: f32,
    /// Base font size in points.
    pub font_size: f32,
    /// Font family.
    pub font: FontChoice,
    /// Compact density mode.
    pub compact: bool,
    /// Show server rail.
    pub show_server_rail: bool,
    /// Show navigation sidebar.
    pub show_nav_sidebar: bool,
    /// Widget dock visible.
    pub dock_visible: bool,
    /// Widget dock width in logical pixels.
    pub dock_width: f32,
    /// Animation intensity.
    pub animation: AnimationIntensity,
    /// Workspace display name.
    pub workspace_name: String,
    /// Default layout id (None = first).
    pub default_layout: Option<LayoutId>,
    /// Show seconds in the header clock.
    pub clock_seconds: bool,
    /// OS-level notifications enabled.
    pub os_notifications: bool,
    /// Notification categories enabled.
    pub notify_categories: Vec<String>,
    /// Discord `OAuth2` client id (public; overridable).
    pub discord_client_id: Option<String>,
    /// Discord OAuth redirect port.
    pub discord_redirect_port: u16,
    /// Optional full redirect URI override (must match the Developer Portal
    /// registration exactly, e.g. `http://localhost:8471/callback`).
    /// `None` = `http://127.0.0.1:{port}/callback`.
    #[serde(default)]
    pub discord_redirect_uri: Option<String>,
    /// Demo mode: use sample data provider, never call Discord.
    pub demo_mode: bool,
    /// Gateway (bot websocket) enabled.
    pub gateway_enabled: bool,
    /// Privileged intent: guild members.
    pub intent_guild_members: bool,
    /// Privileged intent: presences.
    pub intent_presences: bool,
    /// Privileged intent: message content.
    pub intent_message_content: bool,
    /// Theme preset.
    #[serde(default)]
    pub theme: ThemeChoice,
    /// Custom accent color as `0xRRGGBB` (None = preset accent).
    #[serde(default)]
    pub accent_color: Option<u32>,
    /// Local Discord RPC server enabled (rich presence for games).
    #[serde(default = "default_true")]
    pub rpc_enabled: bool,
    /// User-defined keybind overrides (empty = built-in defaults).
    #[serde(default)]
    pub keybinds: Vec<Keybind>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            ui_scale: 100.0,
            font_size: 13.0,
            font: FontChoice::JetBrainsMono,
            compact: false,
            show_server_rail: true,
            show_nav_sidebar: true,
            dock_visible: true,
            dock_width: 340.0,
            animation: AnimationIntensity::Subtle,
            workspace_name: "WORKSPACE".to_string(),
            default_layout: None,
            clock_seconds: true,
            os_notifications: false,
            notify_categories: vec!["discord".into(), "system".into(), "widget".into()],
            discord_client_id: None,
            discord_redirect_port: 8471,
            discord_redirect_uri: None,
            demo_mode: true,
            gateway_enabled: false,
            intent_guild_members: false,
            intent_presences: false,
            intent_message_content: false,
            theme: ThemeChoice::Mono,
            accent_color: None,
            rpc_enabled: true,
            keybinds: Vec::new(),
        }
    }
}

/// Ephemeral UI state — never persisted.
#[derive(Debug, Clone, Default)]
pub struct UiState {
    /// Currently selected page.
    pub page: Option<Page>,
    /// Selected guild id for server workspace.
    pub selected_guild: Option<String>,
    /// Selected channel id.
    pub selected_channel: Option<String>,
    /// Command palette open.
    pub palette_open: bool,
    /// Settings section tab.
    pub settings_tab: SettingsTab,
    /// Widget currently being configured.
    pub configuring_widget: Option<WidgetInstanceId>,
    /// Layout editor mode active.
    pub layout_editing: bool,
    /// Widget being dragged in the layout editor.
    pub drag_widget: Option<WidgetInstanceId>,
    /// Widget focused for keyboard move/resize.
    pub focused_widget: Option<WidgetInstanceId>,
    /// Toast messages (transient).
    pub toasts: Vec<Toast>,
    /// Discord setup dialog open.
    pub discord_setup_open: bool,
    /// Join-server invite dialog open.
    pub join_invite_open: bool,
    /// Add-integration dialog open.
    pub add_integration_open: bool,
    /// Import dialog state.
    pub import_dialog: Option<ImportDialogState>,
    /// Confirm dialog.
    pub confirm: Option<ConfirmDialog>,
    /// Keybind id currently being recorded in settings (None = not recording).
    pub keybind_capture: Option<String>,
    /// Collapsed channel categories in nav sidebar.
    pub collapsed_categories: Vec<String>,
    /// Chat composer draft text.
    pub chat_draft: String,
    /// Message id being edited in the composer, if any.
    pub chat_editing: Option<String>,
}

/// Settings page sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    /// Theme, fonts, density.
    #[default]
    Appearance,
    /// Workspace name, layouts.
    Workspace,
    /// Discord connection and bot.
    Discord,
    /// Notification preferences.
    Notifications,
    /// External plugins.
    Plugins,
    /// Advanced / debug.
    Advanced,
}

/// A transient toast notification.
#[derive(Debug, Clone)]
pub struct Toast {
    /// Toast text.
    pub text: String,
    /// Severity.
    pub kind: ToastKind,
    /// Creation time (egui frame count proxy handled by app).
    pub created: std::time::Instant,
}

/// Toast severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    /// Informational.
    Info,
    /// Success.
    Success,
    /// Warning.
    Warning,
    /// Error.
    Error,
}

/// Import dialog state.
#[derive(Debug, Clone, Default)]
pub struct ImportDialogState {
    /// Parsed widgets awaiting confirmation.
    pub pending: Vec<serde_json::Value>,
    /// Error message if parse failed.
    pub error: Option<String>,
}

/// A confirmation dialog request.
#[derive(Debug, Clone)]
pub struct ConfirmDialog {
    /// Dialog title.
    pub title: String,
    /// Body text.
    pub body: String,
    /// Action to dispatch on confirm.
    pub on_confirm: crate::actions::Action,
    /// Destructive styling.
    pub destructive: bool,
}

/// Discord integration runtime state.
#[derive(Debug, Clone, Default)]
pub struct DiscordState {
    /// Overall connection status.
    pub status: ConnectionStatus,
    /// OAuth account, if connected.
    pub account: Option<DiscordAccount>,
    /// Guilds visible via OAuth.
    pub guilds: Vec<GuildInfo>,
    /// Bot status.
    pub bot_status: BotStatus,
    /// Bot user id when online.
    pub bot_user_id: Option<String>,
    /// Bot display name.
    pub bot_username: Option<String>,
    /// Gateway/websocket state description.
    pub gateway_state: Option<String>,
    /// Approximate bot latency ms.
    pub bot_latency_ms: Option<u64>,
    /// Granted scopes shown in settings.
    pub granted_scopes: Vec<String>,
    /// Channels of the selected guild (for nav + channel page).
    pub channels: Vec<ChannelEntry>,
    /// Chat messages per channel id (newest last, capped).
    pub messages: std::collections::HashMap<String, Vec<ChatMessage>>,
    /// Typing indicators: channel id -> (username, unix secs).
    pub typing: std::collections::HashMap<String, Vec<(String, i64)>>,
    /// Bot voice connection state.
    pub voice: Option<VoiceInfo>,
}

/// Persistent workspace state loaded from storage.
#[derive(Debug, Clone, Default)]
pub struct WorkspaceState {
    /// All saved layouts.
    pub layouts: Vec<Layout>,
    /// Currently active layout id.
    pub active_layout: Option<LayoutId>,
    /// Personal notes.
    pub notes: Vec<Note>,
    /// Notification history (most recent last).
    pub notifications: Vec<Notification>,
    /// Audit log entries (newest last, capped).
    pub audit_log: Vec<AuditEntry>,
}

/// An audit log entry for moderation/management actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Unix seconds.
    pub at: i64,
    /// Actor (`user`, `bot`, `system`).
    pub actor: String,
    /// Action description.
    pub action: String,
    /// Target description.
    pub target: String,
    /// Whether it succeeded.
    pub ok: bool,
}

/// Root application state.
#[derive(Debug)]
pub struct AppState {
    /// Persistent preferences.
    pub prefs: Preferences,
    /// Ephemeral UI state.
    pub ui: UiState,
    /// Discord integration state.
    pub discord: DiscordState,
    /// Workspace persistent state.
    pub workspace: WorkspaceState,
    /// Widget runtime data keyed by `WidgetInstanceId` -> type-erased JSON snapshots.
    pub widget_data: std::collections::HashMap<WidgetInstanceId, serde_json::Value>,
    /// Widget runtime states keyed by instance id.
    pub widget_states: std::collections::HashMap<WidgetInstanceId, WidgetRuntimeState>,
    /// Whether initial data has been loaded from storage.
    pub hydrated: bool,
    /// Demo mode banner is implied by `prefs.demo_mode`.
    pub last_frame: std::time::Instant,
}

/// Per-widget-instance runtime state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetRuntimeState {
    /// Waiting for first data.
    Loading,
    /// Data available.
    Ready,
    /// No data to show.
    Empty,
    /// Last fetch failed.
    Error,
    /// Integration disconnected.
    Disconnected,
    /// Missing required permission/scope.
    PermissionMissing,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            prefs: Preferences::default(),
            ui: UiState::default(),
            discord: DiscordState::default(),
            workspace: WorkspaceState::default(),
            widget_data: std::collections::HashMap::default(),
            widget_states: std::collections::HashMap::default(),
            hydrated: false,
            last_frame: std::time::Instant::now(),
        }
    }
}
