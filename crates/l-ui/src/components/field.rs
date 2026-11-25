//! Text field with terminal styling.

use egui::{TextEdit, Ui};

use crate::theme::color;

/// A monospace text field with placeholder and optional password masking.
pub fn field(
    ui: &mut Ui,
    id: &str,
    text: &mut String,
    placeholder: &str,
    password: bool,
) -> egui::Response {
    let mut te = TextEdit::singleline(text)
        .id(egui::Id::unique(id))
        .hint_text(placeholder)
        .font(egui::TextStyle::Monospace);
    if password {
        te = te.password(true);
    }
    ui.add(te)
}

/// Multiline field for notes etc.
pub fn multiline(ui: &mut Ui, id: &str, text: &mut String, placeholder: &str) -> egui::Response {
    ui.add(
        TextEdit::multiline(text)
            .id(egui::Id::unique(id))
            .hint_text(placeholder)
            .font(egui::TextStyle::Monospace)
            .desired_width(f32::INFINITY),
    )
}

/// Read-only monospace value display with a label.
pub fn readout(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).small().color(color::text_muted()));
        ui.label(
            egui::RichText::new(value)
                .small()
                .color(color::text_secondary()),
        );
    });
}
