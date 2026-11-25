//! Typed UI actions dispatched into the reducer.

use crate::events::AppEvent;
use crate::models::{GridPos, GridSize, LayoutId, NotificationId, Page, Surface, WidgetInstanceId};
use crate::state::{
    AuditEntry, ConfirmDialog, Preferences, SettingsTab, ToastKind, WidgetRuntimeState,
};
use serde_json::Value;

/// Every user or system intent that mutates state.
#[derive(Debug, Clone)]
pub enum Action {
    /// A background service event arrived.
    Event(AppEvent),
    /// Navigate to a page.
    Navigate(Page),
    /// Toggle the command palette.
    TogglePalette,
    /// Close the command palette.
    ClosePalette,
    /// Set the active guild.
    SelectGuild(Option<String>),
    /// Set the active channel.
    SelectChannel(Option<String>),
    /// Load message history for a channel (bot token required).
    LoadChannelMessages(String),
    /// Send a chat message as the bot.
    SendChatMessage {
        /// Channel id.
        channel_id: String,
        /// Message text.
        content: String,
    },
    /// Edit one of the bot's messages.
    EditChatMessage {
        /// Channel id.
        channel_id: String,
        /// Message id.
        message_id: String,
        /// New text.
        content: String,
    },
    /// Delete a message (bot's own, or with Manage Messages).
    DeleteChatMessage {
        /// Channel id.
        channel_id: String,
        /// Message id.
        message_id: String,
    },
    /// Add or remove the bot's reaction on a message.
    ReactChatMessage {
        /// Channel id.
        channel_id: String,
        /// Message id.
        message_id: String,
        /// Emoji (unicode or `name:id` custom).
        emoji: String,
        /// True = add, false = remove.
        add: bool,
    },
    /// Trigger the typing indicator in a channel.
    SendTyping {
        /// Channel id.
        channel_id: String,
    },
    /// Connect the bot to a voice channel.
    JoinVoiceChannel {
        /// Guild id.
        guild_id: String,
        /// Voice channel id.
        channel_id: String,
    },
    /// Disconnect the bot from voice.
    LeaveVoiceChannel,
    /// Update bot voice self-mute/self-deafen (None = unchanged).
    VoiceSetFlags {
        /// New `self_mute` value.
        muted: Option<bool>,
        /// New `self_deaf` value.
        deafened: Option<bool>,
    },
    /// Toggle a nav category collapse.
    ToggleCategory(String),
    /// Update preferences (merged by caller into a full new value where needed).
    UpdatePrefs(Box<Preferences>),
    /// Change settings tab.
    SettingsTab(SettingsTab),
    /// Navigate to Settings and select a tab.
    NavigateSettingsTab(SettingsTab),
    /// Toggle dock visibility.
    ToggleDock,
    /// Toggle a sidebar.
    ToggleSidebar(SidebarKind),
    /// Open widget configuration dialog.
    ConfigureWidget(WidgetInstanceId),
    /// Close widget configuration dialog.
    CloseWidgetConfig,
    /// Set widget config value (key path handled by widget).
    SetWidgetConfig {
        /// Instance id.
        id: WidgetInstanceId,
        /// Config key.
        key: String,
        /// New JSON value.
        value: Value,
    },
    /// Reset widget config to defaults.
    ResetWidgetConfig(WidgetInstanceId),
    /// Remove a widget instance.
    RemoveWidget(WidgetInstanceId),
    /// Add a widget instance of a type (returns via effect; id assigned by manager).
    AddWidget {
        /// Registry id.
        widget_type: String,
        /// Target surface.
        surface: Surface,
    },
    /// Toggle layout editing mode.
    ToggleLayoutEditing,
    /// Begin dragging a widget.
    DragStart(WidgetInstanceId),
    /// Drag a widget to a grid cell.
    DragTo {
        /// Instance id.
        id: WidgetInstanceId,
        /// Target cell.
        pos: GridPos,
        /// Surface.
        surface: Surface,
    },
    /// Resize a widget.
    ResizeWidget {
        /// Instance id.
        id: WidgetInstanceId,
        /// New size.
        size: GridSize,
        /// Surface.
        surface: Surface,
    },
    /// Focus a widget for keyboard move/resize.
    FocusWidget(Option<WidgetInstanceId>),
    /// Keyboard nudge for focused widget.
    NudgeWidget {
        /// Instance id.
        id: WidgetInstanceId,
        /// Delta x.
        dx: i32,
        /// Delta y.
        dy: i32,
        /// Delta w.
        dw: i32,
        /// Delta h.
        dh: i32,
        /// Surface.
        surface: Surface,
    },
    /// Switch active layout.
    SelectLayout(LayoutId),
    /// Rename a layout.
    RenameLayout {
        /// Layout id.
        id: LayoutId,
        /// New name.
        name: String,
    },
    /// Delete a layout.
    DeleteLayout(LayoutId),
    /// Save current widgets as a new layout.
    SaveLayoutAs(String),
    /// Apply a preset layout.
    ApplyPreset(String),
    /// Create a new note.
    NewNote,
    /// Update a note field.
    UpdateNote {
        /// Note id.
        id: uuid::Uuid,
        /// Field to set.
        field: NoteField,
        /// New text.
        text: String,
    },
    /// Delete a note.
    DeleteNote(uuid::Uuid),
    /// Mark notification read/unread.
    SetNotificationRead {
        /// Notification id.
        id: NotificationId,
        /// Read flag.
        read: bool,
    },
    /// Mark all notifications read.
    MarkAllNotificationsRead,
    /// Clear all notifications.
    ClearNotifications,
    /// Delete one notification.
    DeleteNotification(NotificationId),
    /// Push a toast.
    Toast {
        /// Message.
        text: String,
        /// Severity.
        kind: ToastKind,
    },
    /// Dismiss a toast by index.
    DismissToast(usize),
    /// Show a confirm dialog.
    Confirm(Box<ConfirmDialog>),
    /// Dismiss the confirm dialog (confirm or cancel handled via `on_confirm` action).
    DismissConfirm,
    /// Open/close Discord setup dialog.
    SetDiscordSetupOpen(bool),
    /// Open/close the join-server (invite link) dialog.
    SetJoinInviteOpen(bool),
    /// Join a Discord server from an invite link or code.
    JoinGuildInvite(String),
    /// Open/close add-integration dialog.
    SetAddIntegrationOpen(bool),
    /// Begin Discord OAuth flow.
    StartDiscordOAuth,
    /// Open (or focus) the embedded Discord web client window.
    OpenDiscordClient,
    /// Restart the embedded Discord web client (re-reads mod files).
    RestartDiscordClient,
    /// Cancel in-flight OAuth.
    CancelDiscordOAuth,
    /// Disconnect Discord account.
    DisconnectDiscord,
    /// Set bot token (write-only; goes to keychain via effect).
    SetBotToken(String),
    /// Re-verify the configured bot token against Discord.
    VerifyBot,
    /// Remove bot token.
    RemoveBotToken,
    /// Set Discord client id.
    SetDiscordClientId(String),
    /// Import widget config JSON (validated).
    ImportWidgetConfigs(String),
    /// Export widget configs requested (triggers file dialog via effect).
    ExportWidgetConfigs,
    /// Widget data snapshot arrived for an instance.
    WidgetData {
        /// Instance id.
        id: WidgetInstanceId,
        /// JSON payload.
        data: Value,
    },
    /// Widget runtime state changed.
    WidgetState {
        /// Instance id.
        id: WidgetInstanceId,
        /// New state.
        state: WidgetRuntimeState,
    },
    /// Refresh a widget now.
    RefreshWidget(WidgetInstanceId),
    /// Quick command / palette command executed.
    RunCommand(String),
    /// Invoke a plugin-provided palette command.
    PluginCommand {
        /// Plugin id.
        plugin: String,
        /// Command id within the plugin.
        command: String,
    },
    /// Toggle demo mode.
    ToggleDemoMode,
    /// Reset all settings to defaults.
    ResetSettings,
    /// Open a validated https URL in the system browser.
    OpenUrl(String),
    /// Request OS notification for a stored notification.
    OsNotify(NotificationId),
    /// Add audit entry.
    Audit(AuditEntry),
    /// Widget instance removed confirmation handled by manager (no-op here).
    WidgetRemoved(WidgetInstanceId),
    /// Toggle widget enabled state.
    ToggleWidgetEnabled(WidgetInstanceId),
    /// Move widget between surfaces.
    MoveWidgetToSurface {
        /// Instance id.
        id: WidgetInstanceId,
        /// Target surface.
        surface: Surface,
    },
    /// Duplicate a widget instance.
    DuplicateWidget(WidgetInstanceId),
    /// Reorder widget within surface (lower index = earlier).
    ReorderWidget {
        /// Instance id.
        id: WidgetInstanceId,
        /// Direction.
        dir: ReorderDir,
        /// Surface.
        surface: Surface,
    },
}

/// Which sidebar to toggle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarKind {
    /// Far-left server rail.
    ServerRail,
    /// Navigation sidebar.
    Nav,
}

/// Note field for updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteField {
    /// Title.
    Title,
    /// Body.
    Body,
}

/// Reorder direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReorderDir {
    /// Toward index 0.
    Up,
    /// Toward the end.
    Down,
}
