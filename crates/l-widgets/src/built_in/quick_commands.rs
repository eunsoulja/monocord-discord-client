//! Quick Commands widget: user-defined command shortcuts.

use egui::{RichText, Ui};

use l_core::models::{GridSize, PlacedWidget};
use l_ui::theme::color;

use crate::core::types::{
    DataNeed, Permission, RefreshPolicy, SettingField, SettingKind, WidgetCategory, WidgetCx,
    WidgetData, WidgetError, WidgetFactory, WidgetInstance, WidgetManifest, cfg_string,
};

static MANIFEST: std::sync::LazyLock<WidgetManifest> =
    std::sync::LazyLock::new(|| WidgetManifest {
        id: "l.quick_commands",
        name: "Quick Commands",
        description: "Saved commands and shortcuts, also usable from the palette.",
        icon: "command",
        version: "1.0.0",
        category: WidgetCategory::Developer,
        permissions: &[Permission::LocalStorage],
        default_size: GridSize::new(3, 2),
        min_size: GridSize::new(2, 2),
        max_size: Some(GridSize::new(6, 4)),
        refresh: RefreshPolicy::Manual,
        settings: vec![SettingField {
            key: "commands".into(),
            label: "Commands (label=command per line)".into(),
            kind: SettingKind::Text,
            default: serde_json::json!("palette=command:sys.palette"),
        }],
    });

pub struct QuickCommandsFactory;

impl WidgetFactory for QuickCommandsFactory {
    fn manifest(&self) -> &'static WidgetManifest {
        &MANIFEST
    }
    fn create(&self, placed: &PlacedWidget) -> Result<Box<dyn WidgetInstance>, WidgetError> {
        Ok(Box::new(QcWidget {
            placed: placed.clone(),
        }))
    }
}

struct QcWidget {
    placed: PlacedWidget,
}

impl WidgetInstance for QcWidget {
    fn data_needs(&self) -> Vec<DataNeed> {
        Vec::new()
    }

    fn on_config_changed(&mut self, config: &serde_json::Value) {
        self.placed.config = config.clone();
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut WidgetCx<'_>, _data: &WidgetData) {
        let raw = cfg_string(&self.placed, &MANIFEST, "commands");
        let mut any = false;
        for line in raw.lines().take(12) {
            let line = line.trim();
            let Some((label, cmd)) = line.split_once('=') else {
                continue;
            };
            any = true;
            if ui
                .add(egui::Button::new(RichText::new(label.trim()).small()))
                .clicked()
            {
                let cmd = cmd.trim().to_string();
                (cx.dispatch)(l_core::Action::RunCommand(cmd));
            }
        }
        if !any {
            ui.label(
                RichText::new("no commands configured")
                    .small()
                    .color(color::text_muted()),
            );
        }
    }
}
