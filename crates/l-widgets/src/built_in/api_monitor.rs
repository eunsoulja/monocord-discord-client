//! API Monitor widget: probes user-configured HTTPS endpoints.

use egui::{RichText, Ui};

use l_core::models::{GridSize, PlacedWidget};
use l_ui::theme::color;

use crate::built_in::common::{StatusTone, caption, rel_time, stat_row, status_text};
use crate::core::types::{
    DataNeed, Permission, RefreshPolicy, SettingField, SettingKind, WidgetCategory, WidgetCx,
    WidgetData, WidgetError, WidgetFactory, WidgetInstance, WidgetManifest, cfg_string, config_get,
};

static MANIFEST: std::sync::LazyLock<WidgetManifest> =
    std::sync::LazyLock::new(|| WidgetManifest {
        id: "l.api_monitor",
        name: "API Monitor",
        description: "Response status and latency for custom endpoints.",
        icon: "network",
        version: "1.0.0",
        category: WidgetCategory::Developer,
        permissions: &[Permission::Network],
        default_size: GridSize::new(4, 3),
        min_size: GridSize::new(3, 2),
        max_size: Some(GridSize::new(8, 6)),
        refresh: RefreshPolicy::Interval {
            default_secs: 60,
            min_secs: 15,
        },
        settings: vec![
            SettingField {
                key: "endpoints".into(),
                label: "Endpoints (name=https://… per line)".into(),
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

/// One probe result.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ProbeResult {
    /// Endpoint name.
    pub name: String,
    /// URL.
    pub url: String,
    /// HTTP status or 0 on transport error.
    pub status: u16,
    /// Latency ms.
    pub latency_ms: u64,
    /// Last success unix seconds.
    pub last_ok: i64,
    /// Whether last probe succeeded.
    pub ok: bool,
}

/// Payload.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ApiPayload {
    /// Probe results.
    pub results: Vec<ProbeResult>,
}

pub struct ApiMonitorFactory;

impl WidgetFactory for ApiMonitorFactory {
    fn manifest(&self) -> &'static WidgetManifest {
        &MANIFEST
    }
    fn create(&self, placed: &PlacedWidget) -> Result<Box<dyn WidgetInstance>, WidgetError> {
        Ok(Box::new(ApiWidget {
            placed: placed.clone(),
        }))
    }
}

struct ApiWidget {
    placed: PlacedWidget,
}

impl ApiWidget {
    fn endpoints(&self) -> Vec<(String, String)> {
        let raw = cfg_string(&self.placed, &MANIFEST, "endpoints");
        raw.lines()
            .filter_map(|l| {
                let (name, url) = l.trim().split_once('=')?;
                let url = url.trim();
                if !url.starts_with("https://") || url.len() > 512 {
                    return None;
                }
                Some((name.trim().to_string(), url.to_string()))
            })
            .take(12)
            .collect()
    }

    fn endpoints_key(&self) -> String {
        // Encode name=url pairs so the fetcher can reconstruct endpoints.
        self.endpoints()
            .iter()
            .map(|(n, u)| format!("{n}={u}"))
            .collect::<Vec<_>>()
            .join("|")
    }
}

impl WidgetInstance for ApiWidget {
    fn data_needs(&self) -> Vec<DataNeed> {
        vec![DataNeed::ApiProbes {
            endpoints_key: self.endpoints_key(),
        }]
    }

    fn refresh_secs(&self) -> Option<u64> {
        config_get::<u64>(&self.placed, &MANIFEST, "refresh_secs")
    }

    fn on_config_changed(&mut self, config: &serde_json::Value) {
        self.placed.config = config.clone();
    }

    fn ui(&mut self, ui: &mut Ui, _cx: &mut WidgetCx<'_>, data: &WidgetData) {
        if self.endpoints().is_empty() {
            ui.label(
                RichText::new("configure endpoints as name=https://… in settings")
                    .small()
                    .color(color::text_muted()),
            );
            return;
        }
        let Some(payload) = data.decode::<ApiPayload>() else {
            ui.label(RichText::new("probing…").small().color(color::text_muted()));
            return;
        };
        caption(ui, "endpoints");
        for r in payload.results.iter().take(10) {
            ui.horizontal(|ui| {
                status_text(
                    ui,
                    if r.ok { "ok" } else { "fail" },
                    if r.ok {
                        StatusTone::Ok
                    } else {
                        StatusTone::Err
                    },
                );
                ui.label(RichText::new(&r.name).small().color(color::text()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} · {}ms", r.status, r.latency_ms))
                            .small()
                            .color(color::text_secondary()),
                    );
                });
            });
            stat_row(ui, "last ok", &rel_time(r.last_ok));
        }
        if payload.results.is_empty() {
            ui.label(
                RichText::new("no results yet")
                    .small()
                    .color(color::text_muted()),
            );
        }
    }
}
