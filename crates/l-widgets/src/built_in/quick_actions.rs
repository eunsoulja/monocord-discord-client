//! Quick Actions widget: configurable shortcuts that dispatch app actions.

use egui::{RichText, Ui};

use l_core::models::{GridSize, Page, PlacedWidget};
use l_ui::theme::color;

use crate::core::types::{
    DataNeed, Permission, RefreshPolicy, SettingField, SettingKind, WidgetCategory, WidgetCx,
    WidgetData, WidgetError, WidgetFactory, WidgetInstance, WidgetManifest, cfg_string,
};

static MANIFEST: std::sync::LazyLock<WidgetManifest> =
    std::sync::LazyLock::new(|| WidgetManifest {
        id: "l.quick_actions",
        name: "Quick Actions",
        description: "One-click shortcuts: navigation, Discord links, refresh.",
        icon: "zap",
        version: "1.0.0",
        category: WidgetCategory::Productivity,
        permissions: &[Permission::LocalStorage],
        default_size: GridSize::new(3, 2),
        min_size: GridSize::new(2, 2),
        max_size: Some(GridSize::new(6, 4)),
        refresh: RefreshPolicy::Manual,
        settings: vec![SettingField {
            key: "actions".into(),
            label: "Actions (label=target per line; target: page:name or url:https://…)".into(),
            kind: SettingKind::Text,
            default: serde_json::json!(
                "workspace=page:workspace\nrefresh=command:sys.refresh_widgets"
            ),
        }],
    });

pub struct QuickActionsFactory;

impl WidgetFactory for QuickActionsFactory {
    fn manifest(&self) -> &'static WidgetManifest {
        &MANIFEST
    }
    fn create(&self, placed: &PlacedWidget) -> Result<Box<dyn WidgetInstance>, WidgetError> {
        Ok(Box::new(QuickActions {
            placed: placed.clone(),
        }))
    }
}

struct QuickActions {
    placed: PlacedWidget,
}

/// A parsed quick action.
#[derive(Debug, Clone)]
struct Qa {
    label: String,
    target: String,
}

impl QuickActions {
    fn actions(&self) -> Vec<Qa> {
        let raw = cfg_string(&self.placed, &MANIFEST, "actions");
        raw.lines()
            .filter_map(|l| {
                let l = l.trim();
                if l.is_empty() {
                    return None;
                }
                let (label, target) = l.split_once('=')?;
                Some(Qa {
                    label: label.trim().to_string(),
                    target: target.trim().to_string(),
                })
            })
            .take(12)
            .collect()
    }
}

fn page_from_name(name: &str) -> Option<Page> {
    Some(match name {
        "workspace" => Page::ServerWorkspace,
        "library" => Page::WidgetLibrary,
        "activity" => Page::Activity,
        "notifications" => Page::Notifications,
        "bot" => Page::BotManagement,
        "settings" => Page::Settings,
        _ => return None,
    })
}

impl WidgetInstance for QuickActions {
    fn data_needs(&self) -> Vec<DataNeed> {
        Vec::new()
    }

    fn on_config_changed(&mut self, config: &serde_json::Value) {
        self.placed.config = config.clone();
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut WidgetCx<'_>, _data: &WidgetData) {
        let actions = self.actions();
        if actions.is_empty() {
            ui.label(
                RichText::new("no actions configured")
                    .small()
                    .color(color::text_muted()),
            );
            return;
        }
        for qa in actions {
            if ui
                .add(egui::Button::new(RichText::new(&qa.label).small()))
                .clicked()
            {
                let target = qa.target.clone();
                if let Some(page) = target.strip_prefix("page:").and_then(page_from_name) {
                    (cx.dispatch)(l_core::Action::Navigate(page));
                } else if let Some(url) = target.strip_prefix("url:") {
                    (cx.dispatch)(l_core::Action::OpenUrl(url.to_string()));
                } else if let Some(cmd) = target.strip_prefix("command:") {
                    (cx.dispatch)(l_core::Action::RunCommand(cmd.to_string()));
                } else if target == "refresh" {
                    (cx.dispatch)(l_core::Action::Event(l_core::events::AppEvent::Tick));
                }
            }
        }
    }
}
