//! Widget type registry.

use std::collections::HashMap;
use std::sync::Arc;

use l_core::models::PlacedWidget;

use crate::core::types::{WidgetError, WidgetFactory, WidgetInstance, WidgetManifest};

/// Registry of widget factories keyed by manifest id.
pub struct WidgetRegistry {
    factories: HashMap<&'static str, Arc<dyn WidgetFactory>>,
}

impl WidgetRegistry {
    /// Empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    /// Register a factory.
    pub fn register(&mut self, factory: Arc<dyn WidgetFactory>) {
        let id = factory.manifest().id;
        self.factories.insert(id, factory);
    }

    /// Look up a factory.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Arc<dyn WidgetFactory>> {
        self.factories.get(id)
    }

    /// All manifests.
    #[must_use]
    pub fn manifests(&self) -> Vec<&'static WidgetManifest> {
        self.factories.values().map(|f| f.manifest()).collect()
    }

    /// Create an instance for a placed widget.
    pub fn create(&self, placed: &PlacedWidget) -> Result<Box<dyn WidgetInstance>, WidgetError> {
        let factory = self
            .get(&placed.widget_type)
            .ok_or_else(|| WidgetError::UnknownType(placed.widget_type.clone()))?;
        factory.create(placed)
    }
}

impl Default for WidgetRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Register every built-in widget. Adding a widget means adding a module and
/// one line here — never touching the renderer or layout engine.
pub fn register_all(registry: &mut WidgetRegistry) {
    registry.register(Arc::new(crate::built_in::server_stats::ServerStatsFactory));
    registry.register(Arc::new(
        crate::built_in::member_activity::MemberActivityFactory,
    ));
    registry.register(Arc::new(
        crate::built_in::server_uptime::ServerUptimeFactory,
    ));
    registry.register(Arc::new(
        crate::built_in::quick_actions::QuickActionsFactory,
    ));
    registry.register(Arc::new(
        crate::built_in::notifications::NotificationsFactory,
    ));
    registry.register(Arc::new(
        crate::built_in::personal_notes::PersonalNotesFactory,
    ));
    registry.register(Arc::new(
        crate::built_in::quick_commands::QuickCommandsFactory,
    ));
    registry.register(Arc::new(crate::built_in::bot_monitor::BotMonitorFactory));
    registry.register(Arc::new(
        crate::built_in::voice_activity::VoiceActivityFactory,
    ));
    registry.register(Arc::new(crate::built_in::api_monitor::ApiMonitorFactory));
    registry.register(Arc::new(crate::built_in::system_stats::SystemStatsFactory));
}
