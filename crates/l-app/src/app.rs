//! The eframe application: owns state, services, widget manager, and the
//! action/effect pipeline.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;
use l_core::actions::Action;
use l_core::effects::Effect;
use l_core::events::AppEvent;
use l_core::models::{
    GridPos, GridSize, Layout, LayoutId, Note, Notification, PlacedWidget, Surface,
    WidgetInstanceId,
};
use l_core::reducer::reduce;
use l_core::state::{AppState, WidgetRuntimeState};
use l_runtime::hub::{DataHub, HubEvent};
use l_widgets::core::layout;
use l_widgets::core::manager::WidgetManager;
use l_widgets::core::registry::{WidgetRegistry, register_all};
use l_widgets::core::types::{LibraryFilter, WidgetData};
use l_widgets::renderer::{RenderCx, draw_widget};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

use crate::backend::{self, BackendShared};
use crate::{dialogs, pages, palette, shell};

/// Main application.
pub struct LApp {
    pub(crate) state: AppState,
    shared: Arc<BackendShared>,
    pub(crate) registry: WidgetRegistry,
    manager: WidgetManager,
    hub: DataHub,
    app_rx: mpsc::UnboundedReceiver<AppEvent>,
    /// `need_key` -> subscriber widget ids (for hub event routing).
    need_subs: HashMap<String, Vec<WidgetInstanceId>>,
    /// Actions queued by UI drawing code (dispatched after the frame section).
    /// `Rc<RefCell>` so render callbacks can push without borrowing `self`.
    pub(crate) pending: Rc<RefCell<Vec<Action>>>,
    /// Whether the default layout has been ensured.
    default_layout_done: bool,
    /// Local-data publish dirty flag.
    local_dirty: bool,
    /// Whether widget sync is needed.
    widgets_dirty: bool,
    /// Library page state.
    pub library_filter: LibraryFilter,
    pub library_search: String,
    /// Notifications page source filter.
    pub notif_filter: String,
    /// Join-server dialog text input.
    pub join_invite_input: String,
    /// Bot token entry field in the setup dialog / settings (write-only).
    pub bot_token_input: String,
    /// Command palette query + selection.
    pub palette_query: String,
    pub palette_sel: usize,
    /// Drag/resize accumulators for edit mode (`RefCell`: render callbacks
    /// share it without conflicting mutable borrows).
    edit_drag: RefCell<HashMap<WidgetInstanceId, EditDrag>>,
    /// Last computed cell size for drag conversion.
    cell_size: (f32, f32),
    /// Tokio runtime kept alive for the app's lifetime.
    _rt: Runtime,
    /// Keeps a runtime context on the UI thread so hub pollers can `spawn`.
    _runtime_enter: tokio::runtime::EnterGuard<'static>,
}

struct EditDrag {
    pos: GridPos,
    size: GridSize,
    acc: egui::Vec2,
}

impl LApp {
    /// Create the app inside eframe.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        l_ui::fonts::install_fonts(&cc.egui_ctx);
        let mut registry = WidgetRegistry::new();
        register_all(&mut registry);

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .build()
            .expect("tokio runtime");
        let (app_tx, app_rx) = mpsc::unbounded_channel::<AppEvent>();
        let handle = rt.handle().clone();
        let (shared, fetch) = backend::build(handle.clone(), app_tx);
        // Local Discord RPC endpoint for rich presence (SET_ACTIVITY etc.).
        crate::rpc::spawn(&shared);
        // Register plugin-provided widget views (Legcord-style plugin widgets).
        crate::plugins::register_plugin_widgets(&mut registry, &shared.plugins);
        let hub = DataHub::new(fetch, Duration::from_secs(60));
        // Hub pollers are spawned from the UI thread; keep a runtime context.
        // Leak one Handle clone so the EnterGuard can be 'static.
        let enter_handle: &'static tokio::runtime::Handle = Box::leak(Box::new(handle.clone()));
        let runtime_enter = enter_handle.enter();

        // Mirror initial prefs into shared.
        shared
            .demo
            .store(true, std::sync::atomic::Ordering::Relaxed);

        let mut app = Self {
            state: AppState::default(),
            shared,
            registry,
            manager: WidgetManager::new(),
            hub,
            app_rx,
            need_subs: HashMap::new(),
            pending: Rc::new(RefCell::new(Vec::new())),
            default_layout_done: false,
            local_dirty: true,
            widgets_dirty: true,
            library_filter: LibraryFilter::All,
            library_search: String::new(),
            notif_filter: "all".into(),
            join_invite_input: String::new(),
            bot_token_input: String::new(),
            palette_query: String::new(),
            palette_sel: 0,
            edit_drag: RefCell::new(HashMap::new()),
            cell_size: (96.0, 72.0),
            _rt: rt,
            _runtime_enter: runtime_enter,
        };
        backend::hydrate(&app.shared);
        // `l-app --discord` or `L_OPEN_DISCORD=1` opens the Discord client
        // window alongside the dashboard (deferred through the action queue).
        if std::env::args().any(|a| a == "--discord" || a == "discord")
            || std::env::var_os("L_OPEN_DISCORD").is_some()
        {
            app.queue(Action::OpenDiscordClient);
        }
        app
    }

    /// Queue an action for end-of-frame dispatch.
    pub(crate) fn queue(&mut self, action: Action) {
        self.pending.borrow_mut().push(action);
    }

    /// Shared plugin host.
    pub(crate) fn plugin_host(&self) -> &Arc<l_plugins::PluginHost> {
        &self.shared.plugins
    }

    /// Rescan the plugins directory and (re)register widget factories.
    /// Rescan is a fast filesystem pass, safe on the UI thread.
    pub(crate) fn rescan_plugin_widgets(&mut self) {
        self.shared.plugins.rescan();
        crate::plugins::register_plugin_widgets(&mut self.registry, &self.shared.plugins);
        self.widgets_dirty = true;
    }

    /// Close whichever overlay/dialog is currently open (Escape cascade).
    fn dismiss_overlays(&mut self) {
        if self.state.ui.palette_open {
            self.queue(Action::ClosePalette);
        }
        if self.state.ui.configuring_widget.is_some() {
            self.queue(Action::CloseWidgetConfig);
        }
        if self.state.ui.confirm.is_some() {
            self.queue(Action::DismissConfirm);
        }
        if self.state.ui.discord_setup_open {
            self.queue(Action::SetDiscordSetupOpen(false));
        }
        if self.state.ui.import_dialog.is_some() {
            self.state.ui.import_dialog = None;
        }
    }

    /// Tokio handle for spawning background work from UI code.
    pub(crate) fn tokio(&self) -> &tokio::runtime::Handle {
        &self.shared.tokio
    }

    /// Dispatch immediately (used for service events).
    fn dispatch_now(&mut self, action: Action) {
        // Intercept actions that need app-level handling.
        match &action {
            Action::SelectGuild(Some(g)) => {
                let g = g.clone();
                backend::fetch_channels(&self.shared, g);
            }
            Action::ApplyPreset(name) => {
                let name = name.clone();
                self.apply_preset(&name);
                return;
            }
            _ => {}
        }
        let fx = reduce(&mut self.state, action);
        self.after_reduce(&fx);
        self.handle_effects(fx);
    }

    /// Apply a named preset: rebuild the active layout's contents.
    fn apply_preset(&mut self, name: &str) {
        if self.state.workspace.layouts.is_empty() {
            self.ensure_default_layout();
        }
        let widgets = l_widgets::presets::preset_widgets(name, &self.registry);
        if widgets.is_empty() {
            self.queue(Action::Toast {
                text: format!("unknown preset: {name}"),
                kind: l_core::state::ToastKind::Warning,
            });
            return;
        }
        let Some(layout) = self.active_layout_mut() else {
            return;
        };
        let (main, dock): (Vec<_>, Vec<_>) = widgets
            .into_iter()
            .partition(|w| w.surface == Surface::Main);
        layout.main = main;
        layout.dock = dock;
        layout.preset = Some(name.to_string());
        let snapshot = layout.clone();
        let db = self.shared.db.clone();
        backend::db_call(&db, move |c| {
            let _ = l_storage::repos::save_layout(c, &snapshot);
        });
        self.widgets_dirty = true;
        self.queue(Action::Toast {
            text: format!("preset applied: {name}"),
            kind: l_core::state::ToastKind::Success,
        });
    }

    /// Mirror state into backend shared + mark sync needs.
    fn after_reduce(&mut self, fx: &[Effect]) {
        // Mirror prefs-derived flags.
        self.shared.demo.store(
            self.state.prefs.demo_mode,
            std::sync::atomic::Ordering::Relaxed,
        );
        self.shared.gateway_enabled.store(
            self.state.prefs.gateway_enabled,
            std::sync::atomic::Ordering::Relaxed,
        );
        self.shared.rpc_enabled.store(
            self.state.prefs.rpc_enabled,
            std::sync::atomic::Ordering::Relaxed,
        );
        *self.shared.intents.lock().unwrap() = (
            self.state.prefs.intent_guild_members,
            self.state.prefs.intent_presences,
            self.state.prefs.intent_message_content,
        );
        self.shared
            .client_id
            .lock()
            .unwrap()
            .clone_from(&self.state.prefs.discord_client_id);
        self.shared
            .selected_guild
            .lock()
            .unwrap()
            .clone_from(&self.state.ui.selected_guild);

        for f in fx {
            match f {
                Effect::SyncWidgetSubscriptions
                | Effect::RefreshWidget(_)
                | Effect::SaveWidgetInstance(_)
                | Effect::DeleteWidgetInstance(_) => self.widgets_dirty = true,
                _ => {}
            }
        }
        // Local data may have changed on most actions; cheap to republish only
        // when flagged — flag on note/notification-ish actions too.
        self.local_dirty = true;
    }

    /// Partition and execute effects.
    fn handle_effects(&mut self, fx: Vec<Effect>) {
        let mut backend_fx = Vec::new();
        for f in fx {
            match f {
                Effect::SyncWidgetSubscriptions => {
                    self.widgets_dirty = true;
                }
                Effect::RefreshWidget(id) => self.refresh_widget(id),
                Effect::SaveWidgetInstance(w) => self.save_widget_instance(&w),
                Effect::DeleteWidgetInstance(id) => {
                    self.manager.remove(id);
                    self.state.widget_data.remove(&id);
                    self.state.widget_states.remove(&id);
                }
                Effect::PickImportFile => self.pick_import_file(),
                Effect::PickExportFile => self.pick_export_file(),
                other => backend_fx.push(other),
            }
        }
        backend::execute_effects(&self.shared, backend_fx);
        if self.state.prefs.gateway_enabled {
            backend::ensure_gateway(&self.shared);
        }
    }

    /// Persist a placed widget by saving its parent layout.
    fn save_widget_instance(&self, w: &PlacedWidget) {
        let Some(layout) = self
            .state
            .workspace
            .layouts
            .iter()
            .find(|l| l.main.iter().chain(l.dock.iter()).any(|x| x.id == w.id))
            .cloned()
        else {
            return;
        };
        let db = self.shared.db.clone();
        backend::db_call(&db, move |c| l_storage::repos::save_layout(c, &layout));
    }

    /// Kick the hub pollers subscribed to a widget's needs.
    fn refresh_widget(&self, id: WidgetInstanceId) {
        if let Some(m) = self.manager.get(id) {
            for need in m.instance.data_needs() {
                self.hub.refresh_key(&need.key());
            }
        }
    }

    pub(crate) fn pick_import_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("JSON", &["json"])
            .pick_file()
        {
            match std::fs::read_to_string(&path) {
                Ok(text) => self.queue(Action::ImportWidgetConfigs(text)),
                Err(e) => self.queue(Action::Toast {
                    text: format!("import read failed: {e}"),
                    kind: l_core::state::ToastKind::Error,
                }),
            }
        }
    }

    fn pick_export_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("JSON", &["json"])
            .set_file_name("l-widgets.json")
            .save_file()
        {
            *self.shared.export_path.lock().unwrap() = Some(path);
        }
    }

    /// Apply a confirmed widget import (called from the import dialog).
    pub fn apply_import(&mut self) {
        let pending = self
            .state
            .ui
            .import_dialog
            .take()
            .map_or_default(|d| d.pending);
        for entry in pending {
            let widget_type = entry
                .get("widget_type")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if widget_type.is_empty() || self.registry.get(&widget_type).is_none() {
                continue;
            }
            let surface = match entry.get("surface").and_then(|v| v.as_str()) {
                Some("dock") => Surface::Dock,
                _ => Surface::Main,
            };
            let size = entry.get("size").cloned().unwrap_or(serde_json::json!({}));
            let w = size
                .get("w")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(4) as u32;
            let h = size
                .get("h")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(3) as u32;
            let name = entry
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let config = entry
                .get("config")
                .cloned()
                .unwrap_or(serde_json::json!({}));
            self.add_placed_widget(PlacedWidget {
                id: WidgetInstanceId::new_v4(),
                widget_type,
                name,
                enabled: true,
                pos: GridPos { x: 0, y: 0 },
                size: GridSize::new(w.max(2), h.max(2)),
                config,
                surface,
            });
        }
        self.queue(Action::Toast {
            text: "widgets imported".into(),
            kind: l_core::state::ToastKind::Success,
        });
    }

    /// Insert a placed widget at the first free position and persist.
    pub fn add_placed_widget(&mut self, mut w: PlacedWidget) {
        let active = self.state.workspace.active_layout;
        let idx = self
            .state
            .workspace
            .layouts
            .iter()
            .position(|l| Some(l.id) == active)
            .unwrap_or(0);
        if self.state.workspace.layouts.is_empty() {
            return;
        }
        let layout = &mut self.state.workspace.layouts[idx];
        let cols = match w.surface {
            Surface::Main => layout.main_columns,
            Surface::Dock => layout.dock_columns,
        };
        let existing: Vec<PlacedWidget> = match w.surface {
            Surface::Main => layout.main.clone(),
            Surface::Dock => layout.dock.clone(),
        };
        w.pos = layout::first_free(&existing, w.size, cols, w.surface);
        match w.surface {
            Surface::Main => layout.main.push(w.clone()),
            Surface::Dock => layout.dock.push(w.clone()),
        }
        let layout_snapshot = layout.clone();
        let db = self.shared.db.clone();
        backend::db_call(&db, move |c| {
            let _ = l_storage::repos::save_layout(c, &layout_snapshot);
        });
        self.widgets_dirty = true;
    }

    /// The active layout (or first).
    pub fn active_layout(&self) -> Option<&Layout> {
        let id = self
            .state
            .workspace
            .active_layout
            .or_else(|| self.state.workspace.layouts.first().map(|l| l.id))?;
        self.state.workspace.layouts.iter().find(|l| l.id == id)
    }

    fn active_layout_mut(&mut self) -> Option<&mut Layout> {
        let id = self
            .state
            .workspace
            .active_layout
            .or_else(|| self.state.workspace.layouts.first().map(|l| l.id))?;
        self.state.workspace.layouts.iter_mut().find(|l| l.id == id)
    }

    /// All placed widgets across surfaces.
    pub fn placed_widgets(&self) -> Vec<PlacedWidget> {
        self.active_layout()
            .map_or_default(|l| l.main.iter().chain(l.dock.iter()).cloned().collect())
    }

    /// Sync widget manager + hub subscriptions.
    fn sync_widgets(&mut self) {
        let placed = self.placed_widgets();
        self.manager.sync(&placed, &self.registry);
        let mut needs = Vec::new();
        let mut subs: HashMap<String, Vec<WidgetInstanceId>> = HashMap::new();
        for w in &placed {
            if let Some(m) = self.manager.get(w.id) {
                for need in m.instance.data_needs() {
                    let key = need.key();
                    subs.entry(key.clone()).or_default().push(w.id);
                    needs.push((w.id, need, m.instance.refresh_secs()));
                }
            }
        }
        self.need_subs = subs;
        self.hub.sync(needs);
        self.widgets_dirty = false;
    }

    /// Publish local data (notes, notifications) to hub subscribers.
    fn publish_local(&mut self) {
        if !self.local_dirty {
            return;
        }
        self.local_dirty = false;
        let notifs: Vec<Notification> = self
            .state
            .workspace
            .notifications
            .iter()
            .rev()
            .take(100)
            .cloned()
            .collect();
        let unread = self
            .state
            .workspace
            .notifications
            .iter()
            .filter(|n| !n.read)
            .count() as u64;
        let notif_json = serde_json::json!({
            "notifications": notifs,
            "unread": unread,
        });

        let mut cache = self.shared.local_cache.lock().unwrap();
        cache.insert("local.notifications".into(), notif_json.clone());
        self.hub.publish("local.notifications", notif_json);

        // Publish notes for the wildcard scope plus each workspace scope.
        let mut scopes: Vec<Option<String>> = vec![None];
        for n in &self.state.workspace.notes {
            if let Some(ws) = &n.workspace
                && !scopes.contains(&Some(ws.clone()))
            {
                scopes.push(Some(ws.clone()));
            }
        }
        scopes.push(Some(self.state.prefs.workspace_name.clone()));
        scopes.sort();
        scopes.dedup();
        for scope in scopes {
            let key = format!("local.notes.{}", scope.as_deref().unwrap_or("*"));
            let filtered: Vec<Note> = match &scope {
                None => self.state.workspace.notes.clone(),
                Some(ws) => self
                    .state
                    .workspace
                    .notes
                    .iter()
                    .filter(|n| n.workspace.as_deref() == Some(ws.as_str()))
                    .cloned()
                    .collect(),
            };
            let payload = serde_json::json!({"notes": filtered});
            cache.insert(key.clone(), payload.clone());
            self.hub.publish(&key, payload);
        }
    }

    /// Drain app events and hub events into actions.
    fn drain_events(&mut self) {
        while let Ok(ev) = self.app_rx.try_recv() {
            self.dispatch_now(Action::Event(ev));
        }
        for ev in self.hub.drain() {
            match ev {
                HubEvent::Data { need_key, data } => {
                    if let Some(ids) = self.need_subs.get(&need_key).cloned() {
                        for id in ids {
                            self.dispatch_now(Action::WidgetData {
                                id,
                                data: data.clone(),
                            });
                            if let Some(m) = self.manager.get_mut(id) {
                                m.state = WidgetRuntimeState::Ready;
                                m.error = None;
                            }
                        }
                    }
                }
                HubEvent::Failed { need_key, error } => {
                    if let Some(ids) = self.need_subs.get(&need_key).cloned() {
                        for id in ids {
                            self.dispatch_now(Action::WidgetState {
                                id,
                                state: WidgetRuntimeState::Error,
                            });
                            if let Some(m) = self.manager.get_mut(id) {
                                m.state = WidgetRuntimeState::Error;
                                m.error = Some(error.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    /// Drain plugin host events (notifications, logs) into toasts/logs.
    fn drain_plugin_events(&mut self) {
        for ev in self.shared.plugins.drain_events() {
            match ev {
                l_plugins::PluginEvent::Notify { title, body, .. } => {
                    let text = if title.is_empty() {
                        body
                    } else if body.is_empty() {
                        title
                    } else {
                        format!("{title}: {body}")
                    };
                    if !text.is_empty() {
                        self.queue(Action::Toast {
                            text,
                            kind: l_core::state::ToastKind::Info,
                        });
                    }
                }
                l_plugins::PluginEvent::Log {
                    plugin,
                    level,
                    message,
                } => {
                    tracing::info!(plugin, level, "{message}");
                }
            }
        }
    }

    /// Ensure a default layout exists after hydration.
    fn ensure_default_layout(&mut self) {
        if !self.state.workspace.layouts.is_empty() {
            return;
        }
        let widgets = l_widgets::presets::preset_widgets("server_management", &self.registry);
        let (main, dock): (Vec<_>, Vec<_>) = widgets
            .into_iter()
            .partition(|w| w.surface == Surface::Main);
        let layout = Layout {
            id: LayoutId::new_v4(),
            name: "default".into(),
            is_default: true,
            preset: Some("server_management".into()),
            main,
            dock,
            main_columns: 12,
            dock_columns: 4,
        };
        self.state.workspace.layouts.push(layout.clone());
        self.state.workspace.active_layout = Some(layout.id);
        self.state.prefs.default_layout = Some(layout.id);
        let prefs = self.state.prefs.clone();
        let db = self.shared.db.clone();
        backend::db_call(&db, move |c| {
            let _ = l_storage::repos::save_layout(c, &layout);
            let _ = l_storage::repos::save_preferences(c, &prefs);
        });
        self.widgets_dirty = true;
    }

    /// Draw the main widget grid for a surface.
    pub(crate) fn draw_surface(
        &mut self,
        ui: &mut egui::Ui,
        surface: Surface,
        cols: u32,
        edit_mode: bool,
    ) {
        let Some(layout) = self.active_layout() else {
            l_ui::components::empty_state::empty_state(
                ui,
                l_ui::icons::Icon::LayoutDashboard,
                "no layout",
                "create one in settings",
            );
            return;
        };
        let placed: Vec<PlacedWidget> = match surface {
            Surface::Main => layout.main.clone(),
            Surface::Dock => layout.dock.clone(),
        };
        if placed.is_empty() {
            l_ui::components::empty_state::empty_state(
                ui,
                l_ui::icons::Icon::Plus,
                "empty surface",
                "add widgets from the library",
            );
            return;
        }

        let avail_w = ui.available_width().max(200.0);
        let gap = 8.0f32;
        let cols = cols.max(1);
        let cell_w = (avail_w - gap * (cols as f32 - 1.0)) / cols as f32;
        let cell_h = cell_w * 0.72;
        self.cell_size = (cell_w + gap, cell_h + gap);

        let max_row = placed.iter().map(|w| w.pos.y + w.size.h).max().unwrap_or(1);
        let total_h = max_row as f32 * (cell_h + gap) + gap;

        let mut sorted = placed.clone();
        sorted.sort_by_key(|w| (w.pos.y, w.pos.x));

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_width(avail_w);
                ui.set_min_height(total_h);
                for w in &sorted {
                    let rect = egui::Rect::from_min_size(
                        egui::pos2(
                            w.pos.x as f32 * (cell_w + gap) + gap * 0.0,
                            w.pos.y as f32 * (cell_h + gap),
                        ),
                        egui::vec2(
                            w.size.w as f32 * cell_w + (w.size.w as f32 - 1.0) * gap,
                            w.size.h as f32 * cell_h + (w.size.h as f32 - 1.0) * gap,
                        ),
                    );
                    self.draw_one_widget(ui, w, rect, edit_mode, surface);
                }
            });
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_one_widget(
        &mut self,
        ui: &mut egui::Ui,
        w: &PlacedWidget,
        rect: egui::Rect,
        edit_mode: bool,
        surface: Surface,
    ) {
        let manifest = self.registry.get(&w.widget_type).map(|f| f.manifest());
        let data = self
            .state
            .widget_data
            .get(&w.id)
            .map_or_default(|v| WidgetData { json: v.clone() });

        // Disjoint field borrows: state/registry/cell_size immutable,
        // manager + edit_drag + pending shared via RefCell/Rc.
        let state = &self.state;
        let animation = match state.prefs.animation {
            l_core::state::AnimationIntensity::Off => l_widgets::core::types::IntensityHint::Off,
            l_core::state::AnimationIntensity::Subtle => {
                l_widgets::core::types::IntensityHint::Subtle
            }
            l_core::state::AnimationIntensity::Full => l_widgets::core::types::IntensityHint::Full,
        };
        let cell = self.cell_size;
        let edit_drag = &self.edit_drag;
        let q = self.pending.clone();

        let q_conf = q.clone();
        let mut on_configure = move |id: WidgetInstanceId| {
            q_conf.borrow_mut().push(Action::ConfigureWidget(id));
        };

        let q_drag = q.clone();
        let drag_map = edit_drag;
        let mut on_drag = move |id: WidgetInstanceId, delta: egui::Vec2| {
            let mut map = drag_map.borrow_mut();
            let entry = map.entry(id).or_insert(EditDrag {
                pos: w.pos,
                size: w.size,
                acc: egui::Vec2::ZERO,
            });
            entry.acc += delta;
            let dx_cells = (entry.acc.x / cell.0).round() as i32;
            let dy_cells = (entry.acc.y / cell.1).round() as i32;
            if dx_cells != 0 || dy_cells != 0 {
                let target = GridPos {
                    x: entry.pos.x.saturating_add_signed(dx_cells),
                    y: entry.pos.y.saturating_add_signed(dy_cells),
                };
                let widgets = active_widgets(state);
                let cols = surface_cols(state, surface);
                if layout::try_move(&widgets, id, target, entry.size, cols, surface).is_some() {
                    q_drag.borrow_mut().push(Action::DragTo {
                        id,
                        pos: target,
                        surface,
                    });
                    entry.pos = target;
                    entry.acc = egui::Vec2::ZERO;
                }
            }
        };

        let q_resize = q.clone();
        let min = manifest.map_or(GridSize::new(2, 2), |m| m.min_size);
        let max = manifest.and_then(|m| m.max_size);
        let mut on_resize = move |id: WidgetInstanceId, dw: f32, dh: f32| {
            let mut map = drag_map.borrow_mut();
            let entry = map.entry(id).or_insert(EditDrag {
                pos: w.pos,
                size: w.size,
                acc: egui::Vec2::ZERO,
            });
            entry.acc += egui::vec2(dw, dh);
            let dw_cells = (entry.acc.x / cell.0).round() as i32;
            let dh_cells = (entry.acc.y / cell.1).round() as i32;
            if dw_cells != 0 || dh_cells != 0 {
                let new_size = GridSize::new(
                    entry.size.w.saturating_add_signed(dw_cells).max(min.w),
                    entry.size.h.saturating_add_signed(dh_cells).max(min.h),
                );
                let widgets = active_widgets(state);
                let cols = surface_cols(state, surface);
                if layout::try_resize(&widgets, id, new_size, min, max, cols, surface).is_some() {
                    q_resize.borrow_mut().push(Action::ResizeWidget {
                        id,
                        size: new_size,
                        surface,
                    });
                    entry.size = new_size;
                    entry.acc = egui::Vec2::ZERO;
                }
            }
        };

        let q_dispatch = q.clone();
        let mut cx = RenderCx {
            state,
            dispatch: &mut move |a| q_dispatch.borrow_mut().push(a),
            animation,
            edit_mode,
            on_configure: &mut on_configure,
            on_drag: &mut on_drag,
            on_resize: &mut on_resize,
        };

        let managed = self.manager.get_mut(w.id);
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
            |ui| {
                draw_widget(ui, w, manifest, managed, &data, &mut cx);
            },
        );
    }
}

impl eframe::App for LApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Keep the clock ticking.
        ctx.request_repaint_after(Duration::from_secs(1));

        // 1. Service events.
        self.drain_events();
        self.drain_plugin_events();

        // 2. Post-hydration setup.
        if self.state.hydrated && !self.default_layout_done {
            self.default_layout_done = true;
            self.ensure_default_layout();
        }

        // 3. Widget sync + local publish.
        if self.widgets_dirty || self.state.hydrated {
            self.sync_widgets();
        }
        self.publish_local();

        // 4. Expire old toasts.
        let now = Instant::now();
        self.state
            .ui
            .toasts
            .retain(|t| now.duration_since(t.created) < Duration::from_secs(8));

        // 5. Keyboard shortcuts.
        self.handle_keys(&ctx);

        // 6. Draw UI.
        self.draw(ui);

        // 7. Drain queued actions.
        let pending: Vec<Action> = std::mem::take(&mut *self.pending.borrow_mut());
        for a in pending {
            self.dispatch_now(a);
        }
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {}
}

impl LApp {
    fn handle_keys(&mut self, ctx: &egui::Context) {
        // Recording a new keybind from the settings UI.
        if let Some(capture_id) = self.state.ui.keybind_capture.clone() {
            if let Some(combo) = ctx.input(crate::keybinds::combo_from_input) {
                // Plain Escape cancels recording without changing the binding.
                let is_escape = combo.key == egui::Key::Escape
                    && !combo.ctrl
                    && !combo.alt
                    && !combo.shift;
                if is_escape {
                    self.state.ui.keybind_capture = None;
                } else {
                    let mut p = self.state.prefs.clone();
                    let s = crate::keybinds::combo_to_string(combo);
                    if let Some(k) = p.keybinds.iter_mut().find(|k| k.action == capture_id) {
                        k.combo = s;
                    } else {
                        p.keybinds.push(l_core::state::Keybind {
                            action: capture_id.clone(),
                            combo: s,
                        });
                    }
                    self.state.ui.keybind_capture = None;
                    self.queue(Action::UpdatePrefs(Box::new(p)));
                }
            }
            return;
        }

        // Configurable shortcuts from prefs (defaults + overrides).
        for r in crate::keybinds::resolve(&self.state.prefs) {
            let Some(combo) = r.combo else { continue };
            if !ctx.input(|i| crate::keybinds::combo_matches(combo, i)) {
                continue;
            }
            if r.id == "sys.dismiss" {
                self.dismiss_overlays();
            } else if let Some(action) = r.action {
                self.queue(action);
            }
            break;
        }
        // Arrow-key nudge for focused widget in edit mode.
        if self.state.ui.layout_editing
            && let Some(id) = self.state.ui.focused_widget
        {
            let (dx, dy, dw, dh) = ctx.input(|i| {
                let mut d = (0i32, 0i32, 0i32, 0i32);
                if i.key_pressed(egui::Key::ArrowLeft) {
                    d.0 -= 1;
                }
                if i.key_pressed(egui::Key::ArrowRight) {
                    d.0 += 1;
                }
                if i.key_pressed(egui::Key::ArrowUp) {
                    d.1 -= 1;
                }
                if i.key_pressed(egui::Key::ArrowDown) {
                    d.1 += 1;
                }
                if i.modifiers.shift {
                    if i.key_pressed(egui::Key::ArrowLeft) {
                        d.2 -= 1;
                    }
                    if i.key_pressed(egui::Key::ArrowRight) {
                        d.2 += 1;
                    }
                    if i.key_pressed(egui::Key::ArrowUp) {
                        d.3 -= 1;
                    }
                    if i.key_pressed(egui::Key::ArrowDown) {
                        d.3 += 1;
                    }
                }
                d
            });
            if (dx, dy, dw, dh) != (0, 0, 0, 0) {
                // Find surface.
                let surface = self
                    .active_layout()
                    .and_then(|l| {
                        l.main
                            .iter()
                            .chain(l.dock.iter())
                            .find(|w| w.id == id)
                            .map(|w| w.surface)
                    })
                    .unwrap_or(Surface::Main);
                self.queue(Action::NudgeWidget {
                    id,
                    dx,
                    dy,
                    dw,
                    dh,
                    surface,
                });
            }
        }
    }

    fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        apply_theme_from_prefs(&ctx, &self.state.prefs);

        let panel_frame = |color: egui::Color32| {
            egui::Frame::NONE
                .fill(color)
                .inner_margin(egui::Margin::symmetric(12, 6))
        };

        egui::Panel::top("topbar")
            .exact_size(34.0)
            .frame(panel_frame(l_ui::theme::color::bg()))
            .show(ui, |ui| shell::draw_titlebar(self, ui));

        let rail_w = if self.state.prefs.show_server_rail {
            72.0
        } else {
            0.0
        };
        let nav_w = if self.state.prefs.show_nav_sidebar {
            240.0
        } else {
            0.0
        };
        let dock_w = if self.state.prefs.dock_visible {
            self.state.prefs.dock_width
        } else {
            0.0
        };

        egui::Panel::left("rail")
            .exact_size(rail_w)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::NONE.fill(l_ui::theme::color::rail()))
            .show(ui, |ui| {
                if rail_w > 0.0 {
                    shell::draw_server_rail(self, ui);
                }
            });

        egui::Panel::left("nav")
            .exact_size(nav_w)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::NONE.fill(l_ui::theme::color::bg_secondary()))
            .show(ui, |ui| {
                if nav_w > 0.0 {
                    shell::draw_nav_sidebar(self, ui);
                }
            });

        egui::Panel::right("dock")
            .exact_size(dock_w)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::NONE.fill(l_ui::theme::color::bg_secondary()))
            .show(ui, |ui| {
                if dock_w > 0.0 {
                    shell::draw_dock(self, ui);
                }
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(l_ui::theme::color::bg()))
            .show(ui, |ui| {
                ui.add_space(4.0);
                pages::draw_page(self, ui);
            });

        // Overlays.
        palette::draw_palette(self, &ctx);
        dialogs::draw_dialogs(self, &ctx);
        shell::draw_toasts(self, ui);
    }
}

fn apply_theme_from_prefs(ctx: &egui::Context, prefs: &l_core::state::Preferences) {
    use l_core::state::ThemeChoice;
    let preset = match prefs.theme {
        ThemeChoice::Mono => l_ui::theme::ThemePreset::Mono,
        ThemeChoice::DiscordDark => l_ui::theme::ThemePreset::DiscordDark,
        ThemeChoice::Light => l_ui::theme::ThemePreset::Light,
        ThemeChoice::Amoled => l_ui::theme::ThemePreset::Amoled,
    };
    let accent = prefs
        .accent_color
        .map(|c| egui::Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8));
    l_ui::theme::set_palette(l_ui::theme::palette_for(preset, accent));
    l_ui::theme::apply_theme(
        ctx,
        l_ui::theme::ThemeConfig {
            font_size: prefs.font_size,
            compact: prefs.compact,
            ui_scale: prefs.ui_scale,
        },
    );
}

/// All placed widgets on the active layout.
fn active_widgets(state: &AppState) -> Vec<PlacedWidget> {
    state
        .workspace
        .layouts
        .iter()
        .find(|l| {
            Some(l.id) == state.workspace.active_layout || state.workspace.active_layout.is_none()
        })
        .or_else(|| state.workspace.layouts.first())
        .map_or_default(|l| l.main.iter().chain(l.dock.iter()).cloned().collect())
}

/// Column count for a surface on the active layout.
fn surface_cols(state: &AppState, surface: Surface) -> u32 {
    state
        .workspace
        .layouts
        .iter()
        .find(|l| {
            Some(l.id) == state.workspace.active_layout || state.workspace.active_layout.is_none()
        })
        .or_else(|| state.workspace.layouts.first())
        .map_or(12, |l| match surface {
            Surface::Main => l.main_columns,
            Surface::Dock => l.dock_columns,
        })
}
