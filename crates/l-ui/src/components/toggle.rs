//! Toggle switch (square, monochrome).

use egui::{Color32, Ui, Vec2};

use crate::theme::color;

/// Draw a toggle. `value` is updated in place.
pub fn toggle(ui: &mut Ui, value: &mut bool) -> egui::Response {
    let size = Vec2::new(34.0, 18.0);
    let (rect, mut response) = ui.allocate_exact_size(size, egui::Sense::click());
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    let painter = ui.painter();
    let bg = if *value { color::accent() } else { color::border() };
    painter.rect_filled(rect, 2.0, bg);
    let knob_r = 6.0;
    let knob_x = if *value {
        rect.max.x - knob_r - 3.0
    } else {
        rect.min.x + knob_r + 3.0
    };
    painter.circle_filled(
        egui::pos2(knob_x, rect.center().y),
        knob_r,
        if *value {
            color::bg()
        } else {
            color::text_secondary()
        },
    );
    let _ = Color32::WHITE;
    response
}

/// Toggle with label on the right.
pub fn toggle_labeled(ui: &mut Ui, label: &str, value: &mut bool) -> egui::Response {
    ui.horizontal(|ui| {
        let resp = toggle(ui, value);
        ui.label(label);
        resp
    })
    .inner
}
