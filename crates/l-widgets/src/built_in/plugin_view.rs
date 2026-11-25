//! Generic widget that renders data polled from an external plugin view.
//!
//! Plugin manifests (discovered by `l-plugins`) declare widget views; the app
//! registers one `PluginViewFactory` per view. The plugin returns a JSON view
//! model on each `poll`; this widget renders it. Supported schema:
//!
//! ```json
//! {
//!   "title": "optional heading",
//!   "text": "free-form body text",
//!   "metrics": [{"label": "CPU", "value": "42%", "tone": "ok"}],
//!   "rows":    [{"label": "uptime", "value": "3d", "tone": "muted"}],
//!   "progress": {"label": "disk", "value": 0.62, "max": 1.0}
//! }
//! ```
//!
//! `tone` is one of `ok`, `warn`, `err`, `accent`, `muted` (default `muted`).

use egui::{RichText, Ui};
use l_core::models::PlacedWidget;
use l_ui::theme::color;
use serde_json::Value;

use crate::core::types::{
    DataNeed, WidgetCx, WidgetData, WidgetError, WidgetFactory, WidgetInstance, WidgetManifest,
};

/// Factory for one plugin-declared widget view.
pub struct PluginViewFactory {
    manifest: &'static WidgetManifest,
    plugin: String,
    view: String,
    refresh_secs: u64,
}

impl PluginViewFactory {
    /// Create a factory bound to a plugin view.
    #[must_use]
    pub fn new(
        manifest: &'static WidgetManifest,
        plugin: String,
        view: String,
        refresh_secs: u64,
    ) -> Self {
        Self {
            manifest,
            plugin,
            view,
            refresh_secs,
        }
    }
}

impl WidgetFactory for PluginViewFactory {
    fn manifest(&self) -> &'static WidgetManifest {
        self.manifest
    }

    fn create(&self, placed: &PlacedWidget) -> Result<Box<dyn WidgetInstance>, WidgetError> {
        Ok(Box::new(PluginViewInstance {
            plugin: self.plugin.clone(),
            view: self.view.clone(),
            config: placed.config.clone(),
            refresh_secs: self.refresh_secs,
        }))
    }
}

struct PluginViewInstance {
    plugin: String,
    view: String,
    config: Value,
    refresh_secs: u64,
}

impl WidgetInstance for PluginViewInstance {
    fn data_needs(&self) -> Vec<DataNeed> {
        let params = serde_json::to_string(&self.config).unwrap_or_else(|_| "{}".into());
        vec![DataNeed::Plugin {
            plugin: self.plugin.clone(),
            view: self.view.clone(),
            params,
        }]
    }

    fn on_config_changed(&mut self, config: &Value) {
        self.config = config.clone();
    }

    fn refresh_secs(&self) -> Option<u64> {
        Some(self.refresh_secs.max(5))
    }

    fn ui(&mut self, ui: &mut Ui, _cx: &mut WidgetCx<'_>, data: &WidgetData) {
        if data.is_empty() {
            ui.label(RichText::new("waiting…").small().color(color::text_muted()));
            return;
        }
        let v = data.json.clone();
        if let Some(title) = v.get("title").and_then(Value::as_str)
            && !title.is_empty()
        {
            ui.label(RichText::new(title).small().strong().color(color::text()));
        }
        if let Some(text) = v.get("text").and_then(Value::as_str)
            && !text.is_empty()
        {
            ui.label(RichText::new(text).small().color(color::text_secondary()));
        }
        if let Some(metrics) = v.get("metrics").and_then(Value::as_array) {
            for m in metrics {
                let label = m.get("label").and_then(Value::as_str).unwrap_or("");
                let value = m
                    .get("value")
                    .map_or_else(String::new, value_to_string);
                let tone = m.get("tone").and_then(Value::as_str).unwrap_or("muted");
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).small().color(color::text_muted()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(value)
                                .small()
                                .color(tone_color(tone)),
                        );
                    });
                });
            }
        }
        if let Some(rows) = v.get("rows").and_then(Value::as_array) {
            for r in rows {
                let label = r.get("label").and_then(Value::as_str).unwrap_or("");
                let value = r
                    .get("value")
                    .map_or_else(String::new, value_to_string);
                let tone = r.get("tone").and_then(Value::as_str).unwrap_or("muted");
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).small().color(color::text_muted()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(value)
                                .small()
                                .color(tone_color(tone)),
                        );
                    });
                });
            }
        }
        if let Some(p) = v.get("progress") {
            let label = p.get("label").and_then(Value::as_str).unwrap_or("progress");
            let value = p.get("value").and_then(Value::as_f64).unwrap_or(0.0);
            let max = p.get("max").and_then(Value::as_f64).unwrap_or(1.0);
            let frac = if max > 0.0 {
                (value / max).clamp(0.0, 1.0) as f32
            } else {
                0.0
            };
            let tone = p.get("tone").and_then(Value::as_str).unwrap_or("accent");
            ui.horizontal(|ui| {
                ui.label(RichText::new(label).small().color(color::text_muted()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{:.0}%", frac * 100.0))
                            .small()
                            .color(tone_color(tone)),
                    );
                });
            });
            let bar = ui.available_width();
            let (rect, _) = ui.allocate_exact_size(egui::vec2(bar, 4.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 1.0, color::bg_secondary());
            let fill = egui::Rect::from_min_size(rect.min, egui::vec2(bar * frac, 4.0));
            ui.painter().rect_filled(fill, 1.0, tone_color(tone));
        }
    }
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => v.to_string(),
    }
}

fn tone_color(tone: &str) -> egui::Color32 {
    match tone {
        "ok" | "success" => color::success(),
        "warn" | "warning" => color::warning(),
        "err" | "error" => color::error(),
        "accent" => color::accent(),
        _ => color::text_secondary(),
    }
}
