//! Member Activity widget: recent joins/events timeline.

use egui::{RichText, Ui};

use l_core::models::{GridSize, PlacedWidget};
use l_ui::theme::color;

use crate::built_in::common::{StatusTone, caption, rel_time, stat_row, status_text};
use crate::core::types::{
    DataNeed, Permission, RefreshPolicy, SettingField, SettingKind, WidgetCategory, WidgetCx,
    WidgetData, WidgetError, WidgetFactory, WidgetInstance, WidgetManifest, cfg_string,
};

static MANIFEST: std::sync::LazyLock<WidgetManifest> =
    std::sync::LazyLock::new(|| WidgetManifest {
        id: "l.member_activity",
        name: "Member Activity",
        description: "Recent member joins and server events.",
        icon: "activity",
        version: "1.0.0",
        category: WidgetCategory::Discord,
        permissions: &[Permission::DiscordGuilds, Permission::DiscordIntents],
        default_size: GridSize::new(4, 3),
        min_size: GridSize::new(3, 2),
        max_size: Some(GridSize::new(8, 6)),
        refresh: RefreshPolicy::Interval {
            default_secs: 30,
            min_secs: 10,
        },
        settings: vec![SettingField {
            key: "guild_id".into(),
            label: "Guild ID".into(),
            kind: SettingKind::Text,
            default: serde_json::json!(""),
        }],
    });

/// One activity event.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ActivityEvent {
    /// Kind: join, leave, boost, event.
    pub kind: String,
    /// Actor name.
    pub actor: String,
    /// Unix seconds.
    pub ts: i64,
    /// Detail text.
    pub detail: Option<String>,
}

/// Payload.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ActivityPayload {
    /// Events newest-first.
    pub events: Vec<ActivityEvent>,
    /// Whether member events are available (privileged intent).
    pub members_available: bool,
}

pub struct MemberActivityFactory;

impl WidgetFactory for MemberActivityFactory {
    fn manifest(&self) -> &'static WidgetManifest {
        &MANIFEST
    }
    fn create(&self, placed: &PlacedWidget) -> Result<Box<dyn WidgetInstance>, WidgetError> {
        Ok(Box::new(MemberActivity {
            placed: placed.clone(),
        }))
    }
}

struct MemberActivity {
    placed: PlacedWidget,
}

impl WidgetInstance for MemberActivity {
    fn data_needs(&self) -> Vec<DataNeed> {
        let gid = cfg_string(&self.placed, &MANIFEST, "guild_id");
        let gid = if gid.is_empty() {
            "@selected".into()
        } else {
            gid
        };
        vec![DataNeed::GuildActivity { guild_id: gid }]
    }

    fn on_config_changed(&mut self, config: &serde_json::Value) {
        self.placed.config = config.clone();
    }

    fn ui(&mut self, ui: &mut Ui, _cx: &mut WidgetCx<'_>, data: &WidgetData) {
        let Some(payload) = data.decode::<ActivityPayload>() else {
            ui.label(
                RichText::new("waiting for data…")
                    .small()
                    .color(color::text_muted()),
            );
            return;
        };
        if !payload.members_available {
            status_text(
                ui,
                "member events require the guild-members intent",
                StatusTone::Warn,
            );
        }
        if payload.events.is_empty() {
            ui.label(
                RichText::new("no recent activity")
                    .small()
                    .color(color::text_muted()),
            );
            return;
        }
        caption(ui, "recent events");
        for ev in payload.events.iter().take(8) {
            ui.horizontal(|ui| {
                let tone = match ev.kind.as_str() {
                    "join" => StatusTone::Ok,
                    "boost" => StatusTone::Warn,
                    _ => StatusTone::Neutral,
                };
                ui.label(
                    RichText::new(format!("{:>6}", rel_time(ev.ts)))
                        .small()
                        .color(color::text_muted()),
                );
                ui.label(RichText::new(&ev.actor).small().color(color::text()));
                status_text(ui, &ev.kind, tone);
                if let Some(d) = &ev.detail {
                    ui.label(RichText::new(d).small().color(color::text_muted()));
                }
            });
        }
        let _ = stat_row;
    }
}
