//! Error types for library crates.

use thiserror::Error;

/// Errors produced inside library crates.
#[derive(Debug, Error)]
pub enum AppError {
    /// Serialization or deserialization failed.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    /// Storage failure.
    #[error("storage error: {0}")]
    Storage(String),
    /// Discord API failure.
    #[error("discord error: {0}")]
    Discord(String),
    /// Widget failure.
    #[error("widget error: {0}")]
    Widget(String),
    /// Permission was denied for an operation.
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    /// Invalid user-supplied input.
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// Network failure.
    #[error("network error: {0}")]
    Network(String),
}
