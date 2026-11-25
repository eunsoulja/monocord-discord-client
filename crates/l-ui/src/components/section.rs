//! Section headers and labeled groups.

use egui::Ui;

use crate::theme::{color, section_label};

/// Uppercase section label with optional right-side content.
pub fn section(ui: &mut Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(section_label(title));
    ui.add_space(2.0);
}

/// Labeled row: small muted label + content.
pub fn labeled_row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(120.0, ui.spacing().interact_size.y),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(egui::RichText::new(label).small().color(color::text_muted()));
            },
        );
        add(ui);
    });
}
