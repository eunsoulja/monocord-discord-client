//! Design system for L: monochrome terminal-inspired theme and components.
//!
//! This crate knows nothing about Discord, storage, or application state.

pub mod components;
pub mod fonts;
pub mod icons;
pub mod motion;
pub mod theme;

pub use theme::{ThemeConfig, apply_theme};
