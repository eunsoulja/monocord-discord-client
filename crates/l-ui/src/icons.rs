//! Lucide icon helpers. Icons are font glyphs rendered as text.

use egui::{Color32, FontId, Pos2, Rect, Sense, Ui, Vec2};
/// Re-export for convenience.
pub use lucide_icons::Icon;

/// The glyph string for an icon.
#[must_use]
pub fn glyph(icon: Icon) -> String {
    char::from(icon).to_string()
}

/// Paint an icon centered in `rect`.
pub fn paint_icon(painter: &egui::Painter, icon: Icon, rect: Rect, color: Color32, size: f32) {
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        glyph(icon),
        FontId::new(size, egui::FontFamily::Monospace),
        color,
    );
}

/// Add a non-interactive icon widget.
pub fn icon_widget(ui: &mut Ui, icon: Icon, size: f32, color: Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size + 4.0), Sense::hover());
    paint_icon(ui.painter(), icon, rect, color, size);
    response
}

/// An icon that is also a button.
pub fn icon_button(
    ui: &mut Ui,
    icon: Icon,
    size: f32,
    color: Color32,
    sense: Sense,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size + 8.0), sense);
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, 2.0, crate::theme::color::hover());
    }
    if response.clicked() {
        ui.painter().rect_stroke(
            rect,
            2.0,
            egui::Stroke::new(1.0, crate::theme::color::accent()),
            egui::StrokeKind::Inside,
        );
    }
    paint_icon(
        ui.painter(),
        icon,
        rect,
        if response.hovered() {
            crate::theme::color::accent()
        } else {
            color
        },
        size,
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "icon"));
    response
}

/// Icon + label horizontal row helper.
pub fn icon_label(ui: &mut Ui, icon: Icon, text: &str, color: Color32, size: f32) {
    ui.horizontal(|ui| {
        icon_widget(ui, icon, size, color);
        ui.label(egui::RichText::new(text).size(size).color(color));
    });
}

/// Convenience: a positioned icon at a point.
pub fn icon_at(painter: &egui::Painter, icon: Icon, center: Pos2, color: Color32, size: f32) {
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        glyph(icon),
        FontId::new(size, egui::FontFamily::Monospace),
        color,
    );
}
