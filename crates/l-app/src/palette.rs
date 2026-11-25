//! Command palette overlay (Ctrl+K).

use egui::RichText;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use l_core::actions::Action;
use l_core::commands::{CommandDef, builtin_commands};
use l_core::models::Page;
use l_ui::theme::color;

use crate::app::LApp;

struct Item {
    title: String,
    category: &'static str,
    action: Action,
}

fn collect_items(app: &LApp) -> Vec<Item> {
    let mut items: Vec<Item> = builtin_commands()
        .into_iter()
        .map(|c: CommandDef| Item {
            title: c.title.to_string(),
            category: c.category.header(),
            action: c.action,
        })
        .collect();

    // Dynamic: add each widget type.
    for m in app.registry.manifests() {
        items.push(Item {
            title: format!("add widget: {}", m.name),
            category: "WIDGETS",
            action: Action::AddWidget {
                widget_type: m.id.to_string(),
                surface: l_core::models::Surface::Main,
            },
        });
    }
    // Dynamic: plugin-provided commands.
    let plugin_names: std::collections::HashMap<String, String> = app
        .plugin_host()
        .plugins()
        .into_iter()
        .map(|p| (p.manifest.id.clone(), p.manifest.name.clone()))
        .collect();
    for (plugin_id, cmd) in app.plugin_host().commands() {
        let plugin_name = plugin_names
            .get(&plugin_id)
            .cloned()
            .unwrap_or_else(|| plugin_id.clone());
        items.push(Item {
            title: format!("{}: {}", plugin_name, cmd.name),
            category: "PLUGINS",
            action: Action::PluginCommand {
                plugin: plugin_id.clone(),
                command: cmd.id.clone(),
            },
        });
    }
    // Dynamic: select guilds.
    for g in &app.state.discord.guilds {
        items.push(Item {
            title: format!("guild: {}", g.name),
            category: "DISCORD",
            action: Action::SelectGuild(Some(g.id.clone())),
        });
    }
    // Dynamic: apply presets.
    for p in l_widgets::presets::PRESETS {
        items.push(Item {
            title: format!("apply preset: {p}"),
            category: "WORKSPACE",
            action: Action::ApplyPreset(p.to_string()),
        });
    }
    // Navigate to guild workspace.
    items.push(Item {
        title: "open server workspace".into(),
        category: "NAVIGATION",
        action: Action::Navigate(Page::ServerWorkspace),
    });
    items
}

/// Draw the palette overlay if open.
pub fn draw_palette(app: &mut LApp, ctx: &egui::Context) {
    if !app.state.ui.palette_open {
        return;
    }
    let items = collect_items(app);
    let query = app.palette_query.clone();
    let matcher = SkimMatcherV2::default();
    let mut scored: Vec<(i64, usize)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, it)| {
            if query.trim().is_empty() {
                Some((0, i))
            } else {
                matcher
                    .fuzzy_match(
                        &format!("{} {} {}", it.title, it.category, ""),
                        query.trim(),
                    )
                    .map(|s| (s, i))
            }
        })
        .collect();
    scored.sort_by_key(|(s, _)| -*s);
    let filtered: Vec<usize> = scored.into_iter().take(30).map(|(_, i)| i).collect();

    // Clamp selection.
    if app.palette_sel >= filtered.len() {
        app.palette_sel = filtered.len().saturating_sub(1);
    }

    // Keyboard nav.
    let (up, down, enter) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::Enter),
        )
    });
    if up && app.palette_sel > 0 {
        app.palette_sel -= 1;
    }
    if down && app.palette_sel + 1 < filtered.len() {
        app.palette_sel += 1;
    }
    if enter && !filtered.is_empty() {
        let idx = filtered[app.palette_sel];
        let action = items[idx].action.clone();
        app.queue(Action::ClosePalette);
        app.palette_query.clear();
        app.queue(action);
    }

    // Draw overlay.
    let screen = ctx.viewport_rect();
    ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::unique("palette-dim"),
    ))
    .rect_filled(screen, 0.0, egui::Color32::from_black_alpha(160));

    egui::Area::new(egui::Id::unique("palette"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 90.0))
        .show(ctx, |ui| {
            egui::Frame::window(ui.style())
                .fill(color::panel())
                .stroke(egui::Stroke::new(1.0, color::border()))
                .corner_radius(2)
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(560.0);
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut app.palette_query)
                            .hint_text("type a command…")
                            .desired_width(f32::INFINITY),
                    );
                    if resp.changed() {
                        app.palette_sel = 0;
                    }
                    if resp.has_focus() {
                        // keep focus
                    }
                    ui.add_space(4.0);
                    let mut last_cat = "";
                    for (row, &idx) in filtered.iter().enumerate() {
                        let it = &items[idx];
                        if it.category != last_cat {
                            last_cat = it.category;
                            ui.label(
                                RichText::new(it.category)
                                    .size(9.0)
                                    .color(color::text_muted()),
                            );
                        }
                        let selected = row == app.palette_sel;
                        let r = ui.selectable_label(
                            selected,
                            RichText::new(&it.title).small().color(if selected {
                                color::accent()
                            } else {
                                color::text()
                            }),
                        );
                        if r.clicked() {
                            let action = it.action.clone();
                            app.queue(Action::ClosePalette);
                            app.palette_query.clear();
                            app.queue(action);
                        }
                    }
                    if filtered.is_empty() {
                        ui.label(RichText::new("no matches").small().color(color::text_muted()));
                    }
                });
        });
}
