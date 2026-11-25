//! Widget chrome renderer: frame, title bar, states, error boundary.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Instant;

use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use l_core::models::{PlacedWidget, Surface, WidgetInstanceId};
use l_core::state::WidgetRuntimeState;
use l_core::{Action, AppState};
use l_ui::components::badge::Tone;
use l_ui::components::empty_state::empty_state;
use l_ui::icons::Icon;
use l_ui::theme::color;

use crate::core::manager::ManagedInstance;
use crate::core::types::{IntensityHint, WidgetCx, WidgetData, WidgetManifest, icon_by_name};

/// Context the renderer needs from the app.
pub struct RenderCx<'a> {
    /// App state (read-only).
    pub state: &'a AppState,
    /// Dispatch channel.
    pub dispatch: &'a mut dyn FnMut(Action),
    /// Animation intensity.
    pub animation: IntensityHint,
    /// Layout-edit mode: show move/resize affordances.
    pub edit_mode: bool,
    /// Called when the user picks "configure" on the overflow menu.
    pub on_configure: &'a mut dyn FnMut(WidgetInstanceId),
    /// Called when a drag on the title bar moves.
    pub on_drag: &'a mut dyn FnMut(WidgetInstanceId, egui::Vec2),
    /// Called when the resize handle is dragged (pixels delta).
    pub on_resize: &'a mut dyn FnMut(WidgetInstanceId, f32, f32),
}

/// Draw one widget: chrome + content. `rect` is the allocated cell area in
/// points. Returns the response for the whole widget (drag detection).
pub fn draw_widget(
    ui: &mut Ui,
    placed: &PlacedWidget,
    manifest: Option<&WidgetManifest>,
    managed: Option<&mut ManagedInstance>,
    data: &WidgetData,
    cx: &mut RenderCx<'_>,
) -> egui::Response {
    let rect = ui.available_rect_before_wrap();
    let (rect, response) = ui.allocate_exact_size(rect.size(), Sense::click_and_drag());

    // Frame.
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, 2.0, color::panel());
    painter.rect_stroke(
        rect,
        2.0,
        Stroke::new(1.0, color::border_subtle()),
        egui::StrokeKind::Inside,
    );

    let title_h = 26.0;
    let title_rect = Rect::from_min_size(rect.min, Vec2::new(rect.width(), title_h));
    painter.rect_filled(title_rect, 2.0, color::bg_secondary());
    painter.hline(
        title_rect.left()..=title_rect.right(),
        title_rect.bottom(),
        Stroke::new(1.0, color::border_subtle()),
    );

    // Title content.
    let mut tx = title_rect.left() + 8.0;
    let ty = title_rect.center().y;
    if let Some(icon) = manifest.and_then(|m| icon_by_name(m.icon)) {
        painter.text(
            Pos2::new(tx, ty),
            egui::Align2::LEFT_CENTER,
            l_ui::icons::glyph(icon),
            egui::FontId::new(13.0, egui::FontFamily::Monospace),
            color::text_secondary(),
        );
        tx += 18.0;
    }
    let name = if placed.name.is_empty() {
        manifest.map_or(placed.widget_type.as_str(), |m| m.name)
    } else {
        placed.name.as_str()
    };
    painter.text(
        Pos2::new(tx, ty),
        egui::Align2::LEFT_CENTER,
        name,
        egui::FontId::new(12.0, egui::FontFamily::Monospace),
        if placed.enabled {
            color::text()
        } else {
            color::text_muted()
        },
    );

    // State badge (right side, before buttons).
    let state = managed
        .as_ref()
        .map_or(WidgetRuntimeState::Loading, |m| m.state);
    let err = managed
        .as_ref()
        .and_then(|m| m.error.clone())
        .unwrap_or_default();
    draw_state_badge(
        ui,
        title_rect,
        state,
        if err.is_empty() {
            None
        } else {
            Some(err.as_str())
        },
    );

    // Title-bar buttons.
    draw_title_buttons(ui, placed, title_rect, cx);

    // Edit-mode affordances.
    if cx.edit_mode {
        painter.rect_stroke(
            rect,
            2.0,
            Stroke::new(1.0, color::accent()),
            egui::StrokeKind::Inside,
        );
        // Resize handle bottom-right.
        let handle = Rect::from_min_size(
            rect.right_bottom() - Vec2::new(14.0, 14.0),
            Vec2::splat(14.0),
        );
        let hr = ui.interact(handle, ui.scope_id().with(("rz", placed.id)), Sense::drag());
        if hr.dragged() {
            let d = hr.drag_delta();
            (cx.on_resize)(placed.id, d.x, d.y);
        }
        painter.text(
            handle.center(),
            egui::Align2::CENTER_CENTER,
            "◢",
            egui::FontId::new(10.0, egui::FontFamily::Monospace),
            color::text_muted(),
        );
        // Drag anywhere on title bar moves the widget.
        let title_resp = ui.interact(
            title_rect,
            ui.scope_id().with(("mv", placed.id)),
            Sense::drag(),
        );
        if title_resp.dragged() {
            (cx.on_drag)(placed.id, title_resp.drag_delta());
        }
    }

    // Content area.
    let content_rect = Rect::from_min_max(
        Pos2::new(rect.left() + 8.0, rect.top() + title_h + 4.0),
        Pos2::new(rect.right() - 8.0, rect.bottom() - 6.0),
    );
    if content_rect.width() > 8.0 && content_rect.height() > 8.0 {
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(content_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(content_rect);
        draw_content(&mut child, placed, managed, data, cx, state);
    }

    response
}

fn draw_state_badge(ui: &mut Ui, title_rect: Rect, state: WidgetRuntimeState, error: Option<&str>) {
    let (label, tone) = match state {
        WidgetRuntimeState::Loading => ("…", Tone::Neutral),
        WidgetRuntimeState::Ready => ("", Tone::Neutral),
        WidgetRuntimeState::Empty => ("empty", Tone::Neutral),
        WidgetRuntimeState::Error => ("error", Tone::Error),
        WidgetRuntimeState::Disconnected => ("offline", Tone::Warning),
        WidgetRuntimeState::PermissionMissing => ("perm", Tone::Warning),
    };
    if label.is_empty() {
        return;
    }
    let w = label.len() as f32 * 6.0 + 10.0;
    let rect = Rect::from_min_size(
        Pos2::new(title_rect.right() - w - 50.0, title_rect.top() + 4.0),
        Vec2::new(w, 18.0),
    );
    ui.painter().rect_stroke(
        rect,
        2.0,
        Stroke::new(1.0, tone_color(tone)),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(10.0),
        tone_color(tone),
    );
    if let Some(e) = error {
        ui.painter().text(
            rect.left_bottom() - Vec2::new(0.0, -2.0),
            egui::Align2::LEFT_TOP,
            e,
            egui::FontId::proportional(9.0),
            color::text_muted(),
        );
    }
}

fn tone_color(t: Tone) -> Color32 {
    match t {
        Tone::Neutral => color::text_muted(),
        Tone::Success => color::success(),
        Tone::Warning => color::warning(),
        Tone::Error => color::error(),
        Tone::Accent => color::accent(),
    }
}

/// A small square icon button in the title bar: hover fill + glyph.
fn title_icon_button(
    ui: &mut Ui,
    id: egui::Id,
    rect: Rect,
    icon: Icon,
    tip: &str,
) -> egui::Response {
    let r = ui.interact(rect, id, Sense::click());
    if r.hovered() {
        ui.painter().rect_filled(rect, 3.0, color::hover());
    }
    l_ui::icons::paint_icon(
        ui.painter(),
        icon,
        rect,
        if r.hovered() {
            color::text()
        } else {
            color::text_secondary()
        },
        12.0,
    );
    r.on_hover_text(tip)
}

fn draw_title_buttons(ui: &mut Ui, placed: &PlacedWidget, title_rect: Rect, cx: &mut RenderCx<'_>) {
    let cy = title_rect.center().y;
    let cell = Vec2::splat(18.0);
    // Right-aligned cells with a 2px gap; overflow menu is rightmost.
    let mut right = title_rect.right() - 6.0;
    let menu_rect = Rect::from_center_size(Pos2::new(right - 9.0, cy), cell);
    right -= 20.0;
    let rf_rect = Rect::from_center_size(Pos2::new(right - 9.0, cy), cell);

    let menu_resp = title_icon_button(
        ui,
        ui.scope_id().with(("ovf", placed.id)),
        menu_rect,
        Icon::MoreHorizontal,
        "widget options",
    );
    let rf = title_icon_button(
        ui,
        ui.scope_id().with(("rf", placed.id)),
        rf_rect,
        Icon::RefreshCw,
        "refresh",
    );
    if rf.clicked() {
        (cx.dispatch)(Action::RefreshWidget(placed.id));
    }

    egui::Popup::menu(&menu_resp).show(|ui| {
        if ui.button("configure…").clicked() {
            (cx.on_configure)(placed.id);
            ui.close();
        }
        if ui
            .button(if placed.enabled { "disable" } else { "enable" })
            .clicked()
        {
            (cx.dispatch)(Action::ToggleWidgetEnabled(placed.id));
            ui.close();
        }
        if ui.button("duplicate").clicked() {
            (cx.dispatch)(Action::DuplicateWidget(placed.id));
            ui.close();
        }
        ui.separator();
        if ui.button("move to dock").clicked() {
            (cx.dispatch)(Action::MoveWidgetToSurface {
                id: placed.id,
                surface: Surface::Dock,
            });
            ui.close();
        }
        if ui.button("move to workspace").clicked() {
            (cx.dispatch)(Action::MoveWidgetToSurface {
                id: placed.id,
                surface: Surface::Main,
            });
            ui.close();
        }
        ui.separator();
        if ui.button("reset config").clicked() {
            (cx.dispatch)(Action::ResetWidgetConfig(placed.id));
            ui.close();
        }
        if ui.button("remove widget").clicked() {
            (cx.dispatch)(Action::RemoveWidget(placed.id));
            ui.close();
        }
    });
}

fn draw_content(
    ui: &mut Ui,
    placed: &PlacedWidget,
    managed: Option<&mut ManagedInstance>,
    data: &WidgetData,
    cx: &mut RenderCx<'_>,
    state: WidgetRuntimeState,
) {
    match state {
        WidgetRuntimeState::Loading => {
            loading(ui, cx.animation);
        }
        WidgetRuntimeState::Empty => {
            empty_state(ui, Icon::Inbox, "no data", "");
        }
        WidgetRuntimeState::Disconnected => {
            empty_state(
                ui,
                Icon::WifiOff,
                "disconnected",
                "connect an integration to see data",
            );
        }
        WidgetRuntimeState::PermissionMissing => {
            empty_state(
                ui,
                Icon::Lock,
                "permission required",
                "grant access in settings",
            );
        }
        WidgetRuntimeState::Error => {
            let msg = managed
                .and_then(|m| m.error.clone())
                .unwrap_or_else(|| "unknown error".into());
            empty_state(ui, Icon::TriangleAlert, "widget error", &msg);
        }
        WidgetRuntimeState::Ready => {
            let Some(m) = managed else { return };
            if !placed.enabled {
                ui.label(
                    egui::RichText::new("disabled")
                        .small()
                        .color(color::text_muted()),
                );
                return;
            }
            // Error boundary: a panicking widget shows a placeholder, never
            // takes down the app.
            let mut widget_cx = WidgetCx {
                dispatch: cx.dispatch,
                state: cx.state,
                animation: cx.animation,
            };
            let started = Instant::now();
            let _ = started;
            let result = catch_unwind(AssertUnwindSafe(|| {
                m.instance.ui(ui, &mut widget_cx, data);
            }));
            if result.is_err() {
                ui.label(
                    egui::RichText::new("widget crashed — reload")
                        .small()
                        .color(color::error()),
                );
            }
        }
    }
}

fn loading(ui: &mut Ui, intensity: IntensityHint) {
    let t = ui.input(|i| i.time) as f32;
    let speed = match intensity {
        IntensityHint::Off => 0.0,
        IntensityHint::Subtle => 2.0,
        IntensityHint::Full => 4.0,
    };
    let phase = if speed > 0.0 { (t * speed) % 4.0 } else { 0.0 };
    ui.horizontal(|ui| {
        for i in 0..4 {
            let on = ((i as f32) - phase).abs() < 0.9 || ((i as f32) - phase + 4.0).abs() < 0.9;
            ui.label(egui::RichText::new("▪").color(if on {
                color::text()
            } else {
                color::text_muted()
            }));
        }
        ui.label(
            egui::RichText::new("loading")
                .small()
                .color(color::text_muted()),
        );
    });
}
