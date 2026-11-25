//! External plugin support for L.
//!
//! Plugins are directories under `<data>/plugins/<id>/` containing a
//! `manifest.json` and an executable entry point. The host spawns each plugin
//! lazily and speaks newline-delimited JSON-RPC over stdio. Plugins can
//! provide widget views (rendered by the host's generic plugin widget) and
//! palette commands. Enable/disable state persists in
//! `<data>/plugin-state.json` — the Legcord `pluginStates` equivalent.

pub mod host;
pub mod manifest;

pub use host::{PluginEvent, PluginHost};
pub use manifest::{
    DiscoveredPlugin, PluginCommandDecl, PluginManifestFile, PluginSetting, PluginSettingKind,
    PluginWidgetDecl, load_manifest, scan_dir,
};
