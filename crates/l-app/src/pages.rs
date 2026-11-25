//! Page content: overview/workspace grids, library, activity, notifications,
//! bot management, settings, channel info.

use egui::{Rect, RichText, Sense, Ui, Vec2};
use l_core::actions::Action;
use l_core::models::{ChannelEntry, ChatMessage, MessageReaction, Page};
use l_core::state::SettingsTab;
use l_ui::components::empty_state::empty_state;
use l_ui::components::section::section;
use l_ui::icons::{self, Icon};
use l_ui::theme::color;

use crate::app::LApp;

/// Draw the current page.
pub fn draw_page(app: &mut LApp, ui: &mut Ui) {
    match app.state.ui.page.unwrap_or(Page::ServerWorkspace) {
        Page::ServerWorkspace => draw_workspace(app, ui),
        Page::ChannelInfo => draw_channel_info(app, ui),
        Page::WidgetLibrary => draw_library_page(app, ui),
        Page::Activity => draw_activity(app, ui),
        Page::Notifications => draw_notifications(app, ui),
        Page::BotManagement => draw_bot_mgmt(app, ui),
        Page::Settings => draw_settings(app, ui),
    }
}

fn header(ui: &mut Ui, title: &str, subtitle: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(18.0).strong().color(color::text()));
        ui.label(RichText::new(subtitle).small().color(color::text_muted()));
    });
    ui.add_space(6.0);
}

fn draw_workspace(app: &mut LApp, ui: &mut Ui) {
    let Some(gid) = app.state.ui.selected_guild.clone() else {
        empty_state(
            ui,
            Icon::Server,
            "no server selected",
            "pick a guild from the rail",
        );
        return;
    };
    let guild = app
        .state
        .discord
        .guilds
        .iter()
        .find(|g| g.id == gid)
        .cloned();
    let name = guild
        .as_ref()
        .map_or_else(|| gid.clone(), |g| g.name.clone());
    header(ui, &name, "server workspace");

    // Guild summary row.
    ui.horizontal(|ui| {
        if let Some(g) = &guild {
            field(
                ui,
                "members",
                &g.member_count.map_or("—".into(), |n| n.to_string()),
            );
            field(
                ui,
                "online",
                &g.online_count.map_or("—".into(), |n| n.to_string()),
            );
            field(ui, "bot", if g.bot_present { "present" } else { "absent" });
        }
        let ch_count = app
            .state
            .discord
            .channels
            .iter()
            .filter(|c| c.guild_id == gid && c.kind != 4)
            .count();
        field(ui, "channels", &ch_count.to_string());
    });
    ui.add_space(8.0);

    let cols = app.active_layout().map_or(12, |l| l.main_columns);
    let edit = app.state.ui.layout_editing;
    app.draw_surface(ui, l_core::models::Surface::Main, cols, edit);
}

fn field(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).small().color(color::text_muted()));
        ui.label(RichText::new(value).small().color(color::text()));
        ui.add_space(10.0);
    });
}

fn draw_channel_info(app: &mut LApp, ui: &mut Ui) {
    let Some(cid) = app.state.ui.selected_channel.clone() else {
        empty_state(
            ui,
            Icon::Hash,
            "no channel selected",
            "pick a channel in the sidebar",
        );
        return;
    };
    let ch = app
        .state
        .discord
        .channels
        .iter()
        .find(|c| c.id == cid)
        .cloned();
    match ch {
        Some(c) if c.kind == 0 => draw_chat(app, ui, &c),
        Some(c) if c.kind == 2 => draw_voice_channel(app, ui, &c),
        Some(c) => {
            header(ui, &format!("# {}", c.name), "channel info");
            ui.horizontal(|ui| {
                field(ui, "id", &c.id);
                field(
                    ui,
                    "kind",
                    match c.kind {
                        0 => "text",
                        2 => "voice",
                        4 => "category",
                        _ => "other",
                    },
                );
                field(ui, "guild", &c.guild_id);
            });
            section(ui, "DETAILS");
            ui.label(
                RichText::new(
                    "Channel metadata (topic, slowmode, permissions) requires the bot \
                     `View Channel` + `Manage Channel` permissions and is fetched on demand.",
                )
                .small()
                .color(color::text_muted()),
            );
        }
        None => {
            empty_state(
                ui,
                Icon::Hash,
                "channel not found",
                "the channel list may not be loaded",
            );
        }
    }
}

// ---- Discord chat (bot identity) -----------------------------------------

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Map a common unicode emoji to a Lucide icon for chip rendering.
fn emoji_icon(emoji: &str) -> Option<Icon> {
    let base = emoji.split(':').next().unwrap_or(emoji);
    Some(match base {
        "👍" | "+1" => Icon::ThumbsUp,
        "👎" | "-1" => Icon::ThumbsDown,
        "❤️" | "❤" => Icon::Heart,
        "😂" | "🤣" => Icon::Laugh,
        "🔥" => Icon::Flame,
        "🎉" => Icon::PartyPopper,
        _ => return None,
    })
}

/// Quick-react choices shown in the message context menu.
const QUICK_REACTS: [(&str, &str); 4] = [
    ("react 👍", "👍"),
    ("react ❤️", "❤️"),
    ("react 😂", "😂"),
    ("react 🔥", "🔥"),
];

fn draw_chat(app: &mut LApp, ui: &mut Ui, ch: &ChannelEntry) {
    header(
        ui,
        &format!("# {}", ch.name),
        "chat via the bot account — it only sees channels it can access",
    );

    // Typing indicator (8s freshness window maintained by the reducer).
    let now = unix_now();
    let typing: Vec<String> = app
        .state
        .discord
        .typing
        .get(&ch.id)
        .map_or_else(Vec::new, |v| {
            v.iter()
                .filter(|(_, ts)| now - *ts < 8)
                .map(|(u, _)| u.clone())
                .collect()
        });
    if !typing.is_empty() {
        ui.label(
            RichText::new(format!("{} typing…", typing.join(", ")))
                .small()
                .italics()
                .color(color::text_muted()),
        );
    }

    let messages = app
        .state
        .discord
        .messages
        .get(&ch.id)
        .cloned()
        .unwrap_or_default();

    egui::ScrollArea::vertical()
        .id_salt(("chat", ch.id.clone()))
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if messages.is_empty() {
                empty_state(
                    ui,
                    Icon::Hash,
                    "no messages",
                    "history loads from Discord; live messages arrive via the gateway",
                );
            }
            for m in &messages {
                draw_message(app, ui, ch, m);
            }
        });

    ui.separator();
    draw_composer(app, ui, ch);
}

fn draw_message(app: &mut LApp, ui: &mut Ui, ch: &ChannelEntry, m: &ChatMessage) {
    let start = ui.cursor();
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(&m.author_name)
                .small()
                .strong()
                .color(if m.is_self {
                    color::accent()
                } else {
                    color::text()
                }),
        );
        if m.is_self {
            ui.label(RichText::new("bot").size(9.0).color(color::text_muted()));
        }
        if m.edited {
            ui.label(RichText::new("(edited)").size(9.0).color(color::text_muted()));
        }
        ui.label(
            RichText::new(fmt_time(m.timestamp))
                .size(9.0)
                .color(color::text_muted()),
        );
    });
    if !m.content.is_empty() {
        ui.add(egui::Label::new(RichText::new(&m.content).small().color(color::text())).wrap());
    }
    for a in &m.attachments {
        let resp = ui.link(
            RichText::new(format!("attachment: {}", a.filename))
                .small()
                .color(color::accent()),
        );
        if resp.clicked() {
            let _ = open::that(&a.url);
        }
    }
    if !m.reactions.is_empty() {
        ui.horizontal_wrapped(|ui| {
            for r in &m.reactions {
                if reaction_chip(ui, r).clicked() {
                    app.queue(Action::ReactChatMessage {
                        channel_id: ch.id.clone(),
                        message_id: m.id.clone(),
                        emoji: r.emoji.clone(),
                        add: !r.me,
                    });
                }
            }
        });
    }
    ui.add_space(2.0);

    // Row-level right-click menu: quick reacts + edit/delete for bot messages.
    let end = ui.min_rect();
    let row = Rect::from_min_max(start.min, egui::pos2(end.max.x, end.max.y));
    let row_resp = ui.interact(row, ui.scope_id().with(("msg", &m.id)), Sense::hover());
    row_resp.context_menu(|ui| {
        ui.label(RichText::new(&m.author_name).small().strong());
        ui.separator();
        for (label, emoji) in QUICK_REACTS {
            if ui.button(label).clicked() {
                app.queue(Action::ReactChatMessage {
                    channel_id: ch.id.clone(),
                    message_id: m.id.clone(),
                    emoji: emoji.to_string(),
                    add: true,
                });
                ui.close();
            }
        }
        if m.is_self {
            ui.separator();
            if ui.button("edit").clicked() {
                app.state.ui.chat_editing = Some(m.id.clone());
                app.state.ui.chat_draft.clone_from(&m.content);
                ui.close();
            }
            if ui.button("delete").clicked() {
                app.queue(Action::DeleteChatMessage {
                    channel_id: ch.id.clone(),
                    message_id: m.id.clone(),
                });
                ui.close();
            }
        }
    });
}

/// A compact reaction chip: icon/emoji + count, highlighted when the bot reacted.
fn reaction_chip(ui: &mut Ui, r: &MessageReaction) -> egui::Response {
    let count = r.count.to_string();
    let galley =
        ui.painter()
            .layout_no_wrap(count.clone(), egui::FontId::proportional(10.0), color::text());
    let icon_w = if emoji_icon(&r.emoji).is_some() {
        14.0
    } else {
        ui.painter()
            .layout_no_wrap(
                r.emoji.clone(),
                egui::FontId::proportional(10.0),
                color::text(),
            )
            .size()
            .x
            + 2.0
    };
    let (rect, resp) = ui.allocate_exact_size(
        Vec2::new(galley.size().x + icon_w + 12.0, 18.0),
        Sense::click(),
    );
    let bg = if resp.hovered() {
        color::hover()
    } else if r.me {
        color::accent().gamma_multiply(0.15)
    } else {
        color::hover().gamma_multiply(0.4)
    };
    ui.painter().rect_filled(rect, 3.0, bg);
    let fg = if r.me {
        color::accent()
    } else {
        color::text_secondary()
    };
    let mut x = rect.left() + 5.0;
    if let Some(icon) = emoji_icon(&r.emoji) {
        icons::paint_icon(
            ui.painter(),
            icon,
            Rect::from_min_size(egui::pos2(x, rect.top()), Vec2::splat(14.0)),
            fg,
            10.0,
        );
    } else {
        ui.painter().text(
            egui::pos2(x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            r.emoji.clone(),
            egui::FontId::proportional(10.0),
            fg,
        );
    }
    x += icon_w;
    ui.painter().text(
        egui::pos2(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        count,
        egui::FontId::proportional(10.0),
        fg,
    );
    resp
}

fn draw_composer(app: &mut LApp, ui: &mut Ui, ch: &ChannelEntry) {
    let editing = app.state.ui.chat_editing.is_some();
    let hint = if editing {
        "editing message — enter saves, esc cancels".to_string()
    } else {
        format!("message #{} as the bot account", ch.name)
    };
    let mut submit = false;
    ui.horizontal(|ui| {
        let was_empty = app.state.ui.chat_draft.is_empty();
        let response = ui.add(
            egui::TextEdit::singleline(&mut app.state.ui.chat_draft)
                .desired_width(f32::INFINITY)
                .hint_text(hint),
        );
        if was_empty && !app.state.ui.chat_draft.is_empty() {
            app.queue(Action::SendTyping {
                channel_id: ch.id.clone(),
            });
        }
        let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let send = icons::icon_button(ui, Icon::Send, 14.0, color::text_secondary(), Sense::click());
        submit |= send.clicked() || enter;
        if editing && response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            app.state.ui.chat_editing = None;
            app.state.ui.chat_draft.clear();
        }
        send.on_hover_text("send");
    });
    if submit {
        let content = app.state.ui.chat_draft.trim().to_string();
        if !content.is_empty() {
            if let Some(mid) = app.state.ui.chat_editing.take() {
                app.queue(Action::EditChatMessage {
                    channel_id: ch.id.clone(),
                    message_id: mid,
                    content,
                });
            } else {
                app.queue(Action::SendChatMessage {
                    channel_id: ch.id.clone(),
                    content,
                });
            }
            app.state.ui.chat_draft.clear();
        }
    }
}

// ---- Voice channel view ---------------------------------------------------

fn draw_voice_channel(app: &mut LApp, ui: &mut Ui, ch: &ChannelEntry) {
    header(
        ui,
        &format!("voice / {}", ch.name),
        "the bot joins voice under its own identity, not your account",
    );
    let voice = app.state.discord.voice.clone();
    let here = voice
        .as_ref()
        .is_some_and(|v| v.channel_id == ch.id && (v.connected || v.connecting));
    let elsewhere = voice
        .as_ref()
        .is_some_and(|v| v.channel_id != ch.id && (v.connected || v.connecting));

    section(ui, "VOICE");
    if here {
        let v = voice.clone().unwrap_or_default();
        ui.horizontal(|ui| {
            field(
                ui,
                "status",
                if v.connected {
                    "connected"
                } else {
                    "connecting…"
                },
            );
            field(
                ui,
                "channel",
                if v.channel_name.is_empty() {
                    &ch.name
                } else {
                    &v.channel_name
                },
            );
        });
        if let Some(e) = &v.error {
            ui.label(RichText::new(e).small().color(color::error()));
        }
        ui.horizontal(|ui| {
            let mute = icons::icon_button(
                ui,
                if v.muted { Icon::MicOff } else { Icon::Mic },
                14.0,
                if v.muted {
                    color::error()
                } else {
                    color::text_secondary()
                },
                Sense::click(),
            );
            if mute.clicked() {
                app.queue(Action::VoiceSetFlags {
                    muted: Some(!v.muted),
                    deafened: None,
                });
            }
            mute.on_hover_text(if v.muted { "unmute" } else { "mute" });
            let deafen = icons::icon_button(
                ui,
                if v.deafened {
                    Icon::VolumeX
                } else {
                    Icon::Headphones
                },
                14.0,
                if v.deafened {
                    color::error()
                } else {
                    color::text_secondary()
                },
                Sense::click(),
            );
            if deafen.clicked() {
                app.queue(Action::VoiceSetFlags {
                    muted: None,
                    deafened: Some(!v.deafened),
                });
            }
            deafen.on_hover_text(if v.deafened { "undeafen" } else { "deafen" });
            let leave = icons::icon_button(ui, Icon::PhoneOff, 14.0, color::error(), Sense::click());
            if leave.clicked() {
                app.queue(Action::LeaveVoiceChannel);
            }
            leave.on_hover_text("disconnect from voice");
        });
    } else {
        if elsewhere && let Some(v) = &voice {
            ui.label(
                RichText::new(format!(
                    "bot is in {} — leave it first",
                    if v.channel_name.is_empty() {
                        "another channel"
                    } else {
                        &v.channel_name
                    }
                ))
                .small()
                .color(color::warning()),
            );
        }
        ui.horizontal(|ui| {
            let join =
                icons::icon_button(ui, Icon::Phone, 14.0, color::text_secondary(), Sense::click());
            if join.clicked() {
                app.queue(Action::JoinVoiceChannel {
                    guild_id: ch.guild_id.clone(),
                    channel_id: ch.id.clone(),
                });
            }
            join.on_hover_text("join voice as the bot");
            ui.label(
                RichText::new("join voice as the bot")
                    .small()
                    .color(color::text_secondary()),
            );
        });
        ui.label(
            RichText::new(
                "Requires the bot token, an enabled gateway connection, and the bot invited \
                 to this server with Connect permission.",
            )
            .size(10.0)
            .color(color::text_muted()),
        );
    }
}

fn draw_library_page(app: &mut LApp, ui: &mut Ui) {
    header(ui, "widget library", "browse and add widgets");
    let q = app.pending.clone();
    let mut dispatch = move |a: Action| q.borrow_mut().push(a);
    l_widgets::library::draw_library(
        ui,
        &app.registry,
        &mut app.library_filter,
        &mut app.library_search,
        &mut dispatch,
    );
    ui.add_space(8.0);
    section(ui, "PRESETS");
    ui.horizontal_wrapped(|ui| {
        for p in l_widgets::presets::PRESETS {
            if ui.button(p).clicked() {
                app.queue(Action::ApplyPreset(p.to_string()));
            }
        }
    });
}

fn draw_activity(app: &mut LApp, ui: &mut Ui) {
    header(ui, "activity", "audit log + notification timeline");
    section(ui, "AUDIT LOG");
    let mut entries: Vec<(i64, String, String, String, bool)> = app
        .state
        .workspace
        .audit_log
        .iter()
        .map(|e| {
            (
                e.at,
                e.actor.clone(),
                e.action.clone(),
                e.target.clone(),
                e.ok,
            )
        })
        .collect();
    entries.sort_by_key(|e| std::cmp::Reverse(e.0));
    if entries.is_empty() {
        ui.label(
            RichText::new("no audit entries yet")
                .small()
                .color(color::text_muted()),
        );
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        for (at, actor, action, target, ok) in entries.iter().take(100) {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(fmt_time(*at))
                        .size(10.0)
                        .color(color::text_muted()),
                );
                ui.label(RichText::new(actor).small().color(if *ok {
                    color::success()
                } else {
                    color::error()
                }));
                ui.label(RichText::new(action).small().color(color::text()));
                ui.label(RichText::new(target).small().color(color::text_secondary()));
            });
        }
    });
}

fn fmt_time(ts: i64) -> String {
    // Compact relative-ish timestamp without chrono.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let d = now - ts;
    if d < 60 {
        format!("{}s ago", d.max(0))
    } else if d < 3600 {
        format!("{}m ago", d / 60)
    } else if d < 86400 {
        format!("{}h ago", d / 3600)
    } else {
        format!("{}d ago", d / 86400)
    }
}

fn draw_notifications(app: &mut LApp, ui: &mut Ui) {
    header(ui, "notifications", "local notification center");
    ui.horizontal(|ui| {
        if l_ui::components::button::button(
            ui,
            "mark all read",
            l_ui::components::button::ButtonKind::Default,
        )
        .clicked()
        {
            app.queue(Action::MarkAllNotificationsRead);
        }
        if l_ui::components::button::button(
            ui,
            "clear all",
            l_ui::components::button::ButtonKind::Danger,
        )
        .clicked()
        {
            app.queue(Action::ClearNotifications);
        }
        // Source filter.
        ui.label(RichText::new("filter:").small().color(color::text_muted()));
        for (i, f) in ["all", "discord", "system", "widget"].iter().enumerate() {
            let sel = app.notif_filter == *f;
            if ui
                .selectable_label(sel, RichText::new(*f).small())
                .clicked()
            {
                app.notif_filter = (*f).to_string();
            }
            let _ = i;
        }
    });
    ui.add_space(6.0);

    let filter = app.notif_filter.clone();
    let notifs: Vec<l_core::models::Notification> = app
        .state
        .workspace
        .notifications
        .iter()
        .filter(|n| filter == "all" || n.source == filter || n.category == filter)
        .cloned()
        .collect();
    if notifs.is_empty() {
        empty_state(
            ui,
            Icon::Bell,
            "no notifications",
            "events will appear here",
        );
        return;
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        for n in notifs.iter().take(200) {
            ui.horizontal(|ui| {
                let dot_color = if n.read {
                    color::text_muted()
                } else {
                    color::accent()
                };
                ui.painter()
                    .circle_filled(ui.cursor().min + egui::vec2(4.0, 8.0), 3.0, dot_color);
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&n.title).small().color(if n.read {
                            color::text_secondary()
                        } else {
                            color::text()
                        }));
                        ui.label(RichText::new(&n.source).size(9.0).color(color::text_muted()));
                        ui.label(
                            RichText::new(fmt_time(n.created_at))
                                .size(9.0)
                                .color(color::text_muted()),
                        );
                    });
                    if !n.body.is_empty() {
                        ui.label(RichText::new(&n.body).small().color(color::text_secondary()));
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button(if n.read { "unread" } else { "read" })
                        .clicked()
                    {
                        app.queue(Action::SetNotificationRead {
                            id: n.id,
                            read: !n.read,
                        });
                    }
                    if ui.small_button("x").clicked() {
                        app.queue(Action::DeleteNotification(n.id));
                    }
                });
            });
            ui.add_space(4.0);
        }
    });
}

fn draw_bot_mgmt(app: &mut LApp, ui: &mut Ui) {
    header(ui, "bot management", "discord bot integration");

    section(ui, "CONNECTION");
    let status = app.state.discord.status.clone();
    let status_text = match status {
        l_core::models::ConnectionStatus::Connected => "connected".to_string(),
        l_core::models::ConnectionStatus::Demo => "demo mode".to_string(),
        l_core::models::ConnectionStatus::Connecting => "connecting…".to_string(),
        l_core::models::ConnectionStatus::Degraded(ref e) => format!("degraded: {e}"),
        l_core::models::ConnectionStatus::Disconnected => "disconnected".to_string(),
    };
    ui.label(RichText::new(status_text).color(color::text()));
    if let Some(a) = &app.state.discord.account {
        ui.label(
            RichText::new(format!(
                "@{}{}",
                a.username,
                a.global_name.as_ref().map_or_default(|g| format!(" ({g})"))
            ))
            .small()
            .color(color::text_secondary()),
        );
        ui.label(
            RichText::new(format!("scopes: {}", a.scopes.join(", ")))
                .size(10.0)
                .color(color::text_muted()),
        );
    }
    ui.horizontal(|ui| {
        if matches!(
            status,
            l_core::models::ConnectionStatus::Connected | l_core::models::ConnectionStatus::Demo
        ) {
            if l_ui::components::button::button(
                ui,
                "disconnect",
                l_ui::components::button::ButtonKind::Danger,
            )
            .clicked()
            {
                app.queue(Action::DisconnectDiscord);
            }
        } else if l_ui::components::button::button(
            ui,
            "connect discord",
            l_ui::components::button::ButtonKind::Primary,
        )
        .clicked()
        {
            app.queue(Action::StartDiscordOAuth);
        }
        if l_ui::components::button::button(
            ui,
            "setup…",
            l_ui::components::button::ButtonKind::Default,
        )
        .clicked()
        {
            app.queue(Action::SetDiscordSetupOpen(true));
        }
    });

    section(ui, "BOT");
    let bot_status = format!("{:?}", app.state.discord.bot_status).to_lowercase();
    ui.label(
        RichText::new(format!("status: {bot_status}"))
            .small()
            .color(color::text_secondary()),
    );
    if let Some(name) = &app.state.discord.bot_username {
        ui.label(
            RichText::new(format!("bot: @{name}"))
                .small()
                .color(color::text()),
        );
    }
    if let Some(lat) = app.state.discord.bot_latency_ms {
        ui.label(
            RichText::new(format!("latency: {lat}ms"))
                .size(10.0)
                .color(color::text_muted()),
        );
    }
    if let Some(gw) = &app.state.discord.gateway_state {
        ui.label(
            RichText::new(format!("gateway: {gw}"))
                .size(10.0)
                .color(color::text_muted()),
        );
    }
    ui.horizontal(|ui| {
        if matches!(
            app.state.discord.bot_status,
            l_core::models::BotStatus::NotConfigured | l_core::models::BotStatus::Error(_)
        ) {
            // Token entry is in Settings → Discord; link there.
            if ui.button("set token in settings →").clicked() {
                app.queue(Action::NavigateSettingsTab(SettingsTab::Discord));
            }
        }
        if !matches!(
            app.state.discord.bot_status,
            l_core::models::BotStatus::NotConfigured
        ) && l_ui::components::button::button(
            ui,
            "remove token",
            l_ui::components::button::ButtonKind::Danger,
        )
        .clicked()
        {
            app.queue(Action::RemoveBotToken);
        }
    });

    section(ui, "CAPABILITIES");
    ui.vertical(|ui| {
        cap_row(
            ui,
            "identify (profile)",
            app.state.discord.account.is_some(),
        );
        cap_row(ui, "guilds list", !app.state.discord.guilds.is_empty());
        cap_row(
            ui,
            "bot online",
            matches!(
                app.state.discord.bot_status,
                l_core::models::BotStatus::Online
            ),
        );
        cap_row(ui, "gateway", app.state.discord.gateway_state.is_some());
        cap_row(ui, "demo data", app.state.prefs.demo_mode);
    });
}

fn cap_row(ui: &mut Ui, label: &str, ok: bool) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(if ok { "[x]" } else { "[ ]" })
                .small()
                .color(if ok {
                    color::success()
                } else {
                    color::text_muted()
                }),
        );
        ui.label(RichText::new(label).small().color(color::text_secondary()));
    });
}

fn draw_settings(app: &mut LApp, ui: &mut Ui) {
    header(ui, "settings", "preferences");
    let tabs = [
        (SettingsTab::Appearance, "APPEARANCE"),
        (SettingsTab::Workspace, "WORKSPACE"),
        (SettingsTab::Discord, "DISCORD"),
        (SettingsTab::Notifications, "NOTIFICATIONS"),
        (SettingsTab::Plugins, "PLUGINS"),
        (SettingsTab::Advanced, "ADVANCED"),
    ];
    let active_idx = tabs
        .iter()
        .position(|(t, _)| *t == app.state.ui.settings_tab)
        .unwrap_or(0);
    if let Some(i) = l_ui::components::tabs::tab_strip(
        ui,
        &tabs.map(|(_, l)| l_ui::components::tabs::Tab { id: l, label: l }),
        active_idx,
    ) {
        app.queue(Action::SettingsTab(tabs[i].0));
    }
    ui.add_space(8.0);
    egui::ScrollArea::vertical().show(ui, |ui| match app.state.ui.settings_tab {
        SettingsTab::Appearance => settings_appearance(app, ui),
        SettingsTab::Workspace => settings_workspace(app, ui),
        SettingsTab::Discord => settings_discord(app, ui),
        SettingsTab::Notifications => settings_notifications(app, ui),
        SettingsTab::Plugins => settings_plugins(app, ui),
        SettingsTab::Advanced => settings_advanced(app, ui),
    });
}

fn settings_appearance(app: &mut LApp, ui: &mut Ui) {
    let mut dirty = false;
    {
        let mut p = app.state.prefs.clone();
        ui.horizontal(|ui| {
            ui.label(RichText::new("ui scale").small().color(color::text_muted()));
            if ui
                .add(egui::Slider::new(&mut p.ui_scale, 80.0..=150.0).suffix("%"))
                .changed()
            {
                dirty = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("font size").small().color(color::text_muted()));
            if ui
                .add(egui::Slider::new(&mut p.font_size, 10.0..=18.0).suffix("pt"))
                .changed()
            {
                dirty = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("compact").small().color(color::text_muted()));
            if l_ui::components::toggle::toggle(ui, &mut p.compact).changed() {
                dirty = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("clock seconds")
                    .small()
                    .color(color::text_muted()),
            );
            if l_ui::components::toggle::toggle(ui, &mut p.clock_seconds).changed() {
                dirty = true;
            }
        });
        section(ui, "ANIMATION");
        ui.horizontal(|ui| {
            for (val, label) in [
                (l_core::state::AnimationIntensity::Off, "off"),
                (l_core::state::AnimationIntensity::Subtle, "subtle"),
                (l_core::state::AnimationIntensity::Full, "full"),
            ] {
                if ui
                    .selectable_label(p.animation == val, RichText::new(label).small())
                    .clicked()
                {
                    p.animation = val;
                    dirty = true;
                }
            }
        });
        section(ui, "THEME");
        ui.horizontal(|ui| {
            for preset in l_ui::theme::ThemePreset::ALL {
                let choice = match preset {
                    l_ui::theme::ThemePreset::Mono => l_core::state::ThemeChoice::Mono,
                    l_ui::theme::ThemePreset::DiscordDark => {
                        l_core::state::ThemeChoice::DiscordDark
                    }
                    l_ui::theme::ThemePreset::Light => l_core::state::ThemeChoice::Light,
                    l_ui::theme::ThemePreset::Amoled => l_core::state::ThemeChoice::Amoled,
                };
                if ui
                    .selectable_label(p.theme == choice, RichText::new(preset.label()).small())
                    .clicked()
                {
                    p.theme = choice;
                    dirty = true;
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("custom accent").small().color(color::text_muted()));
            let mut on = p.accent_color.is_some();
            if l_ui::components::toggle::toggle(ui, &mut on).changed() {
                p.accent_color = on.then_some(0x58_65_F2);
                dirty = true;
            }
            if p.accent_color.is_some() {
                let mut c = p.accent_color.unwrap_or(0x58_65_F2);
                let mut rgba = egui::Color32::from_rgb(
                    (c >> 16) as u8,
                    (c >> 8) as u8,
                    c as u8,
                );
                if ui
                    .color_edit_button_srgba(&mut rgba)
                    .on_hover_text("accent color")
                    .changed()
                {
                    c = (u32::from(rgba.r()) << 16) | (u32::from(rgba.g()) << 8) | u32::from(rgba.b());
                    p.accent_color = Some(c);
                    dirty = true;
                }
            }
        });
        if dirty {
            let np = p.clone();
            app.queue(Action::UpdatePrefs(Box::new(np)));
        }
    }
}

fn settings_workspace(app: &mut LApp, ui: &mut Ui) {
    let mut p = app.state.prefs.clone();
    let mut dirty = false;
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("workspace name")
                .small()
                .color(color::text_muted()),
        );
        if l_ui::components::field::field(ui, "ws-name", &mut p.workspace_name, "WORKSPACE", false)
            .changed()
        {
            dirty = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("dock width").small().color(color::text_muted()));
        if ui
            .add(egui::Slider::new(&mut p.dock_width, 240.0..=560.0).suffix("px"))
            .changed()
        {
            dirty = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("show server rail")
                .small()
                .color(color::text_muted()),
        );
        if l_ui::components::toggle::toggle(ui, &mut p.show_server_rail).changed() {
            dirty = true;
        }
        ui.label(RichText::new("show nav").small().color(color::text_muted()));
        if l_ui::components::toggle::toggle(ui, &mut p.show_nav_sidebar).changed() {
            dirty = true;
        }
        ui.label(
            RichText::new("dock visible")
                .small()
                .color(color::text_muted()),
        );
        if l_ui::components::toggle::toggle(ui, &mut p.dock_visible).changed() {
            dirty = true;
        }
    });
    section(ui, "LAYOUTS");
    if let Some(l) = app.active_layout() {
        ui.label(
            RichText::new(format!(
                "active: {} ({} widgets)",
                l.name,
                l.main.len() + l.dock.len()
            ))
            .small()
            .color(color::text_secondary()),
        );
    }
    ui.horizontal(|ui| {
        if ui.button("save layout as…").clicked() {
            // Prompt via a simple inline field is overkill; use confirm-free
            // approach: save with timestamped name.
            let name = format!("layout-{}", app.state.workspace.layouts.len() + 1);
            app.queue(Action::SaveLayoutAs(name));
        }
        if ui.button("export widgets").clicked() {
            app.queue(Action::ExportWidgetConfigs);
        }
        if ui.button("import widgets").clicked() {
            app.pick_import_file();
        }
    });
    section(ui, "KEYBINDS");
    ui.label(
        RichText::new("click a binding, then press the new keys (Esc cancels)")
            .size(10.0)
            .color(color::text_muted()),
    );
    ui.add_space(2.0);
    for r in crate::keybinds::resolve(&app.state.prefs) {
        let recording = app.state.ui.keybind_capture.as_deref() == Some(r.id);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(r.title)
                    .small()
                    .color(color::text_secondary()),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if recording {
                    ui.label(
                        RichText::new("press keys…")
                            .small()
                            .color(color::accent()),
                    );
                } else {
                    let label = if r.combo_str == "—" {
                        "unbound".to_string()
                    } else {
                        r.combo_str.clone()
                    };
                    if ui
                        .add(egui::Button::new(
                            RichText::new(label).small().color(color::text()),
                        ))
                        .clicked()
                    {
                        app.state.ui.keybind_capture = Some(r.id.to_string());
                    }
                }
                if r.overridden && !recording && ui.small_button("reset").clicked() {
                    let mut p2 = app.state.prefs.clone();
                    p2.keybinds.retain(|k| k.action != r.id);
                    app.queue(Action::UpdatePrefs(Box::new(p2)));
                }
            });
        });
    }
    if dirty {
        app.queue(Action::UpdatePrefs(Box::new(p)));
    }
}

fn settings_discord(app: &mut LApp, ui: &mut Ui) {
    let mut p = app.state.prefs.clone();
    let mut dirty = false;
    section(ui, "OAUTH2");
    ui.label(
        RichText::new("Public client — no secret. Create an app at discord.com/developers and paste the Client ID.")
            .small()
            .color(color::text_muted()),
    );
    ui.horizontal(|ui| {
        ui.label(RichText::new("client id").small().color(color::text_muted()));
        if l_ui::components::field::field(
            ui,
            "dc-cid",
            p.discord_client_id.get_or_insert_default(),
            "application id",
            false,
        )
        .changed()
        {
            dirty = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("redirect port")
                .small()
                .color(color::text_muted()),
        );
        if ui
            .add(egui::DragValue::new(&mut p.discord_redirect_port).range(1024..=65535))
            .changed()
        {
            dirty = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("redirect uri")
                .small()
                .color(color::text_muted()),
        );
        let mut uri = p.discord_redirect_uri.clone().unwrap_or_default();
        if l_ui::components::field::field(
            ui,
            "dc-redirect-uri",
            &mut uri,
            "blank = http://127.0.0.1:port/callback",
            false,
        )
        .changed()
        {
            let t = uri.trim().to_string();
            p.discord_redirect_uri = if t.is_empty() { None } else { Some(t) };
            dirty = true;
        }
    });
    ui.label(
        RichText::new(
            "must match a redirect registered in the Developer Portal exactly \
             (localhost and 127.0.0.1 are different hosts)",
        )
        .size(10.0)
        .color(color::warning()),
    );
    section(ui, "BOT TOKEN");
    ui.label(
        RichText::new("Stored in the OS keychain — never displayed after entry.")
            .small()
            .color(color::text_muted()),
    );
    match app.state.discord.bot_status {
        l_core::models::BotStatus::NotConfigured => {
            ui.horizontal(|ui| {
                ui.label(RichText::new("not set").small().color(color::text_muted()));
                let mut token = app.bot_token_input.clone();
                let resp = l_ui::components::field::field(
                    ui,
                    "settings-bot-token",
                    &mut token,
                    "paste bot token",
                    true,
                );
                app.bot_token_input = token;
                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if l_ui::components::button::button(
                    ui,
                    "save",
                    l_ui::components::button::ButtonKind::Primary,
                )
                .clicked()
                    || enter
                {
                    let t = app.bot_token_input.trim().to_string();
                    app.bot_token_input.clear();
                    app.queue(Action::SetBotToken(t));
                }
            });
        }
        _ => {
            ui.horizontal(|ui| {
                ui.label(RichText::new("configured").small().color(color::success()));
                if let Some(name) = &app.state.discord.bot_username {
                    ui.label(
                        RichText::new(format!("@{name}"))
                            .small()
                            .color(color::text_secondary()),
                    );
                }
                if l_ui::components::button::button(
                    ui,
                    "re-verify",
                    l_ui::components::button::ButtonKind::Default,
                )
                .clicked()
                {
                    app.queue(Action::VerifyBot);
                }
                if l_ui::components::button::button(
                    ui,
                    "remove",
                    l_ui::components::button::ButtonKind::Danger,
                )
                .clicked()
                {
                    app.queue(Action::RemoveBotToken);
                }
            });
        }
    }
    section(ui, "GATEWAY & INTENTS");
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("gateway enabled")
                .small()
                .color(color::text_muted()),
        );
        if l_ui::components::toggle::toggle(ui, &mut p.gateway_enabled).changed() {
            dirty = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("members intent (privileged)")
                .small()
                .color(color::text_muted()),
        );
        if l_ui::components::toggle::toggle(ui, &mut p.intent_guild_members).changed() {
            dirty = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("presences intent (privileged)")
                .small()
                .color(color::text_muted()),
        );
        if l_ui::components::toggle::toggle(ui, &mut p.intent_presences).changed() {
            dirty = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("message content intent (privileged)")
                .small()
                .color(color::text_muted()),
        );
        if l_ui::components::toggle::toggle(ui, &mut p.intent_message_content).changed() {
            dirty = true;
        }
    });
    ui.label(
        RichText::new(
            "Privileged intents must be enabled in the Discord Developer Portal for the bot.",
        )
        .size(10.0)
        .color(color::warning()),
    );
    section(ui, "RICH PRESENCE (RPC)");
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("local rpc server (discord-ipc)")
                .small()
                .color(color::text_muted()),
        );
        if l_ui::components::toggle::toggle(ui, &mut p.rpc_enabled).changed() {
            dirty = true;
        }
    });
    ui.label(
        RichText::new(
            "Games and tools using discord-rpc can set your bot's rich presence \
             while the gateway is connected.",
        )
        .size(10.0)
        .color(color::text_muted()),
    );
    section(ui, "DISCORD CLIENT (WEB)");
    let status = if crate::discord_web::is_open() {
        "running"
    } else {
        "closed"
    };
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("status: {status}"))
                .small()
                .color(if status == "running" {
                    color::success()
                } else {
                    color::text_muted()
                }),
        );
        ui.label(
            RichText::new("A real Discord client window (your account, not the bot). \
             Drop .css/.js files into the mods folder to restyle or mod it.")
            .size(10.0)
            .color(color::text_muted()),
        );
    });
    ui.horizontal(|ui| {
        if l_ui::components::button::button(
            ui,
            "open client",
            l_ui::components::button::ButtonKind::Primary,
        )
        .clicked()
        {
            app.queue(Action::OpenDiscordClient);
        }
        if l_ui::components::button::button(
            ui,
            "restart",
            l_ui::components::button::ButtonKind::Default,
        )
        .clicked()
        {
            app.queue(Action::RestartDiscordClient);
        }
        if l_ui::components::button::button(
            ui,
            "reload page",
            l_ui::components::button::ButtonKind::Default,
        )
        .clicked()
        {
            crate::discord_web::reload();
        }
        if l_ui::components::button::button(
            ui,
            "mods folder",
            l_ui::components::button::ButtonKind::Ghost,
        )
        .clicked()
        {
            let dir = crate::discord_web::mods_dir();
            let _ = std::fs::create_dir_all(&dir);
            let _ = open::that(&dir);
        }
    });
    if dirty {
        app.queue(Action::UpdatePrefs(Box::new(p)));
    }
}

fn settings_notifications(app: &mut LApp, ui: &mut Ui) {
    let mut p = app.state.prefs.clone();
    let mut dirty = false;
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("os notifications")
                .small()
                .color(color::text_muted()),
        );
        if l_ui::components::toggle::toggle(ui, &mut p.os_notifications).changed() {
            dirty = true;
        }
    });
    section(ui, "CATEGORIES");
    for cat in ["discord", "system", "widget"] {
        let mut on = p.notify_categories.iter().any(|c| c == cat);
        ui.horizontal(|ui| {
            ui.label(RichText::new(cat).small().color(color::text_muted()));
            if l_ui::components::toggle::toggle(ui, &mut on).changed() {
                if on {
                    p.notify_categories.push(cat.to_string());
                } else {
                    p.notify_categories.retain(|c| c != cat);
                }
                dirty = true;
            }
        });
    }
    if dirty {
        app.queue(Action::UpdatePrefs(Box::new(p)));
    }
}

fn settings_plugins(app: &mut LApp, ui: &mut Ui) {
    section(ui, "PLUGINS");
    ui.label(
        RichText::new(
            "Plugins are folders in the plugins directory with a manifest.json \
             and an executable entry point. They can add widgets and palette \
             commands.",
        )
        .size(10.0)
        .color(color::text_muted()),
    );
    ui.add_space(4.0);
    let plugins = app.plugin_host().plugins();
    if plugins.is_empty() {
        l_ui::components::empty_state::empty_state(
            ui,
            Icon::Package,
            "no plugins found",
            "drop a plugin folder into the plugins directory",
        );
    }
    for p in plugins {
        let id = p.manifest.id.clone();
        let enabled = p.enabled;
        let state = app.plugin_host().state_of(&id);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(&p.manifest.name).small().strong());
                let meta = [
                    (!p.manifest.version.is_empty()).then_some(p.manifest.version.as_str()),
                    (!p.manifest.author.is_empty()).then_some(p.manifest.author.as_str()),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
                if !meta.is_empty() {
                    ui.label(RichText::new(meta).size(10.0).color(color::text_muted()));
                }
                if !p.manifest.description.is_empty() {
                    ui.label(
                        RichText::new(&p.manifest.description)
                            .size(10.0)
                            .color(color::text_secondary()),
                    );
                }
                ui.label(
                    RichText::new(format!(
                        "{} widget(s) · {} command(s) · {state}",
                        p.manifest.widgets.len(),
                        p.manifest.commands.len(),
                    ))
                    .size(10.0)
                    .color(color::text_muted()),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut on = enabled;
                if l_ui::components::toggle::toggle(ui, &mut on).changed() {
                    let host = app.plugin_host().clone();
                    let id2 = id.clone();
                    app.tokio().spawn(async move {
                        let _ = tokio::task::spawn_blocking(move || {
                            host.set_enabled(&id2, on);
                        })
                        .await;
                    });
                }
            });
        });
        ui.add_space(6.0);
    }
    section(ui, "ACTIONS");
    ui.horizontal(|ui| {
        if ui.button("rescan plugins").clicked() {
            app.rescan_plugin_widgets();
        }
        if ui.button("open plugins folder").clicked() {
            let dir = app.plugin_host().plugins_dir().to_path_buf();
            app.tokio().spawn(async move {
                let _ = tokio::task::spawn_blocking(move || open::that(&dir)).await;
            });
        }
    });
}

fn settings_advanced(app: &mut LApp, ui: &mut Ui) {
    section(ui, "MODE");
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("demo mode (sample data)")
                .small()
                .color(color::text_muted()),
        );
        let mut demo = app.state.prefs.demo_mode;
        if l_ui::components::toggle::toggle(ui, &mut demo).changed() {
            app.queue(Action::ToggleDemoMode);
        }
    });
    section(ui, "DATA");
    ui.horizontal(|ui| {
        if ui.button("export widget configs").clicked() {
            app.queue(Action::ExportWidgetConfigs);
        }
        if ui.button("import widget configs").clicked() {
            app.pick_import_file();
        }
    });
    section(ui, "DANGER");
    if l_ui::components::button::button(
        ui,
        "reset all settings",
        l_ui::components::button::ButtonKind::Danger,
    )
    .clicked()
    {
        app.queue(Action::Confirm(Box::new(l_core::state::ConfirmDialog {
            title: "reset settings".into(),
            body: "Preferences return to defaults. Layouts, notes, and notifications are kept."
                .into(),
            on_confirm: Action::ResetSettings,
            destructive: true,
        })));
    }
    ui.add_space(8.0);
    ui.label(
        RichText::new(
            "Monocord — local-first discord companion. Data lives in SQLite; secrets in the OS keychain.",
        )
        .size(10.0)
        .color(color::text_muted()),
    );
}
