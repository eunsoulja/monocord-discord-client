//! Plugin manifest parsing (Legcord-compatible `manifest.json` layout).
//!
//! Plugins live in `<data_dir>/plugins/<plugin_id>/` and contain a
//! `manifest.json` plus an executable entry point. The host spawns the entry
//! point and speaks newline-delimited JSON-RPC over stdio.

use std::path::PathBuf;

use serde::Deserialize;

/// Parsed `manifest.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginManifestFile {
    /// Stable plugin id (also the folder name).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Semver-ish version string.
    #[serde(default)]
    pub version: String,
    /// Short description.
    #[serde(default)]
    pub description: String,
    /// Author.
    #[serde(default)]
    pub author: String,
    /// SPDX-ish license string.
    #[serde(default)]
    pub license: String,
    /// Homepage / repo URL.
    #[serde(default)]
    pub homepage: String,
    /// Executable to spawn, relative to the plugin dir (or an absolute path).
    pub entry: String,
    /// Arguments passed to the entry point.
    #[serde(default)]
    pub args: Vec<String>,
    /// Widget views this plugin provides.
    #[serde(default)]
    pub widgets: Vec<PluginWidgetDecl>,
    /// Palette commands this plugin provides.
    #[serde(default)]
    pub commands: Vec<PluginCommandDecl>,
}

/// A widget view declared by a plugin.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginWidgetDecl {
    /// View id (unique within the plugin).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Description.
    #[serde(default)]
    pub description: String,
    /// Icon name (mapped by the app).
    #[serde(default = "default_icon")]
    pub icon: String,
    /// Default grid size `[w, h]` in cells.
    #[serde(default = "default_size")]
    pub size: [u32; 2],
    /// Default poll interval seconds (0 = host default).
    #[serde(default)]
    pub refresh_secs: u64,
    /// Settings schema for the widget config dialog.
    #[serde(default)]
    pub settings: Vec<PluginSetting>,
}

fn default_icon() -> String {
    "terminal".to_string()
}

fn default_size() -> [u32; 2] {
    [4, 3]
}

/// A palette command declared by a plugin.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginCommandDecl {
    /// Command id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Description shown in the palette.
    #[serde(default)]
    pub description: String,
}

/// A widget setting field (mirrors the widget schema kinds).
#[derive(Debug, Clone, Deserialize)]
pub struct PluginSetting {
    /// Config key.
    pub key: String,
    /// Label.
    pub label: String,
    /// Field kind.
    #[serde(default)]
    pub kind: PluginSettingKind,
    /// Default value as JSON.
    #[serde(default)]
    pub default: serde_json::Value,
}

/// Setting field kinds (subset of the widget setting kinds).
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PluginSettingKind {
    /// Free text.
    #[default]
    Text,
    /// Integer with min/max.
    Integer {
        /// Min.
        #[serde(default)]
        min: i64,
        /// Max.
        #[serde(default = "default_max")]
        max: i64,
    },
    /// Boolean toggle.
    Boolean,
    /// Choice from a list.
    Choice {
        /// Options.
        #[serde(default)]
        options: Vec<String>,
    },
    /// Secret/write-only field.
    Secret,
    /// URL.
    Url,
}

fn default_max() -> i64 {
    i64::MAX
}

/// A discovered plugin on disk.
#[derive(Debug, Clone)]
pub struct DiscoveredPlugin {
    /// Parsed manifest.
    pub manifest: PluginManifestFile,
    /// Plugin directory.
    pub dir: PathBuf,
    /// Whether the plugin is enabled.
    pub enabled: bool,
}

impl DiscoveredPlugin {
    /// Absolute path to the entry executable.
    #[must_use]
    pub fn entry_path(&self) -> PathBuf {
        let p = PathBuf::from(&self.manifest.entry);
        if p.is_absolute() {
            p
        } else {
            self.dir.join(p)
        }
    }
}

/// Errors from manifest loading.
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    /// IO error.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// JSON parse error.
    #[error("manifest json: {0}")]
    Json(#[from] serde_json::Error),
    /// Missing required field.
    #[error("manifest missing field: {0}")]
    Missing(&'static str),
}

/// Load and validate a manifest file.
pub fn load_manifest(path: &std::path::Path) -> Result<PluginManifestFile, ManifestError> {
    let text = std::fs::read_to_string(path)?;
    let m: PluginManifestFile = serde_json::from_str(&text)?;
    if m.id.is_empty() {
        return Err(ManifestError::Missing("id"));
    }
    if m.name.is_empty() {
        return Err(ManifestError::Missing("name"));
    }
    if m.entry.is_empty() {
        return Err(ManifestError::Missing("entry"));
    }
    Ok(m)
}

/// Scan `plugins_dir` for `<id>/manifest.json` files.
pub fn scan_dir(plugins_dir: &std::path::Path) -> Vec<DiscoveredPlugin> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(plugins_dir) else {
        return out;
    };
    for e in entries.flatten() {
        let dir = e.path();
        if !dir.is_dir() {
            continue;
        }
        let manifest_path = dir.join("manifest.json");
        if !manifest_path.exists() {
            continue;
        }
        match load_manifest(&manifest_path) {
            Ok(manifest) => out.push(DiscoveredPlugin {
                manifest,
                dir,
                enabled: true,
            }),
            Err(err) => {
                tracing::warn!("plugin manifest {}: {err}", manifest_path.display());
            }
        }
    }
    out.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
    out
}
