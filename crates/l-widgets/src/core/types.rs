//! Widget manifests, permissions, data needs, and the instance trait.

use std::collections::HashMap;
use std::time::Duration;

use l_core::models::{GridSize, PlacedWidget, WidgetInstanceId};
use l_core::{Action, AppState};

/// Alias for the core runtime state machine.
pub use l_core::state::WidgetRuntimeState as WidgetState;
use serde::{Deserialize, Serialize};

/// Widget category shown in the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WidgetCategory {
    /// Discord integration widgets.
    Discord,
    /// Server monitoring.
    Monitoring,
    /// Productivity tools.
    Productivity,
    /// System info.
    System,
    /// Developer tools.
    Developer,
    /// Custom/user integrations.
    Custom,
}

impl WidgetCategory {
    /// All categories.
    pub const ALL: [WidgetCategory; 6] = [
        WidgetCategory::Discord,
        WidgetCategory::Monitoring,
        WidgetCategory::Productivity,
        WidgetCategory::System,
        WidgetCategory::Developer,
        WidgetCategory::Custom,
    ];

    /// Display label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Discord => "DISCORD",
            Self::Monitoring => "MONITORING",
            Self::Productivity => "PRODUCTIVITY",
            Self::System => "SYSTEM",
            Self::Developer => "DEVELOPER",
            Self::Custom => "CUSTOM",
        }
    }
}

/// Permission a widget may require. The runtime enforces these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    /// Read Discord account identity (OAuth `identify`).
    DiscordIdentify,
    /// Read guild list (OAuth `guilds`).
    DiscordGuilds,
    /// Bot REST access to a guild.
    DiscordBotGuild,
    /// Bot Gateway intents (members/presences/content).
    DiscordIntents,
    /// Outbound network requests to user-configured endpoints.
    Network,
    /// Read/write local notes.
    LocalNotes,
    /// Read notification history.
    LocalNotifications,
    /// Read/write widget config and layout storage.
    LocalStorage,
    /// Show OS notifications.
    OsNotify,
}

/// Refresh policy for a widget's data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefreshPolicy {
    /// Only when the user hits refresh.
    Manual,
    /// Poll on an interval.
    Interval {
        /// Default interval.
        default_secs: u64,
        /// Minimum allowed interval.
        min_secs: u64,
    },
    /// Data pushed by a subscription.
    Push,
}

/// Declarative widget setting field (drives the generic config dialog).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingField {
    /// Config key.
    pub key: String,
    /// Label.
    pub label: String,
    /// Field kind.
    pub kind: SettingKind,
    /// Default value as JSON.
    pub default: serde_json::Value,
}

/// Setting field kinds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SettingKind {
    /// Free text.
    Text,
    /// Integer with min/max.
    Integer {
        /// Min.
        min: i64,
        /// Max.
        max: i64,
    },
    /// Boolean toggle.
    Boolean,
    /// Choice from a list.
    Choice {
        /// Options.
        options: Vec<String>,
    },
    /// Secret/write-only field (never revealed).
    Secret,
    /// URL.
    Url,
}

/// Static description of a widget type.
pub struct WidgetManifest {
    /// Stable namespaced id, e.g. `l.server_stats`.
    pub id: &'static str,
    /// Display name.
    pub name: &'static str,
    /// Short description.
    pub description: &'static str,
    /// Lucide icon name mapped by the app to `lucide_icons::Icon`.
    pub icon: &'static str,
    /// Semver-ish version string.
    pub version: &'static str,
    /// Category.
    pub category: WidgetCategory,
    /// Required permissions.
    pub permissions: &'static [Permission],
    /// Default grid size.
    pub default_size: GridSize,
    /// Minimum grid size.
    pub min_size: GridSize,
    /// Maximum grid size.
    pub max_size: Option<GridSize>,
    /// Refresh policy.
    pub refresh: RefreshPolicy,
    /// Settings schema.
    pub settings: Vec<SettingField>,
}

impl WidgetManifest {
    /// Whether the manifest's permissions are satisfied by `caps`-style check
    /// performed by the caller. Kept as data; enforcement lives in the runtime.
    #[must_use]
    pub fn requires(&self, perm: Permission) -> bool {
        self.permissions.contains(&perm)
    }
}

/// A data need declared by a widget instance.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataNeed {
    /// Discord account profile.
    DiscordAccount,
    /// Guild list with counts.
    DiscordGuilds,
    /// Stats for a specific guild.
    GuildStats {
        /// Guild id (`"@me"` or selected guild allowed at runtime).
        guild_id: String,
    },
    /// Recent member joins/events for a guild.
    GuildActivity {
        /// Guild id.
        guild_id: String,
    },
    /// Voice channel states for a guild.
    GuildVoice {
        /// Guild id.
        guild_id: String,
    },
    /// Bot connection status.
    BotStatus,
    /// Local notes (optionally scoped).
    Notes {
        /// Workspace scope.
        workspace: Option<String>,
    },
    /// Notification history.
    Notifications,
    /// Uptime checks for configured services. `spec_key` encodes the
    /// `name=url|name=url` service list so the fetcher can reconstruct it.
    UptimeServices {
        /// Encoded service spec (dedup + payload key).
        spec_key: String,
    },
    /// Custom API endpoint probes.
    ApiProbes {
        /// Endpoint list signature (dedup key).
        endpoints_key: String,
    },
    /// System stats (CPU/mem approximation).
    SystemStats,
    /// Data polled from an external plugin view. `params` is the JSON-encoded
    /// widget config sent to the plugin; it participates in the need key so
    /// differently-configured instances poll independently.
    Plugin {
        /// Plugin id.
        plugin: String,
        /// View id within the plugin.
        view: String,
        /// JSON-encoded config params.
        params: String,
    },
}

impl DataNeed {
    /// Stable string key used for dedup and event routing.
    #[must_use]
    pub fn key(&self) -> String {
        match self {
            Self::DiscordAccount => "discord.account".into(),
            Self::DiscordGuilds => "discord.guilds".into(),
            Self::GuildStats { guild_id } => format!("discord.guild_stats.{guild_id}"),
            Self::GuildActivity { guild_id } => format!("discord.guild_activity.{guild_id}"),
            Self::GuildVoice { guild_id } => format!("discord.guild_voice.{guild_id}"),
            Self::BotStatus => "discord.bot_status".into(),
            Self::Notes { workspace } => {
                format!("local.notes.{}", workspace.as_deref().unwrap_or("*"))
            }
            Self::Notifications => "local.notifications".into(),
            Self::UptimeServices { spec_key } => format!("uptime.services.{spec_key}"),
            Self::ApiProbes { endpoints_key } => format!("api.probes.{endpoints_key}"),
            Self::SystemStats => "system.stats".into(),
            Self::Plugin {
                plugin,
                view,
                params,
            } => {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                params.hash(&mut h);
                format!("plugin.{plugin}.{view}.{:016x}", h.finish())
            }
        }
    }
}

/// Type-erased JSON snapshot produced by the data hub.
#[derive(Debug, Clone, Default)]
pub struct WidgetData {
    /// Raw JSON payload.
    pub json: serde_json::Value,
}

impl WidgetData {
    /// Empty snapshot.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            json: serde_json::Value::Null,
        }
    }

    /// Decode the payload as `T`.
    #[must_use]
    pub fn decode<T: serde::de::DeserializeOwned>(&self) -> Option<T> {
        serde_json::from_value(self.json.clone()).ok()
    }

    /// Whether the payload is null/missing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.json.is_null()
    }
}

/// Context passed to widget `ui()` — dispatch actions, read app state.
pub struct WidgetCx<'a> {
    /// Dispatch an action into the reducer.
    pub dispatch: &'a mut dyn FnMut(Action),
    /// Read-only app state snapshot accessors.
    pub state: &'a AppState,
    /// Animation intensity for this frame.
    pub animation: crate::core::IntensityHint,
}

/// Intensity hint mirrored from l-ui without a hard dependency cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntensityHint {
    /// Off.
    Off,
    /// Subtle.
    Subtle,
    /// Full.
    Full,
}

/// A live widget instance.
pub trait WidgetInstance: Send {
    /// Data needs for the runtime.
    fn data_needs(&self) -> Vec<DataNeed>;

    /// Draw the ready state. Must be cheap and non-blocking.
    fn ui(&mut self, ui: &mut egui::Ui, cx: &mut WidgetCx<'_>, data: &WidgetData);

    /// Called when config changes.
    fn on_config_changed(&mut self, config: &serde_json::Value);

    /// Per-instance refresh interval override in seconds, if any.
    fn refresh_secs(&self) -> Option<u64> {
        None
    }
}

/// Errors from widget factories.
#[derive(Debug, thiserror::Error)]
pub enum WidgetError {
    /// Invalid configuration.
    #[error("invalid config: {0}")]
    InvalidConfig(String),
    /// Widget type not registered.
    #[error("unknown widget type: {0}")]
    UnknownType(String),
}

/// Factory creating widget instances.
pub trait WidgetFactory: Send + Sync {
    /// The manifest.
    fn manifest(&self) -> &'static WidgetManifest;
    /// Create an instance from a placed-widget config.
    fn create(&self, placed: &PlacedWidget) -> Result<Box<dyn WidgetInstance>, WidgetError>;
}

/// Config helpers: get a typed value from a `PlacedWidget.config` object with
/// manifest defaults applied.
#[must_use]
pub fn config_get<T: serde::de::DeserializeOwned + Clone>(
    placed: &PlacedWidget,
    manifest: &WidgetManifest,
    key: &str,
) -> Option<T> {
    // Instance config overrides manifest defaults.
    if let Some(v) = placed.config.get(key)
        && let Ok(t) = serde_json::from_value::<T>(v.clone())
    {
        return Some(t);
    }
    for f in &manifest.settings {
        if f.key == key
            && let Ok(t) = serde_json::from_value::<T>(f.default.clone())
        {
            return Some(t);
        }
    }
    None
}

/// All config values merged with defaults.
#[must_use]
pub fn merged_config(manifest: &WidgetManifest, placed: &PlacedWidget) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for f in &manifest.settings {
        map.insert(f.key.clone(), f.default.clone());
    }
    if let serde_json::Value::Object(inst) = &placed.config {
        for (k, v) in inst {
            map.insert(k.clone(), v.clone());
        }
    }
    serde_json::Value::Object(map)
}

/// Convenience: read config string.
#[must_use]
pub fn cfg_string(placed: &PlacedWidget, manifest: &WidgetManifest, key: &str) -> String {
    config_get::<String>(placed, manifest, key).unwrap_or_default()
}

/// Convenience: read config bool.
#[must_use]
pub fn cfg_bool(placed: &PlacedWidget, manifest: &WidgetManifest, key: &str) -> bool {
    config_get::<bool>(placed, manifest, key).unwrap_or(false)
}

/// Convenience: read config u64.
#[must_use]
pub fn cfg_u64(placed: &PlacedWidget, manifest: &WidgetManifest, key: &str) -> u64 {
    config_get::<u64>(placed, manifest, key).unwrap_or(0)
}

/// Collect unique data needs across placed widgets (dedup by key).
#[must_use]
pub fn collect_needs(
    placed: &[PlacedWidget],
    instances: &HashMap<WidgetInstanceId, Box<dyn WidgetInstance>>,
) -> Vec<(WidgetInstanceId, DataNeed)> {
    let mut out = Vec::new();
    for w in placed {
        if !w.enabled {
            continue;
        }
        if let Some(inst) = instances.get(&w.id) {
            for need in inst.data_needs() {
                out.push((w.id, need));
            }
        }
    }
    out
}

/// Map a manifest icon name to a `lucide_icons::Icon` variant.
#[must_use]
pub fn icon_by_name(name: &str) -> Option<lucide_icons::Icon> {
    use lucide_icons::Icon;
    Some(match name {
        "activity" => Icon::Activity,
        "bar-chart" | "chart" => Icon::ChartColumn,
        "bell" => Icon::Bell,
        "bot" => Icon::Bot,
        "clock" => Icon::Clock,
        "command" => Icon::SquareTerminal,
        "file-text" | "note" => Icon::FileText,
        "gauge" => Icon::Gauge,
        "hash" => Icon::Hash,
        "layout-dashboard" | "dashboard" => Icon::LayoutDashboard,
        "mic" => Icon::Mic,
        "monitor" => Icon::Monitor,
        "network" => Icon::Network,
        "radio" => Icon::Radio,
        "server" => Icon::Server,
        "settings" => Icon::Settings,
        "terminal" => Icon::Terminal,
        "users" => Icon::Users,
        "zap" => Icon::Zap,
        "globe" => Icon::Globe,
        "database" => Icon::Database,
        "cpu" => Icon::Cpu,
        "hard-drive" => Icon::HardDrive,
        "memory-stick" => Icon::MemoryStick,
        "list-checks" => Icon::ListChecks,
        "sticky-note" => Icon::StickyNote,
        "satellite-dish" => Icon::SatelliteDish,
        "signal" => Icon::Signal,
        "timer" => Icon::Timer,
        "triangle-alert" | "alert" => Icon::TriangleAlert,
        "wifi" => Icon::Wifi,
        _ => return None,
    })
}

/// Widget category filter used by the library UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryFilter {
    /// All widgets.
    All,
    /// One category.
    Category(WidgetCategory),
}

/// Data snapshot bundled for the renderer.
pub type SnapshotMap = HashMap<WidgetInstanceId, WidgetData>;

/// Duration helper.
#[must_use]
pub fn secs(s: u64) -> Duration {
    Duration::from_secs(s)
}
