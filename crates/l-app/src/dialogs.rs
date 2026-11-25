//! Modal dialogs: confirm, Discord setup, widget config, import, integrations.

use egui::{RichText, Ui};
use l_core::actions::Action;
use l_core::models::WidgetInstanceId;
use l_core::state::ConfirmDialog;
use l_ui::components::button::{ButtonKind, button};
use l_ui::components::modal::modal;
use l_ui::theme::color;
use l_widgets::core::types::{SettingKind, merged_config};

use crate::app::LApp;

/// Draw all open dialogs.
pub fn draw_dialogs(app: &mut LApp, ctx: &egui::Context) {
    if let Some(confirm) = app.state.ui.confirm.clone() {
        draw_confirm(app, ctx, &confirm);
    }
    if app.state.ui.discord_setup_open {
        draw_discord_setup(app, ctx);
    }
    if app.state.ui.join_invite_open {
        draw_join_invite(app, ctx);
    }
    if app.state.ui.add_integration_open {
        draw_add_integration(app, ctx);
    }
    if app.state.ui.configuring_widget.is_some() {
        draw_widget_config(app, ctx);
    }
    if app.state.ui.import_dialog.is_some() {
        draw_import(app, ctx);
    }
}

fn draw_confirm(app: &mut LApp, ctx: &egui::Context, confirm: &ConfirmDialog) {
    let title = confirm.title.clone();
    let body = confirm.body.clone();
    let on_confirm = confirm.on_confirm.clone();
    let destructive = confirm.destructive;
    modal(ctx, "confirm-dialog", &title, 380.0, move |ui| {
        ui.label(RichText::new(&body).small().color(color::text_secondary()));
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if button(ui, "cancel", ButtonKind::Default).clicked() {
                app.queue(Action::DismissConfirm);
            }
            let kind = if destructive {
                ButtonKind::Danger
            } else {
                ButtonKind::Primary
            };
            if button(ui, "confirm", kind).clicked() {
                let a = on_confirm.clone();
                app.queue(Action::DismissConfirm);
                app.queue(a);
            }
        });
    });
}

/// Bot invite permissions: view channels, send/read/react/attach, voice connect+speak.
const BOT_PERMISSIONS: u64 =
    (1 << 10) | (1 << 11) | (1 << 16) | (1 << 6) | (1 << 15) | (1 << 20) | (1 << 21);

fn draw_discord_setup(app: &mut LApp, ctx: &egui::Context) {
    modal(ctx, "discord-setup", "discord setup", 480.0, |ui| {
        ui.label(
            RichText::new("work through the checklist — each step unlocks the next")
                .small()
                .color(color::text_muted()),
        );
        ui.add_space(8.0);
        setup_rows(app, ui);
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let cid = app
                .state
                .prefs
                .discord_client_id
                .clone()
                .unwrap_or_default();
            if cid.is_empty() {
                ui.label(
                    RichText::new("invite bot: set the application id first (step 2)")
                        .size(10.0)
                        .color(color::text_muted()),
                );
            } else if button(ui, "invite bot to server", ButtonKind::Primary).clicked() {
                let url = format!(
                    "https://discord.com/oauth2/authorize?client_id={cid}&scope=bot&permissions={BOT_PERMISSIONS}"
                );
                let _ = open::that(&url);
            }
            if button(ui, "developer portal", ButtonKind::Default).clicked() {
                let _ = open::that("https://discord.com/developers/applications");
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if button(ui, "done", ButtonKind::Default).clicked() {
                    app.queue(Action::SetDiscordSetupOpen(false));
                }
            });
        });
    });
}

/// One checklist row: status dot + title + muted detail.
fn setup_row(ui: &mut Ui, ok: bool, title: &str, detail: &str) {
    ui.horizontal(|ui| {
        let tone = if ok {
            l_ui::components::badge::Tone::Success
        } else {
            l_ui::components::badge::Tone::Neutral
        };
        l_ui::components::badge::status_dot(ui, tone, title);
        ui.label(RichText::new(title).small().strong().color(color::text()));
        ui.label(RichText::new(detail).size(10.0).color(color::text_muted()));
    });
}

fn setup_rows(app: &mut LApp, ui: &mut Ui) {
    use l_core::models::BotStatus;

    // 1 — bot token.
    let bot_ok = !matches!(app.state.discord.bot_status, BotStatus::NotConfigured);
    let bot_detail = if bot_ok {
        "stored in the OS keychain"
    } else {
        "paste your bot token below"
    };
    setup_row(ui, bot_ok, "1 · bot token", bot_detail);
    if bot_ok {
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            let name = app
                .state
                .discord
                .bot_username
                .clone()
                .unwrap_or_else(|| "configured".into());
            ui.label(
                RichText::new(format!("@{name}"))
                    .small()
                    .color(color::success()),
            );
            if button(ui, "remove", ButtonKind::Danger).clicked() {
                app.queue(Action::RemoveBotToken);
            }
            if button(ui, "re-verify", ButtonKind::Ghost).clicked() {
                app.queue(Action::VerifyBot);
            }
        });
    } else {
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            let mut token = app.bot_token_input.clone();
            let resp = l_ui::components::field::field(
                ui,
                "setup-bot-token",
                &mut token,
                "MTIz… paste bot token",
                true,
            );
            app.bot_token_input = token;
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if button(ui, "save", ButtonKind::Primary).clicked() || enter {
                let t = app.bot_token_input.trim().to_string();
                app.bot_token_input.clear();
                app.queue(Action::SetBotToken(t));
            }
        });
    }
    ui.add_space(6.0);

    // 2 — application id (OAuth client id + bot invite link).
    let cid = app
        .state
        .prefs
        .discord_client_id
        .clone()
        .unwrap_or_default();
    let cid_ok = !cid.trim().is_empty();
    setup_row(
        ui,
        cid_ok,
        "2 · application id",
        "from the developer portal application page",
    );
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        let mut v = cid.clone();
        let resp = l_ui::components::field::field(
            ui,
            "setup-client-id",
            &mut v,
            "123456789012345678",
            false,
        );
        if resp.changed() {
            app.queue(Action::SetDiscordClientId(v.trim().to_string()));
        }
        ui.label(
            RichText::new("redirect: http://localhost:<port>/callback")
                .size(10.0)
                .color(color::text_muted()),
        );
    });
    ui.add_space(6.0);

    // 3 — personal account via OAuth.
    let acct_ok = app.state.discord.account.is_some();
    let acct_detail = app.state.discord.account.as_ref().map_or_else(
        || "sign in to see your servers".to_string(),
        |a| {
            format!(
                "@{}",
                a.global_name.clone().unwrap_or_else(|| a.username.clone())
            )
        },
    );
    setup_row(ui, acct_ok, "3 · your account", &acct_detail);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        let connected = matches!(
            app.state.discord.status,
            l_core::models::ConnectionStatus::Connected | l_core::models::ConnectionStatus::Demo
        );
        if connected {
            if button(ui, "disconnect", ButtonKind::Danger).clicked() {
                app.queue(Action::DisconnectDiscord);
            }
        } else if button(ui, "connect discord", ButtonKind::Primary).clicked() {
            app.queue(Action::StartDiscordOAuth);
        }
    });
    ui.add_space(6.0);

    // 4 — bot verified online.
    let online = matches!(app.state.discord.bot_status, BotStatus::Online);
    let bot_state_detail = match &app.state.discord.bot_status {
        BotStatus::Online => "verified — gateway can connect".to_string(),
        BotStatus::Configured => "token saved, verifying…".to_string(),
        BotStatus::Error(e) => format!("error: {e}"),
        BotStatus::NotConfigured => "save a token first".to_string(),
    };
    setup_row(ui, online, "4 · bot online", &bot_state_detail);
    ui.add_space(6.0);

    // 5 — gateway enabled (real-time messages + voice handshake).
    let gw = app.state.prefs.gateway_enabled || app.state.discord.gateway_state.is_some();
    setup_row(
        ui,
        gw,
        "5 · gateway",
        "real-time messages and voice — toggle in settings → discord",
    );
}

fn draw_join_invite(app: &mut LApp, ctx: &egui::Context) {
    modal(ctx, "join-invite", "join server", 400.0, |ui| {
        ui.label(
            RichText::new("paste a discord invite link or code")
                .small()
                .color(color::text_secondary()),
        );
        ui.add_space(8.0);
        let mut input = app.join_invite_input.clone();
        let resp = l_ui::components::field::field(
            ui,
            "join-invite-input",
            &mut input,
            "discord.gg/…",
            false,
        );
        app.join_invite_input = input;
        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if button(ui, "join", ButtonKind::Primary).clicked() || enter {
                let v = app.join_invite_input.clone();
                app.join_invite_input.clear();
                app.queue(Action::JoinGuildInvite(v));
            }
            if button(ui, "cancel", ButtonKind::Default).clicked() {
                app.queue(Action::SetJoinInviteOpen(false));
            }
        });
        ui.add_space(6.0);
        ui.label(
            RichText::new("requires discord sign-in with the guilds.join scope")
                .size(10.0)
                .color(color::text_muted()),
        );
    });
}

fn draw_add_integration(app: &mut LApp, ctx: &egui::Context) {
    modal(ctx, "add-integration", "add integration", 380.0, |ui| {
        ui.label(
            RichText::new(
                "Custom widget integrations run inside a sandboxed runtime.\n\
                 External code execution is disabled by default.\n\
                 To add widgets, use the built-in library or import a widget config JSON.",
            )
            .small()
            .color(color::text_secondary()),
        );
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if button(ui, "import widgets", ButtonKind::Primary).clicked() {
                app.queue(Action::SetAddIntegrationOpen(false));
                app.pick_import_file();
            }
            if button(ui, "widget library", ButtonKind::Default).clicked() {
                app.queue(Action::SetAddIntegrationOpen(false));
                app.queue(Action::Navigate(l_core::models::Page::WidgetLibrary));
            }
            if button(ui, "close", ButtonKind::Default).clicked() {
                app.queue(Action::SetAddIntegrationOpen(false));
            }
        });
    });
}

fn draw_widget_config(app: &mut LApp, ctx: &egui::Context) {
    let Some(id) = app.state.ui.configuring_widget else {
        return;
    };
    // Resolve placed + manifest.
    let found = app.active_layout().and_then(|l| {
        l.main
            .iter()
            .chain(l.dock.iter())
            .find(|w| w.id == id)
            .cloned()
    });
    let Some(placed) = found else {
        app.queue(Action::CloseWidgetConfig);
        return;
    };
    let Some(manifest) = app.registry.get(&placed.widget_type).map(|f| f.manifest()) else {
        app.queue(Action::CloseWidgetConfig);
        return;
    };
    let title = format!("{} — {}", manifest.name, placed.name);
    let settings = manifest.settings.clone();
    let merged = merged_config(manifest, &placed);

    modal(ctx, "widget-config", &title, 420.0, move |ui| {
        widget_config_body(app, ui, id, &settings, &merged);
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if button(ui, "reset to defaults", ButtonKind::Default).clicked() {
                app.queue(Action::ResetWidgetConfig(id));
            }
            if button(ui, "remove widget", ButtonKind::Danger).clicked() {
                app.queue(Action::RemoveWidget(id));
                app.queue(Action::CloseWidgetConfig);
            }
            if button(ui, "done", ButtonKind::Primary).clicked() {
                app.queue(Action::CloseWidgetConfig);
            }
        });
    });
}

fn widget_config_body(
    app: &mut LApp,
    ui: &mut Ui,
    id: WidgetInstanceId,
    settings: &[l_widgets::core::types::SettingField],
    merged: &serde_json::Value,
) {
    if settings.is_empty() {
        ui.label(
            RichText::new("no configurable settings")
                .small()
                .color(color::text_muted()),
        );
        return;
    }
    for f in settings {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&f.label).small().color(color::text_muted()));
            match &f.kind {
                SettingKind::Boolean => {
                    let mut v = merged
                        .get(&f.key)
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false);
                    if l_ui::components::toggle::toggle(ui, &mut v).changed() {
                        app.queue(Action::SetWidgetConfig {
                            id,
                            key: f.key.clone(),
                            value: serde_json::json!(v),
                        });
                    }
                }
                SettingKind::Integer { min, max } => {
                    let mut v = merged
                        .get(&f.key)
                        .and_then(serde_json::Value::as_i64)
                        .unwrap_or(*min)
                        .clamp(*min, *max);
                    if ui
                        .add(egui::DragValue::new(&mut v).range(*min..=*max))
                        .changed()
                    {
                        app.queue(Action::SetWidgetConfig {
                            id,
                            key: f.key.clone(),
                            value: serde_json::json!(v),
                        });
                    }
                }
                SettingKind::Choice { options } => {
                    let cur = merged
                        .get(&f.key)
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    egui::ComboBox::from_id_salt(format!("cfg-{}-{}", id, f.key))
                        .selected_text(if cur.is_empty() { "—" } else { &cur })
                        .show_ui(ui, |ui| {
                            for opt in options {
                                if ui.selectable_label(cur == *opt, opt).clicked() {
                                    app.queue(Action::SetWidgetConfig {
                                        id,
                                        key: f.key.clone(),
                                        value: serde_json::json!(opt),
                                    });
                                }
                            }
                        });
                }
                SettingKind::Secret => {
                    let mut s = merged
                        .get(&f.key)
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if s.is_empty() {
                        s = "••••••••".into();
                    }
                    if l_ui::components::field::field(
                        ui,
                        &format!("cfg-{}-{}", id, f.key),
                        &mut s,
                        "enter to set",
                        true,
                    )
                    .changed()
                        && s != "••••••••"
                    {
                        app.queue(Action::SetWidgetConfig {
                            id,
                            key: f.key.clone(),
                            value: serde_json::json!(s),
                        });
                    }
                }
                SettingKind::Text | SettingKind::Url => {
                    let mut s = merged
                        .get(&f.key)
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if l_ui::components::field::field(
                        ui,
                        &format!("cfg-{}-{}", id, f.key),
                        &mut s,
                        &f.label,
                        false,
                    )
                    .changed()
                    {
                        app.queue(Action::SetWidgetConfig {
                            id,
                            key: f.key.clone(),
                            value: serde_json::json!(s),
                        });
                    }
                }
            }
        });
    }
}

fn draw_import(app: &mut LApp, ctx: &egui::Context) {
    let (count, error) = app
        .state
        .ui
        .import_dialog
        .as_ref()
        .map_or((0, None), |d| (d.pending.len(), d.error.clone()));
    modal(
        ctx,
        "import-dialog",
        "import widget configs",
        400.0,
        move |ui| {
            if let Some(err) = &error {
                ui.label(RichText::new(err).small().color(color::error()));
            } else {
                ui.label(
                    RichText::new(format!("{count} widget(s) found in the file"))
                        .small()
                        .color(color::text_secondary()),
                );
                // Preview names.
                if let Some(d) = app.state.ui.import_dialog.as_ref() {
                    for entry in d.pending.iter().take(8) {
                        let wt = entry
                            .get("widget_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("?");
                        ui.label(
                            RichText::new(format!("• {wt}"))
                                .small()
                                .color(color::text_muted()),
                        );
                    }
                }
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if button(ui, "apply", ButtonKind::Primary).clicked() {
                    app.apply_import();
                }
                if button(ui, "cancel", ButtonKind::Default).clicked() {
                    app.state.ui.import_dialog = None;
                }
            });
        },
    );
}
