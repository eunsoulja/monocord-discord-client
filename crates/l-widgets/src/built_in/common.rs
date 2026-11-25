//! Shared helpers for built-in widgets.

use egui::{Color32, RichText, Ui};

use crate::core::types::WidgetCx;
use l_core::state::WidgetRuntimeState;
use l_ui::theme::color;

/// Stat row: label left, value right.
pub fn stat_row(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).small().color(color::text_muted()));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(value).small().color(color::text()));
        });
    });
}

/// Big number + caption stat block.
pub fn stat_block(ui: &mut Ui, value: &str, caption: &str) {
    ui.vertical(|ui| {
        ui.label(
            RichText::new(value)
                .size(20.0)
                .strong()
                .color(color::accent()),
        );
        ui.label(
            RichText::new(caption.to_uppercase())
                .size(10.0)
                .color(color::text_muted()),
        );
    });
}

/// Section caption inside a widget.
pub fn caption(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text.to_uppercase())
            .size(10.0)
            .color(color::text_muted()),
    );
}

/// Colored status text.
pub fn status_text(ui: &mut Ui, text: &str, tone: StatusTone) {
    ui.label(RichText::new(text).small().color(tone.color()));
}

/// Status tone.
#[derive(Debug, Clone, Copy)]
pub enum StatusTone {
    /// Ok.
    Ok,
    /// Warn.
    Warn,
    /// Err.
    Err,
    /// Neutral.
    Neutral,
}

impl StatusTone {
    /// Color.
    #[must_use]
    pub fn color(self) -> Color32 {
        match self {
            Self::Ok => color::success(),
            Self::Warn => color::warning(),
            Self::Err => color::error(),
            Self::Neutral => color::text_secondary(),
        }
    }
}

/// Map core runtime state to a display label.
#[must_use]
pub fn state_label(s: WidgetRuntimeState) -> &'static str {
    match s {
        WidgetRuntimeState::Loading => "LOADING",
        WidgetRuntimeState::Ready => "READY",
        WidgetRuntimeState::Empty => "NO DATA",
        WidgetRuntimeState::Error => "ERROR",
        WidgetRuntimeState::Disconnected => "DISCONNECTED",
        WidgetRuntimeState::PermissionMissing => "PERMISSION REQUIRED",
    }
}

/// Dispatch helper.
pub fn dispatch(cx: &mut WidgetCx<'_>, action: l_core::Action) {
    (cx.dispatch)(action);
}

/// Format unix seconds as a short relative age.
#[must_use]
pub fn rel_time(ts: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let delta = (now - ts).max(0);
    if delta < 60 {
        format!("{delta}s ago")
    } else if delta < 3600 {
        format!("{}m ago", delta / 60)
    } else if delta < 86400 {
        format!("{}h ago", delta / 3600)
    } else {
        format!("{}d ago", delta / 86400)
    }
}
