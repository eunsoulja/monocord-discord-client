//! Plugin integration glue: turns plugin-manifest widget views into
//! registered widget factories (Legcord-style: plugins show up in the
//! library and command palette like built-ins).

use std::sync::Arc;

use l_core::models::GridSize;
use l_plugins::manifest::{PluginSetting, PluginSettingKind, PluginWidgetDecl};
use l_plugins::PluginHost;
use l_widgets::built_in::plugin_view::PluginViewFactory;
use l_widgets::core::registry::WidgetRegistry;
use l_widgets::core::types::{
    RefreshPolicy, SettingField, SettingKind, WidgetCategory, WidgetManifest,
};

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn sanitize_id(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn map_setting(s: &PluginSetting) -> SettingField {
    let kind = match &s.kind {
        PluginSettingKind::Text => SettingKind::Text,
        PluginSettingKind::Integer { min, max } => SettingKind::Integer {
            min: *min,
            max: *max,
        },
        PluginSettingKind::Boolean => SettingKind::Boolean,
        PluginSettingKind::Choice { options } => SettingKind::Choice {
            options: options.clone(),
        },
        PluginSettingKind::Secret => SettingKind::Secret,
        PluginSettingKind::Url => SettingKind::Url,
    };
    SettingField {
        key: s.key.clone(),
        label: s.label.clone(),
        kind,
        default: s.default.clone(),
    }
}

fn register_one(
    registry: &mut WidgetRegistry,
    plugin_name: &str,
    plugin_version: &str,
    plugin_id: &str,
    decl: &PluginWidgetDecl,
) {
    let widget_type = format!("plugin.{}.{}", sanitize_id(plugin_id), sanitize_id(&decl.id));
    let description = if decl.description.is_empty() {
        format!("{plugin_name} plugin view")
    } else {
        format!("{plugin_name} — {}", decl.description)
    };
    let (w, h) = (decl.size[0].clamp(2, 12), decl.size[1].clamp(1, 8));
    let refresh = decl.refresh_secs.max(5);
    // Manifests must be 'static for the registry; plugin manifests are few
    // and app-lifetime, so leaking is acceptable (same lifetime as the
    // process itself).
    let manifest: &'static WidgetManifest = Box::leak(Box::new(WidgetManifest {
        id: leak(widget_type),
        name: leak(decl.name.clone()),
        description: leak(description),
        icon: leak(decl.icon.clone()),
        version: leak(if plugin_version.is_empty() {
            "0.0.0".to_string()
        } else {
            plugin_version.to_string()
        }),
        category: WidgetCategory::Custom,
        permissions: &[],
        default_size: GridSize::new(w, h),
        min_size: GridSize::new(2, 2),
        max_size: None,
        refresh: RefreshPolicy::Interval {
            default_secs: refresh,
            min_secs: 5,
        },
        settings: decl.settings.iter().map(map_setting).collect(),
    }));
    registry.register(Arc::new(PluginViewFactory::new(
        manifest,
        plugin_id.to_string(),
        decl.id.clone(),
        refresh,
    )));
}

/// (Re)register widget factories for every enabled plugin view. Safe to call
/// after `PluginHost::rescan()` — re-registering an existing id replaces it.
pub fn register_plugin_widgets(registry: &mut WidgetRegistry, host: &Arc<PluginHost>) {
    let plugins = host.plugins();
    for (plugin_id, decl) in host.widget_views() {
        let p = plugins.iter().find(|p| p.manifest.id == plugin_id);
        register_one(
            registry,
            p.map_or(plugin_id.as_str(), |p| p.manifest.name.as_str()),
            p.map_or("", |p| p.manifest.version.as_str()),
            &plugin_id,
            &decl,
        );
    }
}
