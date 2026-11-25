//! App shell chrome — Discord-client layout (Legcord-inspired):
//! slim titlebar, server rail with pill indicators, channel sidebar with a
//! bottom user panel, and the widget dock. No bottom status bar; status
//! info lives in the titlebar.

use egui::{Align2, FontId, Rect, RichText, Sense, Ui, pos2, vec2};
use l_core::actions::Action;
use l_core::models::{ChannelEntry, GuildInfo, Page};
use l_core::state::ToastKind;
use l_ui::components::toast::ToastTone;
use l_ui::icons::{self, Icon};
use l_ui::theme::color;

use crate::app::LApp;

/// Guild icon size in the rail.
const GUILD_ICON: f32 = 48.0;
/// Sidebar row height (Discord channels are ~34px incl. padding).
const ROW_H: f32 = 34.0;

fn page_icon(page: Page) -> Icon {
    match page {
        Page::ServerWorkspace => Icon::Server,
        Page::ChannelInfo => Icon::Hash,
        Page::WidgetLibrary => Icon::Boxes,
        Page::Activity => Icon::Activity,
        Page::Notifications => Icon::Bell,
        Page::BotManagement => Icon::Bot,
        Page::Settings => Icon::Settings,
    }
}

// -- Titlebar ---------------------------------------------------------------

/// Slim top bar: brand + context on the left, status + actions on the right.
pub fn draw_titlebar(app: &mut LApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        // Brand mark: accent rounded square with an "M".
        let (mark, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
        ui.painter().rect_filled(mark, 6.0, color::accent());
        ui.painter().text(
            mark.center(),
            Align2::CENTER_CENTER,
            "M",
            FontId::new(13.0, egui::FontFamily::Monospace),
            color::bg(),
        );
        ui.add_space(8.0);
        ui.label(
            RichText::new(&app.state.prefs.workspace_name)
                .small()
                .strong()
                .color(color::text_secondary()),
        );
        ui.add_space(10.0);
        ui.label(RichText::new("/").small().color(color::text_muted()));
        ui.add_space(10.0);
        // Context: current page, or guild/channel when inside a server.
        let page = app.state.ui.page.unwrap_or(Page::ServerWorkspace);
        let context = context_label(app, page);
        ui.label(RichText::new(context).small().color(color::text()));

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if toolbar_btn(ui, Icon::MessageSquare, "open discord client").clicked() {
                app.queue(Action::OpenDiscordClient);
            }
            if toolbar_btn(ui, Icon::Settings, "settings").clicked() {
                app.queue(Action::Navigate(Page::Settings));
            }
            if toolbar_btn(
                ui,
                if app.state.prefs.dock_visible {
                    Icon::PanelRightClose
                } else {
                    Icon::PanelRightOpen
                },
                "toggle widget dock",
            )
            .clicked()
            {
                app.queue(Action::ToggleDock);
            }
            if toolbar_btn(
                ui,
                if app.state.ui.layout_editing {
                    Icon::Check
                } else {
                    Icon::Move
                },
                "edit widget layout",
            )
            .clicked()
            {
                app.queue(Action::ToggleLayoutEditing);
            }
            // Search affordance with the keybind hint, Discord-style.
            let search = toolbar_btn(ui, Icon::Search, "command palette");
            if search.clicked() {
                app.queue(Action::TogglePalette);
            }
            ui.add_space(2.0);
            ui.label(RichText::new("Ctrl+K").size(10.0).color(color::text_muted()));
            ui.add_space(10.0);
            // Status chips: latency, demo flag, clock.
            if app.state.prefs.demo_mode {
                ui.label(RichText::new("DEMO").size(10.0).color(color::warning()));
                ui.add_space(8.0);
            }
            if let Some(lat) = app.state.discord.bot_latency_ms {
                ui.label(
                    RichText::new(format!("{lat}ms"))
                        .size(10.0)
                        .color(color::text_muted()),
                );
                ui.add_space(8.0);
            }
            ui.label(
                RichText::new(chrono_lite())
                    .size(10.0)
                    .color(color::text_muted()),
            );
        });
    });
}

/// Human-readable context for the titlebar.
fn context_label(app: &LApp, page: Page) -> String {
    if page == Page::ChannelInfo
        && let Some(cid) = &app.state.ui.selected_channel
        && let Some(ch) = app
            .state
            .discord
            .channels
            .iter()
            .find(|c| &c.id == cid)
    {
        return format!("#{}", ch.name);
    }
    if let Some(gid) = &app.state.ui.selected_guild
        && matches!(page, Page::ServerWorkspace)
        && let Some(g) = app.state.discord.guilds.iter().find(|g| &g.id == gid)
    {
        return g.name.clone();
    }
    page.title().to_string()
}

fn toolbar_btn(ui: &mut Ui, icon: Icon, tip: &str) -> egui::Response {
    let r = icons::icon_button(ui, icon, 14.0, color::text_secondary(), egui::Sense::click());
    r.clone().on_hover_text(tip);
    r
}

fn chrono_lite() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let s = secs % 86400;
    format!("{:02}:{:02}:{:02}Z", s / 3600, (s / 60) % 60, s % 60)
}

// -- Server rail ------------------------------------------------------------

/// Far-left rail: home button, guild icons with Discord-style selection
/// pills, and the add-server button pinned to the bottom.
pub fn draw_server_rail(app: &mut LApp, ui: &mut Ui) {
    ui.vertical(|ui| {
        ui.add_space(10.0);
        home_button(app, ui);
        for g in app.state.discord.guilds.clone() {
            guild_button(app, ui, &g);
        }
        if app.state.discord.guilds.is_empty() {
            ui.add_space(8.0);
            ui.label(
                RichText::new("no guilds")
                    .size(9.0)
                    .color(color::text_muted()),
            );
        }
        // Pin the add button to the bottom of the rail.
        let remaining = ui.available_height();
        if remaining > 70.0 {
            ui.add_space(remaining - 56.0);
        }
        add_server_button(app, ui);
        ui.add_space(10.0);
    });
}

/// Squircle rail button chrome shared by home/guild/add buttons.
fn rail_button(ui: &mut Ui, size: f32) -> (Rect, egui::Response) {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let fill = if resp.hovered() {
        color::elevated()
    } else {
        color::bg_secondary()
    };
    ui.painter().rect_filled(rect, size * 0.3, fill);
    ui.painter().rect_stroke(
        rect,
        size * 0.3,
        egui::Stroke::new(1.0, color::border_subtle()),
        egui::StrokeKind::Inside,
    );
    (rect, resp)
}

/// Selection pill on the rail's left edge (Discord signature).
fn rail_pill(ui: &mut Ui, cy: f32, selected: bool, hovered: bool) {
    let h = if selected {
        GUILD_ICON
    } else if hovered {
        20.0
    } else {
        8.0
    };
    let color = if selected {
        color::accent()
    } else if hovered {
        color::text_secondary()
    } else {
        color::text_muted()
    };
    let rect = Rect::from_center_size(
        pos2(5.0, cy),
        vec2(8.0, h),
    );
    ui.painter().rect_filled(rect, 4.0, color);
}

/// Home button: accent squircle with the app glyph.
fn home_button(app: &mut LApp, ui: &mut Ui) {
    let (rect, resp) = rail_button(ui, GUILD_ICON);
    rail_pill(ui, rect.center().y, true, resp.hovered());
    ui.painter().rect_filled(rect, GUILD_ICON * 0.3, color::accent());
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        "M",
        FontId::new(20.0, egui::FontFamily::Monospace),
        color::bg(),
    );
    resp.clone().on_hover_text("home");
    if resp.clicked() {
        app.queue(Action::Navigate(Page::ServerWorkspace));
    }
}

/// One guild icon: initials squircle + selection pill + bot dot.
fn guild_button(app: &mut LApp, ui: &mut Ui, g: &GuildInfo) {
    let selected = app.state.ui.selected_guild.as_deref() == Some(g.id.as_str());
    let (rect, resp) = rail_button(ui, GUILD_ICON);
    rail_pill(ui, rect.center().y, selected, resp.hovered());
    // Initials.
    let initials: String = g
        .name
        .split_whitespace()
        .take(2)
        .filter_map(|w| w.chars().next())
        .collect::<String>()
        .to_uppercase();
    let initials = if initials.is_empty() {
        "?".to_string()
    } else {
        initials
    };
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        initials,
        FontId::new(15.0, egui::FontFamily::Monospace),
        if selected {
            color::text()
        } else {
            color::text_secondary()
        },
    );
    // Bot-presence dot on the icon's bottom-right corner.
    if g.bot_present {
        let dot = pos2(rect.right() - 2.0, rect.bottom() - 2.0);
        ui.painter().circle_filled(dot, 4.5, color::success());
        ui.painter()
            .circle_stroke(dot, 4.5, egui::Stroke::new(1.5, color::rail()));
    }
    resp.clone().on_hover_text(&g.name);
    if resp.clicked() {
        app.queue(Action::SelectGuild(Some(g.id.clone())));
        app.queue(Action::Navigate(Page::ServerWorkspace));
    }
    ui.add_space(10.0);
}

/// Add-server button pinned at the rail bottom.
fn add_server_button(app: &mut LApp, ui: &mut Ui) {
    let (rect, resp) = rail_button(ui, GUILD_ICON);
    let icon_color = if resp.hovered() {
        color::success()
    } else {
        color::text_muted()
    };
    icons::paint_icon(
        ui.painter(),
        Icon::Plus,
        rect.shrink(15.0),
        icon_color,
        18.0,
    );
    resp.clone().on_hover_text("join server");
    if resp.clicked() {
        app.queue(Action::SetJoinInviteOpen(true));
    }
}

// -- Sidebar ---------------------------------------------------------------

/// Channel sidebar: guild header, pages, channels, workspace layouts, and
/// the bottom user panel.
pub fn draw_nav_sidebar(app: &mut LApp, ui: &mut Ui) {
    sidebar_header(app, ui);
    // Reserve space for the fixed user panel at the bottom.
    let user_h = 68.0;
    let max_h = (ui.available_height() - user_h - 8.0).max(120.0);
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .max_height(max_h)
        .show(ui, |ui| {
            ui.add_space(6.0);
            pages_section(app, ui);
            channels_section(app, ui);
            layouts_section(app, ui);
        });
    user_panel(app, ui);
}

/// Sidebar header: guild/workspace name + chevron (Discord channel-header
/// analogue). Clicking returns home.
fn sidebar_header(app: &mut LApp, ui: &mut Ui) {
    let (rect, resp) = ui.allocate_exact_size(
        vec2(ui.available_width(), 48.0),
        Sense::click(),
    );
    let hovered = resp.hovered();
    if hovered {
        ui.painter().rect_filled(rect, 0.0, color::hover());
    }
    let title = app.state.ui.selected_guild.as_deref().and_then(|gid| {
        app.state
            .discord
            .guilds
            .iter()
            .find(|g| g.id == gid)
            .map(|g| g.name.clone())
    });
    let label = title.unwrap_or_else(|| app.state.prefs.workspace_name.clone());
    ui.painter().text(
        pos2(rect.left() + 12.0, rect.center().y),
        Align2::LEFT_CENTER,
        label.to_uppercase(),
        FontId::new(12.0, egui::FontFamily::Monospace),
        if hovered {
            color::text()
        } else {
            color::text_secondary()
        },
    );
    icons::paint_icon(
        ui.painter(),
        Icon::ChevronDown,
        Rect::from_min_size(
            pos2(rect.right() - 26.0, rect.center().y - 8.0),
            vec2(16.0, 16.0),
        ),
        color::text_muted(),
        14.0,
    );
    resp.clone().on_hover_text("home");
    if resp.clicked() {
        app.queue(Action::Navigate(Page::ServerWorkspace));
    }
    // Header separator.
    let y = rect.bottom();
    ui.painter().hline(
        rect.left()..=rect.right(),
        y,
        egui::Stroke::new(1.0, color::border_subtle()),
    );
}

/// Discord-style channel-group label.
fn group_label(ui: &mut Ui, text: &str) {
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.label(
            RichText::new(text.to_uppercase())
                .size(11.0)
                .strong()
                .color(color::text_muted()),
        );
    });
}

/// A sidebar row: icon + label with Discord hover/selected chrome.
/// `indent` adds Discord's channel nesting offset.
fn sidebar_row(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    selected: bool,
    indent: f32,
) -> egui::Response {
    let width = (ui.available_width() - indent).max(40.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click());
    let row_rect = Rect::from_min_max(
        pos2(rect.left() + indent, rect.top()),
        pos2(rect.right(), rect.bottom()),
    );
    if selected {
        ui.painter().rect_filled(row_rect, 4.0, color::elevated());
    } else if resp.hovered() {
        ui.painter().rect_filled(row_rect, 4.0, color::hover());
    }
    let icon_rect = Rect::from_min_size(
        pos2(row_rect.left() + 10.0, row_rect.center().y - 8.0),
        vec2(16.0, 16.0),
    );
    icons::paint_icon(
        ui.painter(),
        icon,
        icon_rect,
        if selected {
            color::text()
        } else {
            color::text_muted()
        },
        14.0,
    );
    ui.painter().text(
        pos2(row_rect.left() + 32.0, row_rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::new(13.0, egui::FontFamily::Monospace),
        if selected {
            color::text()
        } else {
            color::text_secondary()
        },
    );
    resp
}

/// App pages section.
fn pages_section(app: &mut LApp, ui: &mut Ui) {
    group_label(ui, "pages");
    let current = app.state.ui.page.unwrap_or(Page::ServerWorkspace);
    for page in Page::ALL {
        let selected = page == current;
        if sidebar_row(ui, page_icon(page), page.title(), selected, 8.0).clicked() {
            app.queue(Action::Navigate(page));
        }
    }
}

/// Guild channels, grouped under collapsible category headers (kind 4).
fn channels_section(app: &mut LApp, ui: &mut Ui) {
    let Some(gid) = app.state.ui.selected_guild.clone() else {
        return;
    };
    let channels: Vec<ChannelEntry> = app
        .state
        .discord
        .channels
        .iter()
        .filter(|c| c.guild_id == gid)
        .cloned()
        .collect();
    if channels.is_empty() {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(18.0);
            ui.label(
                RichText::new("no channels loaded")
                    .size(11.0)
                    .color(color::text_muted()),
            );
        });
        return;
    }
    group_label(ui, "channels");
    let mut under_category = false;
    let mut cat_collapsed = false;
    for ch in &channels {
        if ch.kind == 4 {
            // Category header: click toggles collapse.
            let cat = ch.name.clone();
            cat_collapsed = app.state.ui.collapsed_categories.contains(&cat);
            let row = sidebar_row(ui, Icon::ChevronRight, &cat, false, 8.0);
            if row.clicked() {
                app.queue(Action::ToggleCategory(cat));
            }
            under_category = true;
            continue;
        }
        if cat_collapsed {
            continue;
        }
        let icon = match ch.kind {
            2 => Icon::Volume2,
            _ => Icon::Hash,
        };
        let selected = app.state.ui.selected_channel.as_deref() == Some(ch.id.as_str());
        // Channels under a category get Discord's extra indent.
        let indent = if under_category { 22.0 } else { 8.0 };
        if sidebar_row(ui, icon, &ch.name, selected, indent).clicked() {
            app.queue(Action::SelectChannel(Some(ch.id.clone())));
            app.queue(Action::Navigate(Page::ChannelInfo));
        }
    }
}

/// Workspace layout switcher at the bottom of the scroll area.
fn layouts_section(app: &mut LApp, ui: &mut Ui) {
    let layouts = app.state.workspace.layouts.clone();
    if layouts.is_empty() {
        return;
    }
    let active_id = app.active_layout().map(|l| l.id);
    group_label(ui, "workspace");
    for lay in &layouts {
        let selected = Some(lay.id) == active_id;
        if sidebar_row(ui, Icon::LayoutGrid, &lay.name, selected, 8.0).clicked() {
            app.queue(Action::SelectLayout(lay.id));
        }
    }
    ui.add_space(8.0);
}

// -- User panel --------------------------------------------------------------

/// Bottom user panel: avatar + status, voice quick-controls, settings gear.
fn user_panel(app: &mut LApp, ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), 68.0),
        Sense::hover(),
    );
    ui.painter().hline(
        rect.left()..=rect.right(),
        rect.top(),
        egui::Stroke::new(1.0, color::border_subtle()),
    );

    // Avatar with status dot.
    let avatar = Rect::from_min_size(
        pos2(rect.left() + 10.0, rect.center().y - 17.0),
        vec2(34.0, 34.0),
    );
    let account = app.state.discord.account.clone();
    let name = account.as_ref().map_or_else(
        || app.state.prefs.workspace_name.clone(),
        |a| a.global_name.clone().unwrap_or_else(|| a.username.clone()),
    );
    let letter: String = name.chars().next().unwrap_or('?').to_uppercase().collect();
    ui.painter().rect_filled(avatar, 10.0, color::elevated());
    ui.painter().text(
        avatar.center(),
        Align2::CENTER_CENTER,
        letter,
        FontId::new(15.0, egui::FontFamily::Monospace),
        color::text(),
    );
    // Status dot bottom-right of the avatar.
    let (dot_color, _) = match app.state.discord.status {
        l_core::models::ConnectionStatus::Connected
        | l_core::models::ConnectionStatus::Demo => (color::success(), "connected"),
        l_core::models::ConnectionStatus::Connecting => (color::warning(), "connecting"),
        _ => (color::text_muted(), "offline"),
    };
    let dot = pos2(avatar.right() - 1.0, avatar.bottom() - 1.0);
    ui.painter().circle_filled(dot, 5.0, dot_color);
    ui.painter()
        .circle_stroke(dot, 5.0, egui::Stroke::new(1.5, color::rail()));

    // Name + handle/status.
    let text_x = avatar.right() + 10.0;
    ui.painter().text(
        pos2(text_x, rect.top() + 22.0),
        Align2::LEFT_CENTER,
        name,
        FontId::new(13.0, egui::FontFamily::Monospace),
        color::text(),
    );
    let sub = account
        .as_ref()
        .map_or_else(|| "not connected".to_string(), |a| format!("@{}", a.username));
    ui.painter().text(
        pos2(text_x, rect.top() + 40.0),
        Align2::LEFT_CENTER,
        sub,
        FontId::new(11.0, egui::FontFamily::Monospace),
        color::text_muted(),
    );

    // Right-side icon cluster: voice controls (when active) + settings.
    let mut x = rect.right() - 12.0;
    let voice = app.state.discord.voice.clone();
    let mut icons_row: Vec<(Icon, &str)> = Vec::new();
    if voice.is_some() {
        let muted = voice.as_ref().is_some_and(|v| v.muted);
        let deafened = voice.as_ref().is_some_and(|v| v.deafened);
        icons_row.push((
            if muted { Icon::MicOff } else { Icon::Mic },
            "toggle bot mute",
        ));
        icons_row.push((
            if deafened {
                Icon::VolumeX
            } else {
                Icon::Headphones
            },
            "toggle bot deafen",
        ));
        icons_row.push((Icon::PhoneOff, "disconnect bot from voice"));
    }
    icons_row.push((Icon::Settings, "settings"));
    for (icon, tip) in icons_row.iter().rev() {
        x -= 26.0;
        let hit = Rect::from_min_size(pos2(x, rect.center().y - 11.0), vec2(22.0, 22.0));
        let resp = ui.interact(hit, ui.scope_id().with(("userpanel", *tip)), Sense::click());
        let hovered = resp.hovered();
        if hovered {
            ui.painter().rect_filled(hit, 4.0, color::hover());
        }
        icons::paint_icon(
            ui.painter(),
            *icon,
            hit.shrink(4.0),
            if hovered {
                color::accent()
            } else {
                color::text_muted()
            },
            14.0,
        );
        resp.clone().on_hover_text(*tip);
        if resp.clicked() {
            match *icon {
                Icon::Mic | Icon::MicOff => {
                    if let Some(v) = &voice {
                        app.queue(Action::VoiceSetFlags {
                            muted: Some(!v.muted),
                            deafened: None,
                        });
                    }
                }
                Icon::Headphones | Icon::VolumeX => {
                    if let Some(v) = &voice {
                        app.queue(Action::VoiceSetFlags {
                            muted: None,
                            deafened: Some(!v.deafened),
                        });
                    }
                }
                Icon::PhoneOff => app.queue(Action::LeaveVoiceChannel),
                _ => app.queue(Action::Navigate(Page::Settings)),
            }
        }
    }
}

// -- Dock & toasts -----------------------------------------------------------

/// Right-side widget dock.
pub fn draw_dock(app: &mut LApp, ui: &mut Ui) {
    ui.add_space(4.0);
    let cols = app.active_layout().map_or(4, |l| l.dock_columns);
    let edit = app.state.ui.layout_editing;
    app.draw_surface(ui, l_core::models::Surface::Dock, cols, edit);
}

/// Toast overlay.
pub fn draw_toasts(app: &mut LApp, ui: &mut Ui) {
    let toasts: Vec<(String, ToastTone, f32)> = app
        .state
        .ui
        .toasts
        .iter()
        .map(|t| {
            let tone = match t.kind {
                ToastKind::Info => ToastTone::Info,
                ToastKind::Success => ToastTone::Success,
                ToastKind::Warning => ToastTone::Warning,
                ToastKind::Error => ToastTone::Error,
            };
            (t.text.clone(), tone, t.created.elapsed().as_secs_f32())
        })
        .collect();
    l_ui::components::toast::draw_toasts(ui, &toasts);
}
