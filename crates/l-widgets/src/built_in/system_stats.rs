//! System Stats widget: lightweight local machine stats.

use egui::{RichText, Ui};

use l_core::models::{GridSize, PlacedWidget};
use l_ui::theme::color;

use crate::built_in::common::stat_row;
use crate::core::types::{
    DataNeed, RefreshPolicy, WidgetCategory, WidgetCx, WidgetData, WidgetError, WidgetFactory,
    WidgetInstance, WidgetManifest,
};

static MANIFEST: std::sync::LazyLock<WidgetManifest> =
    std::sync::LazyLock::new(|| WidgetManifest {
        id: "l.system_stats",
        name: "System Stats",
        description: "CPU, memory, and uptime of this machine.",
        icon: "cpu",
        version: "1.0.0",
        category: WidgetCategory::System,
        permissions: &[],
        default_size: GridSize::new(3, 2),
        min_size: GridSize::new(2, 2),
        max_size: Some(GridSize::new(6, 4)),
        refresh: RefreshPolicy::Interval {
            default_secs: 10,
            min_secs: 5,
        },
        settings: vec![],
    });

/// Payload.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SystemPayload {
    /// CPU utilization %.
    pub cpu_pct: f32,
    /// Memory utilization %.
    pub mem_pct: f32,
    /// System uptime seconds.
    pub uptime_secs: u64,
    /// Hostname.
    pub hostname: String,
    /// OS name.
    pub os: String,
}

pub struct SystemStatsFactory;

impl WidgetFactory for SystemStatsFactory {
    fn manifest(&self) -> &'static WidgetManifest {
        &MANIFEST
    }
    fn create(&self, placed: &PlacedWidget) -> Result<Box<dyn WidgetInstance>, WidgetError> {
        Ok(Box::new(SystemStats {
            placed: placed.clone(),
        }))
    }
}

struct SystemStats {
    placed: PlacedWidget,
}

impl WidgetInstance for SystemStats {
    fn data_needs(&self) -> Vec<DataNeed> {
        vec![DataNeed::SystemStats]
    }

    fn on_config_changed(&mut self, config: &serde_json::Value) {
        self.placed.config = config.clone();
    }

    fn ui(&mut self, ui: &mut Ui, _cx: &mut WidgetCx<'_>, data: &WidgetData) {
        let Some(s) = data.decode::<SystemPayload>() else {
            ui.label(RichText::new("waiting…").small().color(color::text_muted()));
            return;
        };
        stat_row(ui, "cpu", &format!("{:.0}%", s.cpu_pct));
        stat_row(ui, "memory", &format!("{:.0}%", s.mem_pct));
        let days = s.uptime_secs / 86400;
        let hours = (s.uptime_secs % 86400) / 3600;
        stat_row(ui, "uptime", &format!("{days}d {hours}h"));
        stat_row(ui, "host", &s.hostname);
        stat_row(ui, "os", &s.os);
    }
}
