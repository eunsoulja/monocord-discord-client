//! Third-party widget sandbox: manifest format, permission model, and a
//! `SandboxHost` that currently refuses to load external code.
//!
//! Roadmap: external widgets will run under `wasmtime` with capability-based
//! host calls (network allow-list, storage quota, Discord read access only
//! when declared and approved). Imported configs are data only — they never
//! carry executable code.

use serde::{Deserialize, Serialize};

use crate::core::types::Permission;

/// On-disk manifest for a third-party widget package.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalWidgetManifest {
    /// Schema version (currently 1).
    pub schema_version: u32,
    /// Unique widget id (namespaced, e.g. `vendor.my_widget`).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Icon name.
    pub icon: String,
    /// Requested permissions.
    pub permissions: Vec<Permission>,
    /// Default size `[w, h]`.
    pub default_size: [u32; 2],
    /// Refresh interval seconds (0 = manual).
    pub refresh_secs: u64,
    /// SHA-256 of the expected WASM module (hex). Enforced at load time.
    pub wasm_sha256: String,
}

/// Errors from the sandbox.
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    /// External widget loading is not yet enabled.
    #[error("external widgets are not supported in this build")]
    Disabled,
    /// Manifest invalid.
    #[error("invalid manifest: {0}")]
    InvalidManifest(String),
    /// Hash mismatch.
    #[error("wasm hash mismatch")]
    HashMismatch,
    /// Permission denied by user.
    #[error("permission denied: {0}")]
    PermissionDenied(String),
}

/// Host API surface a sandboxed widget would see. Every method is
/// capability-checked against the manifest's declared permissions.
pub trait SandboxHost {
    /// Whether a permission was granted for this widget instance.
    fn has_permission(&self, perm: Permission) -> bool;
    /// Read a namespaced storage key (quota enforced by impl).
    fn storage_get(&self, key: &str) -> Result<Option<String>, SandboxError>;
    /// Write a namespaced storage key.
    fn storage_set(&self, key: &str, value: &str) -> Result<(), SandboxError>;
    /// Perform an allow-listed network request.
    fn http_get(&self, url: &str) -> Result<String, SandboxError>;
    /// Emit a notification (requires `OsNotify`).
    fn notify(&self, title: &str, body: &str) -> Result<(), SandboxError>;
}

/// The current sandbox host: refuses to load or run external widget code.
pub struct SandboxHostImpl;

impl SandboxHost for SandboxHostImpl {
    fn has_permission(&self, _perm: Permission) -> bool {
        false
    }
    fn storage_get(&self, _key: &str) -> Result<Option<String>, SandboxError> {
        Err(SandboxError::Disabled)
    }
    fn storage_set(&self, _key: &str, _value: &str) -> Result<(), SandboxError> {
        Err(SandboxError::Disabled)
    }
    fn http_get(&self, _url: &str) -> Result<String, SandboxError> {
        Err(SandboxError::Disabled)
    }
    fn notify(&self, _title: &str, _body: &str) -> Result<(), SandboxError> {
        Err(SandboxError::Disabled)
    }
}

/// Attempt to load an external widget package. Always fails today.
pub fn load_external(_manifest: &ExternalWidgetManifest, _wasm: &[u8]) -> Result<(), SandboxError> {
    Err(SandboxError::Disabled)
}
