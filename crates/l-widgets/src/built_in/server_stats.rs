//! Server Statistics widget.

use egui::{RichText, Ui};

use l_core::models::{GridSize, PlacedWidget};
use l_ui::theme::color;

use crate::built_in::common::{stat_block, stat_row};
use crate::core::types::{
    DataNeed, Permission, RefreshPolicy, SettingField, SettingKind, WidgetCategory, WidgetCx,
    WidgetData, WidgetError, WidgetFactory, WidgetInstance, WidgetManifest, cfg_string, config_get,
    merged_config,
};

static MANIFEST: std::sync::LazyLock<WidgetManifest> =
    std::sync::LazyLock::new(|| WidgetManifest {
        id: "l.server_stats",
        name: "Server Statistics",
        description: "Member counts, channels, and roles for a Discord server.",
        icon: "server",
        version: "1.0.0",
        category: WidgetCategory::Discord,
        permissions: &[Permission::DiscordGuilds, Permission::DiscordBotGuild],
        default_size: GridSize::new(4, 3),
        min_size: GridSize::new(3, 2),
        max_size: Some(GridSize::new(8, 6)),
        refresh: RefreshPolicy::Interval {
            default_secs: 60,
            min_secs: 15,
        },
        settings: vec![
            SettingField {
                key: "guild_id".into(),
                label: "Guild ID".into(),
                kind: SettingKind::Text,
                default: serde_json::json!(""),
            },
            SettingField {
                key: "refresh_secs".into(),
                label: "Refresh (s)".into(),
                kind: SettingKind::Integer { min: 15, max: 3600 },
                default: serde_json::json!(60),
            },
        ],
    });

/// Payload produced by the data hub.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct GuildStats {
    /// Guild name.
    pub name: String,
    /// Member count.
    pub member_count: Option<u64>,
    /// Online count.
    pub online_count: Option<u64>,
    /// Channel count.
    pub channel_count: Option<u64>,
    /// Role count.
    pub role_count: Option<u64>,
    /// Created-at unix seconds.
    pub created_at: Option<i64>,
    /// Approximate or exact flag.
    pub approximate: bool,
}

pub struct ServerStatsFactory;

impl WidgetFactory for ServerStatsFactory {
    fn manifest(&self) -> &'static WidgetManifest {
        &MANIFEST
    }
    fn create(&self, placed: &PlacedWidget) -> Result<Box<dyn WidgetInstance>, WidgetError> {
        Ok(Box::new(ServerStats {
            placed: placed.clone(),
        }))
    }
}

struct ServerStats {
    placed: PlacedWidget,
}

impl ServerStats {
    fn guild_id(&self) -> String {
        cfg_string(&self.placed, &MANIFEST, "guild_id")
    }
}

impl WidgetInstance for ServerStats {
    fn data_needs(&self) -> Vec<DataNeed> {
        let gid = self.guild_id();
        let gid = if gid.is_empty() {
            "@selected".to_string()
        } else {
            gid
        };
        vec![DataNeed::GuildStats { guild_id: gid }]
    }

    fn refresh_secs(&self) -> Option<u64> {
        config_get::<u64>(&self.placed, &MANIFEST, "refresh_secs")
    }

    fn on_config_changed(&mut self, config: &serde_json::Value) {
        self.placed.config = config.clone();
    }

    fn ui(&mut self, ui: &mut Ui, _cx: &mut WidgetCx<'_>, data: &WidgetData) {
        let Some(stats) = data.decode::<GuildStats>() else {
            ui.label(
                RichText::new("waiting for data…")
                    .small()
                    .color(color::text_muted()),
            );
            return;
        };
        ui.horizontal(|ui| {
            stat_block(
                ui,
                &stats.member_count.map_or("—".into(), |n| n.to_string()),
                "members",
            );
            stat_block(
                ui,
                &stats.online_count.map_or("—".into(), |n| n.to_string()),
                "online",
            );
            stat_block(
                ui,
                &stats.channel_count.map_or("—".into(), |n| n.to_string()),
                "channels",
            );
            stat_block(
                ui,
                &stats.role_count.map_or("—".into(), |n| n.to_string()),
                "roles",
            );
        });
        ui.add_space(4.0);
        stat_row(ui, "server", &stats.name);
        if stats.approximate {
            ui.label(
                RichText::new("counts are approximate")
                    .size(10.0)
                    .color(color::text_muted()),
            );
        }
        let _ = merged_config(&MANIFEST, &self.placed);
    }
}
