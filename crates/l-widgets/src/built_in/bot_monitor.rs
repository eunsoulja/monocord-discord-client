//! Bot Monitor widget: bot token status, latency, gateway state.

use egui::{RichText, Ui};

use l_core::models::{GridSize, PlacedWidget};
use l_ui::theme::color;

use crate::built_in::common::{StatusTone, stat_row, status_text};
use crate::core::types::{
    DataNeed, Permission, RefreshPolicy, WidgetCategory, WidgetCx, WidgetData, WidgetError,
    WidgetFactory, WidgetInstance, WidgetManifest,
};

static MANIFEST: std::sync::LazyLock<WidgetManifest> =
    std::sync::LazyLock::new(|| WidgetManifest {
        id: "l.bot_monitor",
        name: "Bot Monitor",
        description: "Bot connection status, latency, and gateway health.",
        icon: "bot",
        version: "1.0.0",
        category: WidgetCategory::Discord,
        permissions: &[Permission::DiscordBotGuild, Permission::DiscordIntents],
        default_size: GridSize::new(3, 2),
        min_size: GridSize::new(3, 2),
        max_size: Some(GridSize::new(6, 4)),
        refresh: RefreshPolicy::Interval {
            default_secs: 30,
            min_secs: 10,
        },
        settings: vec![],
    });

/// Payload produced by the runtime.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BotPayload {
    /// `not_configured` | `configured` | `online` | `error`
    pub status: String,
    /// Bot username.
    pub username: Option<String>,
    /// Latency ms.
    pub latency_ms: Option<u64>,
    /// Gateway state string.
    pub gateway: Option<String>,
    /// Error message.
    pub error: Option<String>,
}

pub struct BotMonitorFactory;

impl WidgetFactory for BotMonitorFactory {
    fn manifest(&self) -> &'static WidgetManifest {
        &MANIFEST
    }
    fn create(&self, placed: &PlacedWidget) -> Result<Box<dyn WidgetInstance>, WidgetError> {
        Ok(Box::new(BotMonitor {
            placed: placed.clone(),
        }))
    }
}

struct BotMonitor {
    placed: PlacedWidget,
}

impl WidgetInstance for BotMonitor {
    fn data_needs(&self) -> Vec<DataNeed> {
        vec![DataNeed::BotStatus]
    }

    fn on_config_changed(&mut self, config: &serde_json::Value) {
        self.placed.config = config.clone();
    }

    fn ui(&mut self, ui: &mut Ui, _cx: &mut WidgetCx<'_>, data: &WidgetData) {
        let Some(b) = data.decode::<BotPayload>() else {
            ui.label(
                RichText::new("no bot configured")
                    .small()
                    .color(color::text_muted()),
            );
            return;
        };
        let (label, tone) = match b.status.as_str() {
            "online" => ("ONLINE", StatusTone::Ok),
            "configured" => ("CONFIGURED", StatusTone::Warn),
            "error" => ("ERROR", StatusTone::Err),
            _ => ("NOT CONFIGURED", StatusTone::Neutral),
        };
        status_text(ui, label, tone);
        if let Some(name) = &b.username {
            stat_row(ui, "bot", name);
        }
        if let Some(ms) = b.latency_ms {
            stat_row(ui, "latency", &format!("{ms} ms"));
        }
        if let Some(gw) = &b.gateway {
            stat_row(ui, "gateway", gw);
        }
        if let Some(err) = &b.error {
            ui.label(RichText::new(err).small().color(color::error()));
        }
        if b.status == "not_configured" {
            ui.label(
                RichText::new("add a bot token in Settings → Discord")
                    .size(10.0)
                    .color(color::text_muted()),
            );
        }
    }
}
