//! Central command registry for the palette and shortcuts.

use crate::actions::Action;
use crate::models::Page;

/// Command category shown as a palette section header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandCategory {
    /// Page navigation.
    Navigation,
    /// Workspace/layout commands.
    Workspace,
    /// Widget commands.
    Widgets,
    /// Discord commands.
    Discord,
    /// System commands.
    System,
}

impl CommandCategory {
    /// Section header text.
    #[must_use]
    pub fn header(self) -> &'static str {
        match self {
            Self::Navigation => "NAVIGATION",
            Self::Workspace => "WORKSPACE",
            Self::Widgets => "WIDGETS",
            Self::Discord => "DISCORD",
            Self::System => "SYSTEM",
        }
    }
}

/// A registered command.
#[derive(Debug, Clone)]
pub struct CommandDef {
    /// Stable id.
    pub id: &'static str,
    /// Display title.
    pub title: &'static str,
    /// Category.
    pub category: CommandCategory,
    /// Extra search keywords.
    pub keywords: &'static [&'static str],
    /// Optional shortcut description (e.g. `Ctrl+K`).
    pub shortcut: Option<&'static str>,
    /// The action dispatched when run.
    pub action: Action,
}

/// Build the static command registry.
///
/// Dynamic commands (per-widget, per-guild) are appended by the app at runtime.
#[must_use]
pub fn builtin_commands() -> Vec<CommandDef> {
    let nav = |id: &'static str, title: &'static str, page: Page| CommandDef {
        id,
        title,
        category: CommandCategory::Navigation,
        keywords: &[],
        shortcut: None,
        action: Action::Navigate(page),
    };
    vec![
        nav(
            "nav.workspace",
            "Open Server Workspace",
            Page::ServerWorkspace,
        ),
        nav("nav.library", "Open Widget Library", Page::WidgetLibrary),
        nav("nav.activity", "Open Activity", Page::Activity),
        nav(
            "nav.notifications",
            "Open Notifications",
            Page::Notifications,
        ),
        nav("nav.bot", "Open Bot Management", Page::BotManagement),
        CommandDef {
            id: "nav.settings",
            title: "Open Settings",
            category: CommandCategory::Navigation,
            keywords: &["preferences", "config"],
            shortcut: None,
            action: Action::Navigate(Page::Settings),
        },
        CommandDef {
            id: "nav.settings.discord",
            title: "Settings: Discord",
            category: CommandCategory::Navigation,
            keywords: &["oauth", "bot", "token"],
            shortcut: None,
            action: Action::NavigateSettingsTab(crate::state::SettingsTab::Discord),
        },
        CommandDef {
            id: "nav.settings.appearance",
            title: "Settings: Appearance",
            category: CommandCategory::Navigation,
            keywords: &["theme", "font", "density"],
            shortcut: None,
            action: Action::NavigateSettingsTab(crate::state::SettingsTab::Appearance),
        },
        CommandDef {
            id: "discord.client",
            title: "Open Discord Client",
            category: CommandCategory::Navigation,
            keywords: &["discord", "chat", "webview", "client"],
            shortcut: None,
            action: Action::OpenDiscordClient,
        },
        CommandDef {
            id: "discord.client.restart",
            title: "Restart Discord Client",
            category: CommandCategory::Navigation,
            keywords: &["discord", "reload", "mods"],
            shortcut: None,
            action: Action::RestartDiscordClient,
        },
        CommandDef {
            id: "ws.toggle_dock",
            title: "Toggle Widget Dock",
            category: CommandCategory::Workspace,
            keywords: &["panel", "sidebar"],
            shortcut: None,
            action: Action::ToggleDock,
        },
        CommandDef {
            id: "ws.toggle_rail",
            title: "Toggle Server Rail",
            category: CommandCategory::Workspace,
            keywords: &[],
            shortcut: None,
            action: Action::ToggleSidebar(crate::actions::SidebarKind::ServerRail),
        },
        CommandDef {
            id: "ws.toggle_nav",
            title: "Toggle Navigation Sidebar",
            category: CommandCategory::Workspace,
            keywords: &[],
            shortcut: None,
            action: Action::ToggleSidebar(crate::actions::SidebarKind::Nav),
        },
        CommandDef {
            id: "ws.edit_layout",
            title: "Edit Widget Layout",
            category: CommandCategory::Workspace,
            keywords: &["drag", "resize", "arrange"],
            shortcut: None,
            action: Action::ToggleLayoutEditing,
        },
        CommandDef {
            id: "sys.check_connections",
            title: "Check Connections",
            category: CommandCategory::System,
            keywords: &["status", "health"],
            shortcut: None,
            action: Action::Event(crate::events::AppEvent::Tick),
        },
        CommandDef {
            id: "sys.refresh_widgets",
            title: "Refresh All Widgets",
            category: CommandCategory::System,
            keywords: &["reload"],
            shortcut: None,
            action: Action::Event(crate::events::AppEvent::Tick),
        },
        CommandDef {
            id: "sys.toggle_demo",
            title: "Toggle Demo Mode",
            category: CommandCategory::System,
            keywords: &["sample", "offline"],
            shortcut: None,
            action: Action::ToggleDemoMode,
        },
        CommandDef {
            id: "sys.palette",
            title: "Command Palette",
            category: CommandCategory::System,
            keywords: &[],
            shortcut: Some("Ctrl+K"),
            action: Action::TogglePalette,
        },
    ]
}

/// A user-bindable keyboard shortcut.
///
/// `action` is `None` for ids handled specially by the app shell (e.g. the
/// dismiss-overlays cascade, which is context dependent).
#[derive(Debug, Clone)]
pub struct KeybindEntry {
    /// Stable id (matches `Preferences::keybinds[].action`).
    pub id: &'static str,
    /// Display title in the settings UI.
    pub title: &'static str,
    /// Default combo like `Ctrl+K`; `None` = unbound by default.
    pub default_combo: Option<&'static str>,
    /// Action dispatched when the combo is pressed.
    pub action: Option<Action>,
}

/// The catalog of bindable shortcuts.
#[must_use]
pub fn keybind_catalog() -> Vec<KeybindEntry> {
    vec![
        KeybindEntry {
            id: "sys.palette",
            title: "Command palette",
            default_combo: Some("Ctrl+K"),
            action: Some(Action::TogglePalette),
        },
        KeybindEntry {
            id: "sys.dismiss",
            title: "Dismiss dialogs & palette",
            default_combo: Some("Escape"),
            action: None,
        },
        KeybindEntry {
            id: "ws.toggle_dock",
            title: "Toggle widget dock",
            default_combo: None,
            action: Some(Action::ToggleDock),
        },
        KeybindEntry {
            id: "ws.toggle_rail",
            title: "Toggle server rail",
            default_combo: None,
            action: Some(Action::ToggleSidebar(crate::actions::SidebarKind::ServerRail)),
        },
        KeybindEntry {
            id: "ws.toggle_nav",
            title: "Toggle navigation sidebar",
            default_combo: None,
            action: Some(Action::ToggleSidebar(crate::actions::SidebarKind::Nav)),
        },
        KeybindEntry {
            id: "ws.edit_layout",
            title: "Toggle layout editing",
            default_combo: None,
            action: Some(Action::ToggleLayoutEditing),
        },
        KeybindEntry {
            id: "nav.settings",
            title: "Open settings",
            default_combo: None,
            action: Some(Action::Navigate(Page::Settings)),
        },
    ]
}
