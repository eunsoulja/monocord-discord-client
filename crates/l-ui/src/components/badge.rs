//! Small badge / status chip.

use egui::{Color32, RichText, Ui};

use crate::theme::color;

/// Badge tone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Neutral.
    Neutral,
    /// Success.
    Success,
    /// Warning.
    Warning,
    /// Error.
    Error,
    /// Accent (white).
    Accent,
}

impl Tone {
    fn color(self) -> Color32 {
        match self {
            Self::Neutral => color::text_secondary(),
            Self::Success => color::success(),
            Self::Warning => color::warning(),
            Self::Error => color::error(),
            Self::Accent => color::accent(),
        }
    }
}

/// Draw a small uppercase badge.
pub fn badge(ui: &mut Ui, text: &str, tone: Tone) {
    let c = tone.color();
    let galley =
        ui.painter()
            .layout_no_wrap(text.to_uppercase(), egui::FontId::proportional(10.0), c);
    let size = galley.size() + egui::vec2(10.0, 4.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect_stroke(
        rect,
        2.0,
        egui::Stroke::new(1.0, c),
        egui::StrokeKind::Inside,
    );
    ui.painter()
        .galley(rect.min + egui::vec2(5.0, 2.0), galley, c);
}

/// Colored status dot.
pub fn status_dot(ui: &mut Ui, tone: Tone, tooltip: &str) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.5, tone.color());
    resp.on_hover_text(tooltip);
}

/// Rich text in a tone color.
#[must_use]
pub fn toned(text: &str, tone: Tone) -> RichText {
    RichText::new(text).color(tone_color_pub(tone))
}

fn tone_color_pub(tone: Tone) -> Color32 {
    tone.color()
}
