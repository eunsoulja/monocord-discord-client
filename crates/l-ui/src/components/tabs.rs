//! Tab strip.

use egui::{RichText, Ui};

use crate::theme::color;

/// A tab in a strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tab {
    /// Stable id.
    pub id: &'static str,
    /// Label.
    pub label: &'static str,
}

/// Draw a tab strip; returns index of clicked tab, if any.
pub fn tab_strip(ui: &mut Ui, tabs: &[Tab], active: usize) -> Option<usize> {
    let mut clicked = None;
    ui.horizontal(|ui| {
        for (i, tab) in tabs.iter().enumerate() {
            let is_active = i == active;
            let text = RichText::new(tab.label).small().color(if is_active {
                color::accent()
            } else {
                color::text_secondary()
            });
            if ui.selectable_label(is_active, text).clicked() {
                clicked = Some(i);
            }
            if is_active {
                // thin underline
                let rect = ui.min_rect();
                ui.painter().hline(
                    rect.left()..=rect.right(),
                    rect.bottom(),
                    egui::Stroke::new(1.0, color::accent()),
                );
            }
        }
    });
    clicked
}
