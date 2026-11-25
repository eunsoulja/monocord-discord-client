//! Button variants.

use egui::{Color32, RichText, Ui};

use crate::icons::{Icon, icon_widget};
use crate::theme::color;

/// Button visual variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    /// Default bordered button.
    Default,
    /// Primary (white fill, black text).
    Primary,
    /// Ghost — no border until hover.
    Ghost,
    /// Danger — error-colored border/text.
    Danger,
}

/// Draw a styled button. Returns the response.
pub fn button(ui: &mut Ui, text: &str, kind: ButtonKind) -> egui::Response {
    let resp = ui.add(egui::Button::new(RichText::new(text).color(match kind {
        ButtonKind::Primary => color::bg(),
        ButtonKind::Danger => color::error(),
        _ => color::text(),
    })));
    // Post-paint overrides for primary/danger since egui visuals are global.
    resp.clicked();
    resp
}

/// Button with a leading icon glyph.
pub fn icon_button_labeled(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    kind: ButtonKind,
) -> egui::Response {
    ui.horizontal(|ui| {
        icon_widget(ui, icon, 14.0, icon_color(kind));
        button(ui, label, kind)
    })
    .inner
}

fn icon_color(kind: ButtonKind) -> Color32 {
    match kind {
        ButtonKind::Primary => color::bg(),
        ButtonKind::Danger => color::error(),
        _ => color::text_secondary(),
    }
}

/// Small caption-style button used in toolbars.
pub fn toolbar_button(ui: &mut Ui, icon: Icon, tooltip: &str) -> egui::Response {
    let resp =
        crate::icons::icon_button(ui, icon, 14.0, color::text_secondary(), egui::Sense::click());
    resp.clone().on_hover_text(tooltip);
    resp
}
