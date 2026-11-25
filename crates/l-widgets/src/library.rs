//! Widget library UI: browse, search, preview, add widgets.

use egui::{RichText, Ui};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use l_core::Action;
use l_core::models::Surface;
use l_ui::components::empty_state::empty_state;
use l_ui::icons::Icon;
use l_ui::theme::color;

use crate::core::registry::WidgetRegistry;
use crate::core::types::{LibraryFilter, WidgetCategory, icon_by_name};

/// Draw the library page content.
pub fn draw_library(
    ui: &mut Ui,
    registry: &WidgetRegistry,
    filter: &mut LibraryFilter,
    search: &mut String,
    dispatch: &mut dyn FnMut(Action),
) {
    // Toolbar: search + category filter.
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(search)
                .hint_text("search widgets…")
                .desired_width(220.0),
        );
        for cat in WidgetCategory::ALL {
            let label = match filter {
                LibraryFilter::Category(c) if *c == cat => {
                    format!("[{cat_label}]", cat_label = cat.label())
                }
                _ => cat.label().to_string(),
            };
            if ui
                .selectable_label(
                    matches!(filter, LibraryFilter::Category(c) if *c == cat),
                    label,
                )
                .clicked()
            {
                *filter = LibraryFilter::Category(cat);
            }
        }
        if ui
            .selectable_label(matches!(filter, LibraryFilter::All), "ALL")
            .clicked()
        {
            *filter = LibraryFilter::All;
        }
    });
    ui.add_space(8.0);

    let manifests = registry.manifests();
    let matcher = SkimMatcherV2::default();
    let mut scored: Vec<_> = manifests
        .into_iter()
        .filter(|m| match filter {
            LibraryFilter::All => true,
            LibraryFilter::Category(c) => m.category == *c,
        })
        .filter_map(|m| {
            if search.trim().is_empty() {
                Some((0, m))
            } else {
                let score = matcher.fuzzy_match(
                    &format!("{} {} {}", m.name, m.id, m.description),
                    search.trim(),
                )?;
                Some((score, m))
            }
        })
        .collect();
    scored.sort_by_key(|(s, _)| -*s);

    if scored.is_empty() {
        empty_state(
            ui,
            Icon::Search,
            "no widgets found",
            "try a different search",
        );
        return;
    }

    // Two-column card list.
    let mut col = 0;
    ui.columns(2, |cols| {
        for (score, m) in scored {
            let cell = &mut cols[col % 2];
            col += 1;
            egui::Frame::window(cell.style())
                .fill(color::bg_secondary())
                .stroke(egui::Stroke::new(1.0, color::border_subtle()))
                .corner_radius(2)
                .inner_margin(egui::Margin::same(10))
                .show(cell, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let icon = icon_by_name(m.icon).unwrap_or(Icon::Box);
                        l_ui::icons::icon_widget(ui, icon, 16.0, color::text_secondary());
                        ui.vertical(|ui| {
                            ui.label(RichText::new(m.name).strong());
                            ui.label(
                                RichText::new(m.category.label())
                                    .size(10.0)
                                    .color(color::text_muted()),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("+ add").clicked() {
                                dispatch(Action::AddWidget {
                                    widget_type: m.id.to_string(),
                                    surface: Surface::Main,
                                });
                            }
                        });
                    });
                    ui.label(
                        RichText::new(m.description)
                            .small()
                            .color(color::text_secondary()),
                    );
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("v{} · {}", m.version, m.id))
                                .size(10.0)
                                .color(color::text_muted()),
                        );
                        if !m.permissions.is_empty() {
                            ui.label(
                                RichText::new(format!("{} perms", m.permissions.len()))
                                    .size(10.0)
                                    .color(color::text_muted()),
                            );
                        }
                        let _ = score;
                    });
                });
            cell.add_space(8.0);
        }
    });
}
