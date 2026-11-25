//! Notification Center widget.

use egui::{RichText, Ui};

use l_core::models::{GridSize, Notification, PlacedWidget};
use l_ui::theme::color;

use crate::built_in::common::{StatusTone, rel_time, status_text};
use crate::core::types::{
    DataNeed, Permission, RefreshPolicy, WidgetCategory, WidgetCx, WidgetData, WidgetError,
    WidgetFactory, WidgetInstance, WidgetManifest,
};

static MANIFEST: std::sync::LazyLock<WidgetManifest> =
    std::sync::LazyLock::new(|| WidgetManifest {
        id: "l.notifications",
        name: "Notification Center",
        description: "Recent notifications with unread count.",
        icon: "bell",
        version: "1.0.0",
        category: WidgetCategory::Productivity,
        permissions: &[Permission::LocalNotifications],
        default_size: GridSize::new(4, 3),
        min_size: GridSize::new(3, 2),
        max_size: Some(GridSize::new(8, 6)),
        refresh: RefreshPolicy::Push,
        settings: vec![],
    });

/// Payload.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct NotificationsPayload {
    /// Notifications newest-first.
    pub notifications: Vec<Notification>,
    /// Unread count.
    pub unread: u64,
}

pub struct NotificationsFactory;

impl WidgetFactory for NotificationsFactory {
    fn manifest(&self) -> &'static WidgetManifest {
        &MANIFEST
    }
    fn create(&self, placed: &PlacedWidget) -> Result<Box<dyn WidgetInstance>, WidgetError> {
        Ok(Box::new(NotifWidget {
            placed: placed.clone(),
        }))
    }
}

struct NotifWidget {
    placed: PlacedWidget,
}

impl WidgetInstance for NotifWidget {
    fn data_needs(&self) -> Vec<DataNeed> {
        vec![DataNeed::Notifications]
    }

    fn on_config_changed(&mut self, config: &serde_json::Value) {
        self.placed.config = config.clone();
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut WidgetCx<'_>, data: &WidgetData) {
        let Some(payload) = data.decode::<NotificationsPayload>() else {
            ui.label(RichText::new("waiting…").small().color(color::text_muted()));
            return;
        };
        if payload.unread > 0 {
            status_text(ui, &format!("{} unread", payload.unread), StatusTone::Warn);
        }
        if payload.notifications.is_empty() {
            ui.label(
                RichText::new("no notifications")
                    .small()
                    .color(color::text_muted()),
            );
            return;
        }
        for n in payload.notifications.iter().take(8) {
            ui.horizontal(|ui| {
                if n.read {
                    ui.label(RichText::new("○").small().color(color::text_muted()));
                } else {
                    ui.label(RichText::new("●").small().color(color::accent()));
                }
                ui.vertical(|ui| {
                    ui.label(RichText::new(&n.title).small().color(color::text()));
                    ui.label(
                        RichText::new(format!("{} · {}", n.source, rel_time(n.created_at)))
                            .size(10.0)
                            .color(color::text_muted()),
                    );
                });
            });
        }
        if ui.small_button("open notifications").clicked() {
            (cx.dispatch)(l_core::Action::Navigate(
                l_core::models::Page::Notifications,
            ));
        }
    }
}
