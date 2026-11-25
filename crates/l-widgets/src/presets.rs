//! Built-in layout presets.

use l_core::models::{
    GridPos, GridSize, Layout, LayoutId, PlacedWidget, Surface, WidgetInstanceId,
};

use crate::core::registry::WidgetRegistry;

/// Preset names.
pub const PRESETS: [&str; 5] = [
    "minimal",
    "developer",
    "server_management",
    "monitoring",
    "custom",
];

fn placed(
    widget_type: &str,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    surface: Surface,
    _registry: &WidgetRegistry,
) -> PlacedWidget {
    PlacedWidget {
        id: WidgetInstanceId::new_v4(),
        widget_type: widget_type.to_string(),
        name: String::new(),
        enabled: true,
        pos: GridPos { x, y },
        size: GridSize::new(w, h),
        config: serde_json::json!({}),
        surface,
    }
}

/// Build the widget set for a preset. `custom` returns empty (user's own).
#[must_use]
pub fn preset_widgets(preset: &str, registry: &WidgetRegistry) -> Vec<PlacedWidget> {
    let mut v = Vec::new();
    let add = |v: &mut Vec<PlacedWidget>, t: &str, x, y, w, h, s: Surface| {
        v.push(placed(t, x, y, w, h, s, registry));
    };
    match preset {
        "minimal" => {
            add(&mut v, "l.server_stats", 0, 0, 4, 3, Surface::Main);
            add(&mut v, "l.notifications", 4, 0, 4, 3, Surface::Main);
            add(&mut v, "l.personal_notes", 0, 3, 4, 3, Surface::Main);
        }
        "developer" => {
            add(&mut v, "l.api_monitor", 0, 0, 4, 3, Surface::Main);
            add(&mut v, "l.system_stats", 4, 0, 3, 2, Surface::Main);
            add(&mut v, "l.quick_commands", 7, 0, 3, 2, Surface::Main);
            add(&mut v, "l.personal_notes", 0, 3, 6, 3, Surface::Main);
            add(&mut v, "l.notifications", 6, 3, 4, 3, Surface::Dock);
        }
        "server_management" => {
            add(&mut v, "l.server_stats", 0, 0, 4, 3, Surface::Main);
            add(&mut v, "l.member_activity", 4, 0, 4, 3, Surface::Main);
            add(&mut v, "l.bot_monitor", 8, 0, 4, 2, Surface::Main);
            add(&mut v, "l.voice_activity", 8, 2, 4, 2, Surface::Main);
            add(&mut v, "l.quick_actions", 0, 3, 4, 2, Surface::Main);
            add(&mut v, "l.notifications", 4, 3, 4, 3, Surface::Dock);
        }
        "monitoring" => {
            add(&mut v, "l.server_uptime", 0, 0, 4, 3, Surface::Main);
            add(&mut v, "l.api_monitor", 4, 0, 4, 3, Surface::Main);
            add(&mut v, "l.system_stats", 8, 0, 4, 2, Surface::Main);
            add(&mut v, "l.bot_monitor", 8, 2, 4, 2, Surface::Main);
            add(&mut v, "l.notifications", 0, 3, 4, 3, Surface::Dock);
        }
        _ => {}
    }
    v
}

/// Build a full `Layout` for a preset.
#[must_use]
pub fn preset_layout(name: &str, registry: &WidgetRegistry) -> Layout {
    let widgets = preset_widgets(name, registry);
    let (main, dock): (Vec<_>, Vec<_>) = widgets
        .into_iter()
        .partition(|w| w.surface == Surface::Main);
    Layout {
        id: LayoutId::new_v4(),
        name: name.to_string(),
        is_default: false,
        preset: Some(name.to_string()),
        main,
        dock,
        main_columns: 12,
        dock_columns: 4,
    }
}
