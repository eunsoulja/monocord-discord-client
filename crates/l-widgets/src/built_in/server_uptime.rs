//! Server Uptime widget: monitors user-configured HTTP endpoints.

use egui::{RichText, Ui};

use l_core::models::{GridSize, PlacedWidget};
use l_ui::theme::color;

use crate::built_in::common::{StatusTone, caption, rel_time, stat_row, status_text};
use crate::core::types::{
    DataNeed, Permission, RefreshPolicy, SettingField, SettingKind, WidgetCategory, WidgetCx,
    WidgetData, WidgetError, WidgetFactory, WidgetInstance, WidgetManifest,
};

static MANIFEST: std::sync::LazyLock<WidgetManifest> =
    std::sync::LazyLock::new(|| WidgetManifest {
        id: "l.server_uptime",
        name: "Server Uptime",
        description: "Uptime percentage and last check for monitored services.",
        icon: "gauge",
        version: "1.0.0",
        category: WidgetCategory::Monitoring,
        permissions: &[Permission::Network, Permission::LocalStorage],
        default_size: GridSize::new(4, 3),
        min_size: GridSize::new(3, 2),
        max_size: Some(GridSize::new(8, 6)),
        refresh: RefreshPolicy::Interval {
            default_secs: 60,
            min_secs: 15,
        },
        settings: vec![
            SettingField {
                key: "services".into(),
                label: "Services (name=url per line)".into(),
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

/// One monitored service result.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ServiceStatus {
    /// Display name.
    pub name: String,
    /// Target URL.
    pub url: String,
    /// Uptime % over the window.
    pub uptime_pct: f64,
    /// Last check unix seconds.
    pub last_check: i64,
    /// Last check succeeded.
    pub last_ok: bool,
    /// Last HTTP status.
    pub last_status: Option<u16>,
    /// Last latency ms.
    pub last_latency_ms: Option<u64>,
}

/// Payload.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct UptimePayload {
    /// Service results.
    pub services: Vec<ServiceStatus>,
}

pub struct ServerUptimeFactory;

impl WidgetFactory for ServerUptimeFactory {
    fn manifest(&self) -> &'static WidgetManifest {
        &MANIFEST
    }
    fn create(&self, placed: &PlacedWidget) -> Result<Box<dyn WidgetInstance>, WidgetError> {
        Ok(Box::new(ServerUptime {
            placed: placed.clone(),
        }))
    }
}

struct ServerUptime {
    placed: PlacedWidget,
}

impl ServerUptime {
    /// Parse `name=url` lines into (name, url) pairs.
    fn endpoints(&self) -> Vec<(String, String)> {
        let raw = crate::core::types::cfg_string(&self.placed, &MANIFEST, "services");
        raw.lines()
            .filter_map(|l| {
                let l = l.trim();
                if l.is_empty() {
                    return None;
                }
                let (name, url) = l.split_once('=')?;
                let url = url.trim();
                if !url.starts_with("https://") {
                    return None;
                }
                Some((name.trim().to_string(), url.to_string()))
            })
            .collect()
    }
}

impl ServerUptime {
    fn spec_key(&self) -> String {
        self.endpoints()
            .iter()
            .map(|(n, u)| format!("{n}={u}"))
            .collect::<Vec<_>>()
            .join("|")
    }
}

impl WidgetInstance for ServerUptime {
    fn data_needs(&self) -> Vec<DataNeed> {
        vec![DataNeed::UptimeServices {
            spec_key: self.spec_key(),
        }]
    }

    fn refresh_secs(&self) -> Option<u64> {
        crate::core::types::config_get::<u64>(&self.placed, &MANIFEST, "refresh_secs")
    }

    fn on_config_changed(&mut self, config: &serde_json::Value) {
        self.placed.config = config.clone();
    }

    fn ui(&mut self, ui: &mut Ui, _cx: &mut WidgetCx<'_>, data: &WidgetData) {
        let endpoints = self.endpoints();
        if endpoints.is_empty() {
            ui.label(
                RichText::new("configure services as name=https://… in widget settings")
                    .small()
                    .color(color::text_muted()),
            );
            return;
        }
        let Some(payload) = data.decode::<UptimePayload>() else {
            ui.label(
                RichText::new("waiting for probes…")
                    .small()
                    .color(color::text_muted()),
            );
            return;
        };
        caption(ui, "uptime");
        for svc in payload.services.iter().take(8) {
            ui.horizontal(|ui| {
                status_text(
                    ui,
                    if svc.last_ok { "up" } else { "down" },
                    if svc.last_ok {
                        StatusTone::Ok
                    } else {
                        StatusTone::Err
                    },
                );
                ui.label(RichText::new(&svc.name).small().color(color::text()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{:.1}%", svc.uptime_pct))
                            .small()
                            .color(color::text_secondary()),
                    );
                });
            });
            stat_row(
                ui,
                "last check",
                &format!(
                    "{} · {}",
                    rel_time(svc.last_check),
                    svc.last_latency_ms.map_or("—".into(), |l| format!("{l}ms"))
                ),
            );
        }
        if payload.services.is_empty() {
            ui.label(
                RichText::new("no probe data yet")
                    .small()
                    .color(color::text_muted()),
            );
        }
    }
}
