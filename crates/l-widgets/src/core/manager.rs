//! `WidgetManager`: owns live instances for placed widgets.

use std::collections::HashMap;

use l_core::models::{PlacedWidget, WidgetInstanceId};

use crate::core::registry::WidgetRegistry;
use crate::core::types::{WidgetInstance, WidgetState};

/// Per-instance runtime bookkeeping.
pub struct ManagedInstance {
    /// The live instance.
    pub instance: Box<dyn WidgetInstance>,
    /// Last known widget type (for re-creation).
    pub widget_type: String,
    /// Current runtime state.
    pub state: WidgetState,
    /// Last error message.
    pub error: Option<String>,
    /// Consecutive failure count (for backoff).
    pub failures: u32,
}

/// Owns the map from placed-widget id to live instance.
pub struct WidgetManager {
    instances: HashMap<WidgetInstanceId, ManagedInstance>,
}

impl WidgetManager {
    /// Empty manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            instances: HashMap::new(),
        }
    }

    /// Sync live instances with the placed set: create missing, drop removed,
    /// recreate on type change, refresh on config change.
    pub fn sync(&mut self, placed: &[PlacedWidget], registry: &WidgetRegistry) {
        let wanted: std::collections::HashSet<WidgetInstanceId> =
            placed.iter().map(|w| w.id).collect();

        // Drop removed.
        self.instances.retain(|id, _| wanted.contains(id));

        for w in placed {
            match self.instances.get_mut(&w.id) {
                Some(m) if m.widget_type == w.widget_type => {
                    // Config may have changed; instances compare cheaply via
                    // on_config_changed (they're expected to be idempotent).
                    m.instance.on_config_changed(&w.config);
                }
                Some(_) => {
                    // Type changed: recreate.
                    self.instances.remove(&w.id);
                    self.create_one(w, registry);
                }
                None => {
                    self.create_one(w, registry);
                }
            }
        }
    }

    fn create_one(&mut self, w: &PlacedWidget, registry: &WidgetRegistry) {
        match registry.create(w) {
            Ok(instance) => {
                self.instances.insert(
                    w.id,
                    ManagedInstance {
                        instance,
                        widget_type: w.widget_type.clone(),
                        state: WidgetState::Loading,
                        error: None,
                        failures: 0,
                    },
                );
            }
            Err(e) => {
                // Store a tombstone so the renderer can show the error.
                self.instances.insert(
                    w.id,
                    ManagedInstance {
                        instance: Box::new(NullInstance),
                        widget_type: w.widget_type.clone(),
                        state: WidgetState::Error,
                        error: Some(e.to_string()),
                        failures: u32::MAX,
                    },
                );
            }
        }
    }

    /// Get a managed instance.
    #[must_use]
    pub fn get(&self, id: WidgetInstanceId) -> Option<&ManagedInstance> {
        self.instances.get(&id)
    }

    /// Get a managed instance mutably.
    pub fn get_mut(&mut self, id: WidgetInstanceId) -> Option<&mut ManagedInstance> {
        self.instances.get_mut(&id)
    }

    /// Iterate all instances.
    pub fn iter(&self) -> impl Iterator<Item = (&WidgetInstanceId, &ManagedInstance)> {
        self.instances.iter()
    }

    /// Iterate mutably.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&WidgetInstanceId, &mut ManagedInstance)> {
        self.instances.iter_mut()
    }

    /// All instance ids.
    #[must_use]
    pub fn ids(&self) -> Vec<WidgetInstanceId> {
        self.instances.keys().copied().collect()
    }

    /// Remove one instance (cancels its background work implicitly).
    pub fn remove(&mut self, id: WidgetInstanceId) {
        self.instances.remove(&id);
    }

    /// Clear all.
    pub fn clear(&mut self) {
        self.instances.clear();
    }
}

impl Default for WidgetManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Placeholder used when factory creation fails.
struct NullInstance;

impl WidgetInstance for NullInstance {
    fn data_needs(&self) -> Vec<crate::core::types::DataNeed> {
        Vec::new()
    }
    fn ui(
        &mut self,
        _ui: &mut egui::Ui,
        _cx: &mut crate::core::types::WidgetCx<'_>,
        _data: &crate::core::types::WidgetData,
    ) {
    }
    fn on_config_changed(&mut self, _config: &serde_json::Value) {}
}
