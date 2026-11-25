//! Voice Activity widget: voice channel participant counts.
//!
//! Data is only available through an authorized bot Gateway session with the
//! voice-states intent; otherwise the widget shows an explanatory empty state.

use egui::{RichText, Ui};

use l_core::models::{GridSize, PlacedWidget};
use l_ui::theme::color;

use crate::built_in::common::stat_row;
use crate::core::types::{
    DataNeed, Permission, RefreshPolicy, SettingField, SettingKind, WidgetCategory, WidgetCx,
    WidgetData, WidgetError, WidgetFactory, WidgetInstance, WidgetManifest, cfg_string,
};

static MANIFEST: std::sync::LazyLock<WidgetManifest> =
    std::sync::LazyLock::new(|| WidgetManifest {
        id: "l.voice_activity",
        name: "Voice Activity",
        description: "Current voice channel participants (requires bot Gateway).",
        icon: "mic",
        version: "1.0.0",
        category: WidgetCategory::Discord,
        permissions: &[Permission::DiscordBotGuild, Permission::DiscordIntents],
        default_size: GridSize::new(3, 2),
        min_size: GridSize::new(3, 2),
        max_size: Some(GridSize::new(6, 4)),
        refresh: RefreshPolicy::Interval {
            default_secs: 15,
            min_secs: 5,
        },
        settings: vec![SettingField {
            key: "guild_id".into(),
            label: "Guild ID".into(),
            kind: SettingKind::Text,
            default: serde_json::json!(""),
        }],
    });

/// Payload.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct VoicePayload {
    /// Whether voice data is available at all.
    pub available: bool,
    /// Reason when unavailable.
    pub reason: Option<String>,
    /// Voice channels with participant counts.
    pub channels: Vec<VoiceChannel>,
}

/// Voice channel snapshot.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct VoiceChannel {
    /// Channel name.
    pub name: String,
    /// Participant count.
    pub participants: u64,
}

pub struct VoiceActivityFactory;

impl WidgetFactory for VoiceActivityFactory {
    fn manifest(&self) -> &'static WidgetManifest {
        &MANIFEST
    }
    fn create(&self, placed: &PlacedWidget) -> Result<Box<dyn WidgetInstance>, WidgetError> {
        Ok(Box::new(VoiceWidget {
            placed: placed.clone(),
        }))
    }
}

struct VoiceWidget {
    placed: PlacedWidget,
}

impl WidgetInstance for VoiceWidget {
    fn data_needs(&self) -> Vec<DataNeed> {
        let gid = cfg_string(&self.placed, &MANIFEST, "guild_id");
        let gid = if gid.is_empty() {
            "@selected".into()
        } else {
            gid
        };
        vec![DataNeed::GuildVoice { guild_id: gid }]
    }

    fn on_config_changed(&mut self, config: &serde_json::Value) {
        self.placed.config = config.clone();
    }

    fn ui(&mut self, ui: &mut Ui, _cx: &mut WidgetCx<'_>, data: &WidgetData) {
        let Some(payload) = data.decode::<VoicePayload>() else {
            ui.label(RichText::new("waiting…").small().color(color::text_muted()));
            return;
        };
        if !payload.available {
            ui.label(
                RichText::new(
                    payload
                        .reason
                        .as_deref()
                        .unwrap_or("voice data requires the bot Gateway session"),
                )
                .small()
                .color(color::text_muted()),
            );
            return;
        }
        if payload.channels.is_empty() {
            ui.label(
                RichText::new("no one in voice")
                    .small()
                    .color(color::text_muted()),
            );
            return;
        }
        for ch in payload.channels.iter().take(8) {
            stat_row(ui, &format!("#{}", ch.name), &ch.participants.to_string());
        }
    }
}
