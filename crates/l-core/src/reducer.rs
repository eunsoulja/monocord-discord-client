//! The reducer: pure state transitions that return effects.
//!
//! All mutations flow through `reduce`. Services report back via
//! `Action::Event(AppEvent)`.

use crate::actions::{Action, NoteField, ReorderDir, SidebarKind};
use crate::effects::Effect;
use crate::events::AppEvent;
use crate::models::{
    BotStatus, ConnectionStatus, GridPos, GridSize, Layout, LayoutId, Note, Page, PlacedWidget,
    Surface, WidgetInstanceId,
};
use crate::sanitize::sanitize_display;
use crate::state::{AppState, DiscordState, Preferences, Toast, ToastKind, WidgetRuntimeState};

/// Apply an action, mutating state and returning requested effects.
pub fn reduce(state: &mut AppState, action: Action) -> Vec<Effect> {
    let mut fx = Vec::new();
    match action {
        Action::Event(ev) => reduce_event(state, ev, &mut fx),
        Action::Navigate(page) => {
            state.ui.page = Some(page);
        }
        Action::NavigateSettingsTab(tab) => {
            state.ui.page = Some(Page::Settings);
            state.ui.settings_tab = tab;
        }
        Action::TogglePalette => state.ui.palette_open = !state.ui.palette_open,
        Action::ClosePalette => state.ui.palette_open = false,
        Action::SelectGuild(id) => state.ui.selected_guild = id,
        Action::SelectChannel(id) => {
            state.ui.selected_channel.clone_from(&id);
            // Auto-load history when a text channel is selected.
            if let Some(cid) = id {
                let is_text = state
                    .discord
                    .channels
                    .iter()
                    .any(|c| c.id == *cid && c.kind == 0);
                if is_text {
                    fx.push(Effect::FetchMessages {
                        channel_id: cid.clone(),
                    });
                }
            }
        }
        Action::LoadChannelMessages(cid) => fx.push(Effect::FetchMessages {
            channel_id: cid.clone(),
        }),
        Action::SendChatMessage {
            channel_id,
            content,
        } => {
            let content = content.trim().to_string();
            if !content.is_empty() {
                fx.push(Effect::ChatSend {
                    channel_id: channel_id.clone(),
                    content,
                });
            }
        }
        Action::EditChatMessage {
            channel_id,
            message_id,
            content,
        } => fx.push(Effect::ChatEdit {
            channel_id: channel_id.clone(),
            message_id: message_id.clone(),
            content: content.clone(),
        }),
        Action::DeleteChatMessage {
            channel_id,
            message_id,
        } => fx.push(Effect::ChatDelete {
            channel_id: channel_id.clone(),
            message_id: message_id.clone(),
        }),
        Action::ReactChatMessage {
            channel_id,
            message_id,
            emoji,
            add,
        } => fx.push(Effect::ChatReact {
            channel_id: channel_id.clone(),
            message_id: message_id.clone(),
            emoji: emoji.clone(),
            add,
        }),
        Action::SendTyping { channel_id } => fx.push(Effect::ChatTyping {
            channel_id: channel_id.clone(),
        }),
        Action::JoinVoiceChannel {
            guild_id,
            channel_id,
        } => {
            fx.push(Effect::VoiceJoin {
                guild_id: guild_id.clone(),
                channel_id: channel_id.clone(),
            });
        }
        Action::LeaveVoiceChannel => fx.push(Effect::VoiceLeave),
        Action::VoiceSetFlags { muted, deafened } => {
            fx.push(Effect::VoiceFlags { muted, deafened });
        }
        Action::ToggleCategory(cat) => {
            if state.ui.collapsed_categories.contains(&cat) {
                state.ui.collapsed_categories.retain(|c| c != &cat);
            } else {
                state.ui.collapsed_categories.push(cat);
            }
        }
        Action::UpdatePrefs(prefs) => {
            state.prefs = *prefs;
            fx.push(Effect::SavePreferences(state.prefs.clone()));
        }
        Action::SettingsTab(tab) => state.ui.settings_tab = tab,
        Action::ToggleDock => {
            state.prefs.dock_visible = !state.prefs.dock_visible;
            fx.push(Effect::SavePreferences(state.prefs.clone()));
        }
        Action::ToggleSidebar(kind) => {
            match kind {
                SidebarKind::ServerRail => {
                    state.prefs.show_server_rail = !state.prefs.show_server_rail;
                }
                SidebarKind::Nav => state.prefs.show_nav_sidebar = !state.prefs.show_nav_sidebar,
            }
            fx.push(Effect::SavePreferences(state.prefs.clone()));
        }
        Action::ConfigureWidget(id) => {
            if placed_widget_exists(state, id) {
                state.ui.configuring_widget = Some(id);
            }
        }
        Action::CloseWidgetConfig => state.ui.configuring_widget = None,
        Action::SetWidgetConfig { id, key, value } => {
            if let Some(w) = find_placed_mut(state, id) {
                if let serde_json::Value::Object(map) = &mut w.config {
                    map.insert(key, value);
                }
                fx.push(Effect::SaveWidgetInstance(w.clone()));
            }
        }
        Action::ResetWidgetConfig(id) => {
            if let Some(w) = find_placed_mut(state, id) {
                w.config = serde_json::json!({});
                fx.push(Effect::SaveWidgetInstance(w.clone()));
            }
        }
        Action::RemoveWidget(id) => {
            remove_placed(state, id);
            fx.push(Effect::DeleteWidgetInstance(id));
            fx.push(Effect::SyncWidgetSubscriptions);
            state.widget_data.remove(&id);
            state.widget_states.remove(&id);
        }
        Action::AddWidget {
            widget_type,
            surface,
        } => {
            // The app layer assigns id/pos via the layout engine; here we
            // append a default placement at the end of the surface.
            let id = WidgetInstanceId::new_v4();
            let (cols, existing) = surface_widgets(state, surface);
            let pos = next_free_pos(&existing, cols);
            let placed = PlacedWidget {
                id,
                widget_type: widget_type.clone(),
                name: String::new(),
                enabled: true,
                pos,
                size: GridSize::new(4, 3),
                config: serde_json::json!({}),
                surface,
            };
            push_placed(state, placed.clone());
            fx.push(Effect::SaveWidgetInstance(placed));
            fx.push(Effect::SyncWidgetSubscriptions);
        }
        Action::ToggleLayoutEditing => state.ui.layout_editing = !state.ui.layout_editing,
        Action::DragStart(id) => state.ui.drag_widget = Some(id),
        Action::DragTo { id, pos, surface } => {
            if let Some(w) = find_placed_mut(state, id) {
                w.pos = pos;
                w.surface = surface;
                fx.push(Effect::SaveWidgetInstance(w.clone()));
            }
            state.ui.drag_widget = None;
        }
        Action::ResizeWidget { id, size, surface } => {
            if let Some(w) = find_placed_mut(state, id) {
                w.size = size;
                w.surface = surface;
                fx.push(Effect::SaveWidgetInstance(w.clone()));
            }
        }
        Action::FocusWidget(id) => state.ui.focused_widget = id,
        Action::NudgeWidget {
            id,
            dx,
            dy,
            dw,
            dh,
            surface,
        } => {
            if let Some(w) = find_placed_mut(state, id) {
                w.pos.x = w.pos.x.saturating_add_signed(dx);
                w.pos.y = w.pos.y.saturating_add_signed(dy);
                w.size.w = w.size.w.saturating_add_signed(dw).max(1);
                w.size.h = w.size.h.saturating_add_signed(dh).max(1);
                w.surface = surface;
                fx.push(Effect::SaveWidgetInstance(w.clone()));
            }
        }
        Action::SelectLayout(id) => {
            state.workspace.active_layout = Some(id);
            fx.push(Effect::SaveWorkspace);
        }
        Action::RenameLayout { id, name } => {
            if let Some(l) = state.workspace.layouts.iter_mut().find(|l| l.id == id) {
                l.name = sanitize_display(&name);
                fx.push(Effect::SaveLayout(l.clone()));
            }
        }
        Action::DeleteLayout(id) => {
            state.workspace.layouts.retain(|l| l.id != id);
            if state.workspace.active_layout == Some(id) {
                state.workspace.active_layout = state.workspace.layouts.first().map(|l| l.id);
            }
            fx.push(Effect::DeleteLayoutRow(id));
            fx.push(Effect::SaveWorkspace);
        }
        Action::SaveLayoutAs(name) => {
            let name = sanitize_display(&name);
            if let Some(l) = active_layout(state) {
                let mut copy = l.clone();
                copy.id = LayoutId::new_v4();
                copy.name = name;
                copy.is_default = false;
                copy.preset = None;
                state.workspace.layouts.push(copy.clone());
                state.workspace.active_layout = Some(copy.id);
                fx.push(Effect::SaveLayout(copy));
                fx.push(Effect::SaveWorkspace);
            }
        }
        Action::ApplyPreset(preset) => {
            // The widget layer supplies preset construction; the app intercepts
            // this action before it reaches the reducer when it knows the presets.
            fx.push(Effect::ServiceToast {
                text: format!("preset:{preset}"),
                kind: "info".into(),
            });
        }
        Action::NewNote => {
            let note = Note {
                id: uuid::Uuid::new_v4(),
                title: String::new(),
                body: String::new(),
                workspace: Some(state.prefs.workspace_name.clone()),
                updated_at: now_secs(),
            };
            state.workspace.notes.push(note.clone());
            fx.push(Effect::SaveNote(note));
        }
        Action::UpdateNote { id, field, text } => {
            let text = sanitize_display(&text);
            if let Some(n) = state.workspace.notes.iter_mut().find(|n| n.id == id) {
                match field {
                    NoteField::Title => n.title = text,
                    NoteField::Body => n.body = text,
                }
                n.updated_at = now_secs();
                fx.push(Effect::SaveNote(n.clone()));
            }
        }
        Action::DeleteNote(id) => {
            state.workspace.notes.retain(|n| n.id != id);
            fx.push(Effect::DeleteNoteRow(id));
        }
        Action::SetNotificationRead { id, read } => {
            if let Some(n) = state
                .workspace
                .notifications
                .iter_mut()
                .find(|n| n.id == id)
            {
                n.read = read;
                fx.push(Effect::SaveNotification(n.clone()));
            }
        }
        Action::MarkAllNotificationsRead => {
            for n in &mut state.workspace.notifications {
                n.read = true;
            }
            fx.push(Effect::SaveWorkspace);
        }
        Action::ClearNotifications => {
            state.workspace.notifications.clear();
            fx.push(Effect::ClearNotificationsRow);
        }
        Action::DeleteNotification(id) => {
            state.workspace.notifications.retain(|n| n.id != id);
            fx.push(Effect::DeleteNotificationRow(id));
        }
        Action::Toast { text, kind } => {
            state.ui.toasts.push(Toast {
                text: sanitize_display(&text),
                kind,
                created: std::time::Instant::now(),
            });
            if state.ui.toasts.len() > 8 {
                state.ui.toasts.remove(0);
            }
        }
        Action::DismissToast(i) => {
            if i < state.ui.toasts.len() {
                state.ui.toasts.remove(i);
            }
        }
        Action::Confirm(dlg) => state.ui.confirm = Some(*dlg),
        Action::DismissConfirm => state.ui.confirm = None,
        Action::SetDiscordSetupOpen(b) => state.ui.discord_setup_open = b,
        Action::SetJoinInviteOpen(b) => state.ui.join_invite_open = b,
        Action::JoinGuildInvite(input) => {
            state.ui.join_invite_open = false;
            let input = input.trim().to_string();
            if input.is_empty() {
                fx.push(Effect::ServiceToast {
                    text: "enter an invite link or code".into(),
                    kind: "warning".into(),
                });
            } else {
                fx.push(Effect::JoinInvite { input });
            }
        }
        Action::SetAddIntegrationOpen(b) => state.ui.add_integration_open = b,
        Action::StartDiscordOAuth => {
            state.discord.status = ConnectionStatus::Connecting;
            match &state.prefs.discord_client_id {
                Some(cid) if !cid.is_empty() => {
                    fx.push(Effect::StartOAuth {
                        client_id: cid.clone(),
                        port: state.prefs.discord_redirect_port,
                        redirect_uri: state.prefs.discord_redirect_uri.clone(),
                    });
                }
                _ => {
                    state.discord.status = ConnectionStatus::Disconnected;
                    fx.push(Effect::ServiceToast {
                        text: "Set a Discord Client ID in Settings → Discord first.".into(),
                        kind: "warning".into(),
                    });
                }
            }
        }
        Action::OpenDiscordClient => fx.push(Effect::OpenDiscordClient),
        Action::RestartDiscordClient => fx.push(Effect::RestartDiscordClient),
        Action::CancelDiscordOAuth => fx.push(Effect::CancelOAuth),
        Action::DisconnectDiscord => {
            state.discord = DiscordState::default();
            state.discord.status = ConnectionStatus::Disconnected;
            fx.push(Effect::DisconnectDiscord);
            fx.push(Effect::SaveWorkspace);
        }
        Action::SetBotToken(token) => {
            let token = token.trim().to_string();
            if token.is_empty() {
                return fx;
            }
            fx.push(Effect::StoreBotToken(token));
            fx.push(Effect::VerifyBot);
        }
        Action::VerifyBot => fx.push(Effect::VerifyBot),
        Action::RemoveBotToken => {
            state.discord.bot_status = BotStatus::NotConfigured;
            fx.push(Effect::DeleteBotToken);
        }
        Action::SetDiscordClientId(cid) => {
            state.prefs.discord_client_id = Some(sanitize_display(&cid));
            fx.push(Effect::SavePreferences(state.prefs.clone()));
        }
        Action::ImportWidgetConfigs(json) => match validate_widget_import(&json) {
            Ok(_) => {
                state.ui.import_dialog = Some(crate::state::ImportDialogState {
                    pending: vec![serde_json::from_str(&json).unwrap_or_default()],
                    error: None,
                });
            }
            Err(e) => {
                state.ui.import_dialog = Some(crate::state::ImportDialogState {
                    pending: vec![],
                    error: Some(e),
                });
            }
        },
        Action::ExportWidgetConfigs => {
            let exported = export_widget_configs(state);
            fx.push(Effect::PickExportFile);
            fx.push(Effect::WriteExportFile {
                name: "l-widgets.json".into(),
                contents: exported,
            });
        }
        Action::WidgetData { id, data } => {
            state.widget_data.insert(id, data);
            state.widget_states.insert(id, WidgetRuntimeState::Ready);
        }
        Action::WidgetState { id, state: ws } => {
            state.widget_states.insert(id, ws);
        }
        Action::WidgetRemoved(id) => {
            state.widget_data.remove(&id);
            state.widget_states.remove(&id);
        }
        Action::RefreshWidget(id) => fx.push(Effect::RefreshWidget(id)),
        Action::RunCommand(_) => {}
        Action::PluginCommand { plugin, command } => {
            fx.push(Effect::PluginInvoke { plugin, command });
        }
        Action::ToggleDemoMode => {
            state.prefs.demo_mode = !state.prefs.demo_mode;
            fx.push(Effect::SavePreferences(state.prefs.clone()));
        }
        Action::ResetSettings => {
            let ws_name = state.prefs.workspace_name.clone();
            let client_id = state.prefs.discord_client_id.clone();
            let port = state.prefs.discord_redirect_port;
            state.prefs = Preferences::default();
            state.prefs.workspace_name = ws_name;
            state.prefs.discord_client_id = client_id;
            state.prefs.discord_redirect_port = port;
            fx.push(Effect::SavePreferences(state.prefs.clone()));
        }
        Action::OpenUrl(url) => {
            if is_safe_https_url(&url) {
                fx.push(Effect::OpenUrl(url));
            } else {
                fx.push(Effect::ServiceToast {
                    text: "Refused to open non-https URL.".into(),
                    kind: "warning".into(),
                });
            }
        }
        Action::OsNotify(id) => {
            if let Some(n) = state.workspace.notifications.iter().find(|n| n.id == id) {
                fx.push(Effect::OsNotification {
                    title: n.title.clone(),
                    body: n.body.clone(),
                });
            }
        }
        Action::Audit(entry) => {
            state.workspace.audit_log.push(entry);
            if state.workspace.audit_log.len() > 500 {
                let excess = state.workspace.audit_log.len() - 500;
                state.workspace.audit_log.drain(..excess);
            }
            fx.push(Effect::AppendAudit(
                state.workspace.audit_log.last().cloned().unwrap(),
            ));
        }
        Action::ToggleWidgetEnabled(id) => {
            if let Some(w) = find_placed_mut(state, id) {
                w.enabled = !w.enabled;
                fx.push(Effect::SaveWidgetInstance(w.clone()));
            }
        }
        Action::MoveWidgetToSurface { id, surface } => {
            if placed_widget_exists(state, id) {
                let (cols, existing) = surface_widgets(state, surface);
                let existing: Vec<PlacedWidget> =
                    existing.into_iter().filter(|w| w.id != id).collect();
                let pos = next_free_pos(&existing, cols);
                if let Some(w) = find_placed_mut(state, id) {
                    w.surface = surface;
                    w.pos = pos;
                    fx.push(Effect::SaveWidgetInstance(w.clone()));
                }
            }
        }
        Action::DuplicateWidget(id) => {
            if let Some(w) = find_placed(state, id).cloned() {
                let mut copy = w.clone();
                copy.id = WidgetInstanceId::new_v4();
                copy.name = format!("{} (copy)", w.name);
                let (cols, existing) = surface_widgets(state, copy.surface);
                copy.pos = next_free_pos(&existing, cols);
                push_placed(state, copy.clone());
                fx.push(Effect::SaveWidgetInstance(copy));
                fx.push(Effect::SyncWidgetSubscriptions);
            }
        }
        Action::ReorderWidget { id, dir, surface } => {
            reorder_widget(state, id, dir, surface);
            fx.push(Effect::SaveWorkspace);
        }
    }
    fx
}

fn reduce_event(state: &mut AppState, ev: AppEvent, fx: &mut Vec<Effect>) {
    use AppEvent as E;
    match ev {
        E::Hydrated(data) => {
            state.hydrated = true;
            if let Some(p) = data.prefs {
                state.prefs = p;
            }
            state.workspace.layouts = data.layouts;
            state.workspace.notes = data.notes;
            state.workspace.notifications = data.notifications;
            state.workspace.audit_log = data.audit_log;
            // Widget instances are stored inside layouts; nothing extra needed.
            let _ = data.widget_instances;
            state.workspace.active_layout = state
                .prefs
                .default_layout
                .filter(|id| state.workspace.layouts.iter().any(|l| l.id == *id))
                .or_else(|| state.workspace.layouts.first().map(|l| l.id));
        }
        E::PreferencesChanged(p) => {
            state.prefs = *p;
        }
        E::OAuthCompleted { ok, error } => {
            if ok {
                fx.push(Effect::FetchDiscordData);
            } else {
                state.discord.status = ConnectionStatus::Disconnected;
                fx.push(Effect::ServiceToast {
                    text: format!("Discord sign-in failed: {}", error.unwrap_or_default()),
                    kind: "error".into(),
                });
            }
        }
        E::DiscordDataFetched {
            account,
            guilds,
            scopes,
        } => {
            state.discord.account = account;
            state.discord.guilds = guilds;
            state.discord.granted_scopes = scopes;
            state.discord.status = if state.prefs.demo_mode {
                ConnectionStatus::Demo
            } else {
                ConnectionStatus::Connected
            };
            fx.push(Effect::SaveWorkspace);
        }
        E::BotVerified {
            ok,
            bot_user_id,
            bot_username,
            error,
        } => {
            if ok {
                state.discord.bot_status = BotStatus::Online;
                state.discord.bot_user_id = bot_user_id;
                state.discord.bot_username = bot_username;
            } else {
                state.discord.bot_status = BotStatus::Error(error.unwrap_or_default());
            }
        }
        E::RestResponse { tag, status, body } => {
            if status >= 400 {
                fx.push(Effect::ServiceToast {
                    text: format!("Discord API {status} for {tag}"),
                    kind: "warning".into(),
                });
            }
            // Widget data hub consumes rest responses via the runtime; the app
            // stores notable payloads under a synthetic widget key if needed.
            let _ = body;
        }
        E::InviteJoinResult {
            ok,
            guild_name,
            error,
        } => {
            if ok {
                fx.push(Effect::ServiceToast {
                    text: format!("joined {}", guild_name.unwrap_or_else(|| "server".into())),
                    kind: "success".into(),
                });
                fx.push(Effect::FetchDiscordData);
            } else {
                fx.push(Effect::ServiceToast {
                    text: format!("join failed: {}", error.unwrap_or_default()),
                    kind: "error".into(),
                });
            }
        }
        E::RestFailed { tag, error } => {
            fx.push(Effect::ServiceToast {
                text: format!("Discord request failed ({tag}): {error}"),
                kind: "error".into(),
            });
        }
        E::WidgetData { id, data, .. } => {
            state.widget_data.insert(id, data);
            state.widget_states.insert(id, WidgetRuntimeState::Ready);
        }
        E::WidgetFailed { id, error } => {
            state.widget_states.insert(id, WidgetRuntimeState::Error);
            fx.push(Effect::ServiceToast {
                text: format!("Widget fetch failed: {error}"),
                kind: "warning".into(),
            });
        }
        E::FileChosen { path } => {
            // Read happens in app layer; reducer just records intent via toast.
            let _ = path;
        }
        E::ExportFileChosen { path } => {
            let _ = path;
        }
        E::ServiceToast { text, kind } => {
            let toast_kind = match kind.as_str() {
                "error" => ToastKind::Error,
                "warning" => ToastKind::Warning,
                "success" => ToastKind::Success,
                _ => ToastKind::Info,
            };
            state.ui.toasts.push(Toast {
                text: sanitize_display(&text),
                kind: toast_kind,
                created: std::time::Instant::now(),
            });
        }
        E::NewNotification(n) => {
            state.workspace.notifications.push(*n);
            fx.push(Effect::SaveWorkspace);
        }
        E::GatewayUpdate {
            state: gw_state,
            latency_ms,
            connected,
        } => {
            state.discord.gateway_state = Some(gw_state.clone());
            state.discord.bot_latency_ms = latency_ms;
            if connected {
                // A live gateway implies the bot is online.
                if matches!(
                    state.discord.bot_status,
                    BotStatus::Configured | BotStatus::Online
                ) {
                    state.discord.bot_status = BotStatus::Online;
                }
            } else if state.discord.bot_status == BotStatus::Online {
                state.discord.bot_status = BotStatus::Configured;
            }
        }
        E::ChannelsLoaded { guild_id, channels } => {
            // Replace entries for this guild, keep others.
            state.discord.channels.retain(|c| c.guild_id != *guild_id);
            state.discord.channels.extend(channels.iter().cloned());
        }
        E::MessagesLoaded {
            channel_id,
            messages,
        } => {
            let mut msgs = messages.clone();
            msgs.sort_by_key(|m| m.timestamp);
            msgs.truncate(300);
            state.discord.messages.insert(channel_id.clone(), msgs);
        }
        E::ChatMessageUpsert { message } => {
            let list = state
                .discord
                .messages
                .entry(message.channel_id.clone())
                .or_default();
            if let Some(pos) = list.iter().position(|m| m.id == message.id) {
                list[pos] = message.clone();
            } else {
                list.push(message.clone());
                list.sort_by_key(|m| m.timestamp);
                if list.len() > 300 {
                    let excess = list.len() - 300;
                    list.drain(..excess);
                }
            }
        }
        E::ChatMessageDeleted {
            channel_id,
            message_id,
        } => {
            if let Some(list) = state.discord.messages.get_mut(&channel_id) {
                list.retain(|m| m.id != *message_id);
            }
        }
        E::TypingEvent { channel_id, user } => {
            let list = state.discord.typing.entry(channel_id.clone()).or_default();
            let now = now_secs();
            list.retain(|(_, ts)| now - *ts < 8);
            if !list.iter().any(|(u, _)| u == &user) {
                list.push((user.clone(), now));
            }
        }
        E::VoiceUpdate { info } => {
            if info.channel_id.is_empty() && !info.connected && !info.connecting {
                state.discord.voice = None;
            } else {
                state.discord.voice = Some(info.clone());
            }
        }

        E::Tick => {}
    }
}

// ---------- layout helpers ----------

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Find a placed widget across the active layout.
fn active_layout(state: &AppState) -> Option<&Layout> {
    let id = state
        .workspace
        .active_layout
        .or_else(|| state.workspace.layouts.first().map(|l| l.id))?;
    state.workspace.layouts.iter().find(|l| l.id == id)
}

fn active_layout_mut(state: &mut AppState) -> Option<&mut Layout> {
    let id = state
        .workspace
        .active_layout
        .or_else(|| state.workspace.layouts.first().map(|l| l.id))?;
    state.workspace.layouts.iter_mut().find(|l| l.id == id)
}

fn placed_widget_exists(state: &AppState, id: WidgetInstanceId) -> bool {
    active_layout(state).is_some_and(|l| l.main.iter().chain(l.dock.iter()).any(|w| w.id == id))
}

fn find_placed(state: &AppState, id: WidgetInstanceId) -> Option<&PlacedWidget> {
    active_layout(state)?
        .main
        .iter()
        .chain(active_layout(state)?.dock.iter())
        .find(|w| w.id == id)
}

fn find_placed_mut(state: &mut AppState, id: WidgetInstanceId) -> Option<&mut PlacedWidget> {
    let l = active_layout_mut(state)?;
    l.main
        .iter_mut()
        .chain(l.dock.iter_mut())
        .find(|w| w.id == id)
}

fn surface_widgets(state: &AppState, surface: Surface) -> (u32, Vec<PlacedWidget>) {
    if let Some(l) = active_layout(state) {
        let (cols, v) = match surface {
            Surface::Main => (l.main_columns, &l.main),
            Surface::Dock => (l.dock_columns, &l.dock),
        };
        (cols.max(1), v.clone())
    } else {
        (4, Vec::new())
    }
}

fn push_placed(state: &mut AppState, w: PlacedWidget) {
    if let Some(l) = active_layout_mut(state) {
        match w.surface {
            Surface::Main => l.main.push(w),
            Surface::Dock => l.dock.push(w),
        }
    }
}

fn remove_placed(state: &mut AppState, id: WidgetInstanceId) {
    if let Some(l) = active_layout_mut(state) {
        l.main.retain(|w| w.id != id);
        l.dock.retain(|w| w.id != id);
    }
}

/// Find a free grid position below existing widgets.
fn next_free_pos(existing: &[PlacedWidget], cols: u32) -> GridPos {
    let mut y = 0u32;
    loop {
        for x in 0..cols {
            let candidate = GridPos { x, y };
            let free = existing.iter().all(|w| {
                !(candidate.x < w.pos.x + w.size.w
                    && candidate.x + 4 > w.pos.x
                    && candidate.y < w.pos.y + w.size.h
                    && candidate.y + 3 > w.pos.y)
            });
            if free {
                return candidate;
            }
        }
        y += 1;
        if y > 1000 {
            return GridPos { x: 0, y: 0 };
        }
    }
}

fn reorder_widget(state: &mut AppState, id: WidgetInstanceId, dir: ReorderDir, surface: Surface) {
    let Some(l) = active_layout_mut(state) else {
        return;
    };
    let v = match surface {
        Surface::Main => &mut l.main,
        Surface::Dock => &mut l.dock,
    };
    if let Some(i) = v.iter().position(|w| w.id == id) {
        let j = match dir {
            ReorderDir::Up => i.saturating_sub(1),
            ReorderDir::Down => (i + 1).min(v.len().saturating_sub(1)),
        };
        v.swap(i, j);
    }
}

fn export_widget_configs(state: &AppState) -> String {
    let widgets: Vec<serde_json::Value> = active_layout(state).map_or_default(|l| {
        l.main
            .iter()
            .chain(l.dock.iter())
            .map(|w| {
                serde_json::json!({
                    "widget_type": w.widget_type,
                    "name": w.name,
                    "config": w.config,
                    "surface": w.surface,
                    "size": {"w": w.size.w, "h": w.size.h},
                })
            })
            .collect()
    });
    serde_json::to_string_pretty(&serde_json::json!({
        "schema_version": 1,
        "app": "l",
        "widgets": widgets,
    }))
    .unwrap_or_else(|_| "{}".into())
}

/// Validate an imported widget-config file. Data only — never code.
pub fn validate_widget_import(json: &str) -> Result<Vec<serde_json::Value>, String> {
    // Enforce a size cap before parsing.
    if json.len() > 1024 * 1024 {
        return Err("import too large".into());
    }
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let obj = v.as_object().ok_or("top-level must be an object")?;
    if obj
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        != Some(1)
    {
        return Err("unsupported schema_version".into());
    }
    let widgets = obj
        .get("widgets")
        .and_then(|w| w.as_array())
        .ok_or("missing widgets array")?;
    if widgets.len() > 256 {
        return Err("too many widgets".into());
    }
    Ok(widgets.clone())
}

fn is_safe_https_url(url: &str) -> bool {
    url.starts_with("https://") && url.len() < 2048 && !url.chars().any(char::is_control)
}
