//! Toast rendering (stack of transient messages).

use egui::{Color32, RichText, Ui};

use crate::theme::color;

/// Toast severity (mirrors l-core's `ToastKind` without depending on it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastTone {
    /// Info.
    Info,
    /// Success.
    Success,
    /// Warning.
    Warning,
    /// Error.
    Error,
}

impl ToastTone {
    fn color(self) -> Color32 {
        match self {
            Self::Info => color::text_secondary(),
            Self::Success => color::success(),
            Self::Warning => color::warning(),
            Self::Error => color::error(),
        }
    }
    fn prefix(self) -> &'static str {
        match self {
            Self::Info => "i",
            Self::Success => "+",
            Self::Warning => "!",
            Self::Error => "x",
        }
    }
}

/// Draw toasts bottom-right.
pub fn draw_toasts(ui: &mut Ui, toasts: &[(String, ToastTone, f32)]) {
    if toasts.is_empty() {
        return;
    }
    egui::Area::new(egui::Id::unique("toasts"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -12.0))
        .show(ui.ctx(), |ui| {
            for (text, tone, _age) in toasts {
                egui::Frame::window(ui.style())
                    .fill(color::panel())
                    .stroke(egui::Stroke::new(1.0, color::border()))
                    .corner_radius(2)
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(tone.prefix()).color(tone.color()));
                            ui.label(RichText::new(text).small().color(color::text()));
                        });
                    });
            }
        });
}
