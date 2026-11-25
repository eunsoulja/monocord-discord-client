//! Empty state placeholder.

use egui::{RichText, Ui};

use crate::icons::{Icon, icon_widget};
use crate::theme::color;

/// Centered empty state with icon, title, and hint.
pub fn empty_state(ui: &mut Ui, icon: Icon, title: &str, hint: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(12.0);
        icon_widget(ui, icon, 22.0, color::text_muted());
        ui.add_space(8.0);
        ui.label(RichText::new(title).color(color::text_secondary()));
        if !hint.is_empty() {
            ui.label(RichText::new(hint).small().color(color::text_muted()));
        }
        ui.add_space(12.0);
    });
}
