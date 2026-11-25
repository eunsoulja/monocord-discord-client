//! Modal dialog helper (egui Area + centered panel).

use egui::{Align2, Color32, Frame, Id, Order, Ui, Vec2};

use crate::theme::{color, space};

/// Show a modal overlay. `content` is called with a child Ui inside the panel.
/// Close is signaled by the caller (e.g. via a bool in captured state).
pub fn modal<R>(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    width: f32,
    content: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let backdrop = ctx.viewport_rect();
    ctx.layer_painter(egui::LayerId::new(
        Order::Foreground,
        Id::unique(format!("{id}-dim")),
    ))
    .rect_filled(backdrop, 0.0, Color32::from_black_alpha(170));

    let mut result = None;
    egui::Area::new(Id::unique(id))
        .order(Order::Foreground)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .interactable(true)
        .show(ctx, |ui| {
            Frame::window(ui.style())
                .fill(color::panel())
                .stroke(egui::Stroke::new(1.0, color::border()))
                .corner_radius(2)
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    ui.set_width(width);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(title.to_uppercase()).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("x").clicked() {
                                ui.close();
                            }
                        });
                    });
                    ui.add_space(f32::from(space::SM as i8));
                    result = Some(content(ui));
                });
        });
    result
}
