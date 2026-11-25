//! Configurable keyboard shortcuts (Legcord-style keybinds).
//!
//! Combos are stored as strings like `Ctrl+K` or `Alt+F5` in
//! `Preferences::keybinds`; this module parses, formats, and matches them
//! against egui input.

use egui::{InputState, Key, Modifiers};
use l_core::actions::Action;
use l_core::commands::keybind_catalog;
use l_core::state::Preferences;

/// A parsed key combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Combo {
    /// Ctrl (or Cmd on macOS) held.
    pub ctrl: bool,
    /// Alt held.
    pub alt: bool,
    /// Shift held.
    pub shift: bool,
    /// The non-modifier key.
    pub key: Key,
}

/// Whether the combo's primary modifier is down (Ctrl or Cmd).
fn ctrl_down(m: Modifiers) -> bool {
    m.ctrl || m.mac_cmd || m.command
}

/// Format a combo as `Ctrl+Alt+Shift+K`.
#[must_use]
pub fn combo_to_string(c: Combo) -> String {
    let mut parts = Vec::new();
    if c.ctrl {
        parts.push("Ctrl");
    }
    if c.alt {
        parts.push("Alt");
    }
    if c.shift {
        parts.push("Shift");
    }
    if let Some(name) = key_name(c.key) {
        parts.push(name);
    }
    parts.join("+")
}

/// Name of a bindable key, or `None` for keys we don't support.
#[must_use]
pub fn key_name(key: Key) -> Option<&'static str> {
    Some(match key {
        Key::A => "A",
        Key::B => "B",
        Key::C => "C",
        Key::D => "D",
        Key::E => "E",
        Key::F => "F",
        Key::G => "G",
        Key::H => "H",
        Key::I => "I",
        Key::J => "J",
        Key::K => "K",
        Key::L => "L",
        Key::M => "M",
        Key::N => "N",
        Key::O => "O",
        Key::P => "P",
        Key::Q => "Q",
        Key::R => "R",
        Key::S => "S",
        Key::T => "T",
        Key::U => "U",
        Key::V => "V",
        Key::W => "W",
        Key::X => "X",
        Key::Y => "Y",
        Key::Z => "Z",
        Key::Num0 => "0",
        Key::Num1 => "1",
        Key::Num2 => "2",
        Key::Num3 => "3",
        Key::Num4 => "4",
        Key::Num5 => "5",
        Key::Num6 => "6",
        Key::Num7 => "7",
        Key::Num8 => "8",
        Key::Num9 => "9",
        Key::F1 => "F1",
        Key::F2 => "F2",
        Key::F3 => "F3",
        Key::F4 => "F4",
        Key::F5 => "F5",
        Key::F6 => "F6",
        Key::F7 => "F7",
        Key::F8 => "F8",
        Key::F9 => "F9",
        Key::F10 => "F10",
        Key::F11 => "F11",
        Key::F12 => "F12",
        Key::F13 => "F13",
        Key::F14 => "F14",
        Key::F15 => "F15",
        Key::F16 => "F16",
        Key::F17 => "F17",
        Key::F18 => "F18",
        Key::F19 => "F19",
        Key::F20 => "F20",
        Key::F21 => "F21",
        Key::F22 => "F22",
        Key::F23 => "F23",
        Key::F24 => "F24",
        Key::F25 => "F25",
        Key::F26 => "F26",
        Key::F27 => "F27",
        Key::F28 => "F28",
        Key::F29 => "F29",
        Key::F30 => "F30",
        Key::F31 => "F31",
        Key::F32 => "F32",
        Key::F33 => "F33",
        Key::F34 => "F34",
        Key::F35 => "F35",
        Key::ArrowDown => "Down",
        Key::ArrowLeft => "Left",
        Key::ArrowRight => "Right",
        Key::ArrowUp => "Up",
        Key::Escape => "Escape",
        Key::Tab => "Tab",
        Key::Backspace => "Backspace",
        Key::Enter => "Enter",
        Key::Space => "Space",
        Key::Insert => "Insert",
        Key::Delete => "Delete",
        Key::Home => "Home",
        Key::End => "End",
        Key::PageUp => "PageUp",
        Key::PageDown => "PageDown",
        _ => return None,
    })
}

/// Parse a key name back into an `egui::Key`.
fn parse_key(name: &str) -> Option<Key> {
    Some(match name {
        "A" => Key::A,
        "B" => Key::B,
        "C" => Key::C,
        "D" => Key::D,
        "E" => Key::E,
        "F" => Key::F,
        "G" => Key::G,
        "H" => Key::H,
        "I" => Key::I,
        "J" => Key::J,
        "K" => Key::K,
        "L" => Key::L,
        "M" => Key::M,
        "N" => Key::N,
        "O" => Key::O,
        "P" => Key::P,
        "Q" => Key::Q,
        "R" => Key::R,
        "S" => Key::S,
        "T" => Key::T,
        "U" => Key::U,
        "V" => Key::V,
        "W" => Key::W,
        "X" => Key::X,
        "Y" => Key::Y,
        "Z" => Key::Z,
        "0" => Key::Num0,
        "1" => Key::Num1,
        "2" => Key::Num2,
        "3" => Key::Num3,
        "4" => Key::Num4,
        "5" => Key::Num5,
        "6" => Key::Num6,
        "7" => Key::Num7,
        "8" => Key::Num8,
        "9" => Key::Num9,
        "F1" => Key::F1,
        "F2" => Key::F2,
        "F3" => Key::F3,
        "F4" => Key::F4,
        "F5" => Key::F5,
        "F6" => Key::F6,
        "F7" => Key::F7,
        "F8" => Key::F8,
        "F9" => Key::F9,
        "F10" => Key::F10,
        "F11" => Key::F11,
        "F12" => Key::F12,
        "F13" => Key::F13,
        "F14" => Key::F14,
        "F15" => Key::F15,
        "F16" => Key::F16,
        "F17" => Key::F17,
        "F18" => Key::F18,
        "F19" => Key::F19,
        "F20" => Key::F20,
        "F21" => Key::F21,
        "F22" => Key::F22,
        "F23" => Key::F23,
        "F24" => Key::F24,
        "F25" => Key::F25,
        "F26" => Key::F26,
        "F27" => Key::F27,
        "F28" => Key::F28,
        "F29" => Key::F29,
        "F30" => Key::F30,
        "F31" => Key::F31,
        "F32" => Key::F32,
        "F33" => Key::F33,
        "F34" => Key::F34,
        "F35" => Key::F35,
        "Down" | "ArrowDown" => Key::ArrowDown,
        "Left" | "ArrowLeft" => Key::ArrowLeft,
        "Right" | "ArrowRight" => Key::ArrowRight,
        "Up" | "ArrowUp" => Key::ArrowUp,
        "Escape" | "Esc" => Key::Escape,
        "Tab" => Key::Tab,
        "Backspace" => Key::Backspace,
        "Enter" | "Return" => Key::Enter,
        "Space" => Key::Space,
        "Insert" | "Ins" => Key::Insert,
        "Delete" | "Del" => Key::Delete,
        "Home" => Key::Home,
        "End" => Key::End,
        "PageUp" | "PgUp" => Key::PageUp,
        "PageDown" | "PgDn" => Key::PageDown,
        _ => return None,
    })
}

/// Parse a combo string like `Ctrl+Alt+K`. Returns `None` if invalid.
#[must_use]
pub fn parse_combo(s: &str) -> Option<Combo> {
    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut key = None;
    for part in s.split('+') {
        let part = part.trim();
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "cmd" | "command" => ctrl = true,
            "alt" | "option" => alt = true,
            "shift" => shift = true,
            _ => {
                if key.is_some() {
                    return None;
                }
                key = Some(parse_key(part)?);
            }
        }
    }
    Some(Combo {
        ctrl,
        alt,
        shift,
        key: key?,
    })
}

/// Build a combo from the current egui input (used while recording a binding).
///
/// Returns the combo for the first pressed key this frame, or `None`.
#[must_use]
pub fn combo_from_input(i: &InputState) -> Option<Combo> {
    for ev in &i.events {
        if let egui::Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } = ev
        {
            if key_name(*key).is_none() {
                continue;
            }
            return Some(Combo {
                ctrl: ctrl_down(*modifiers),
                alt: modifiers.alt,
                shift: modifiers.shift,
                key: *key,
            });
        }
    }
    None
}

/// Whether `combo` was pressed this frame (exact modifier match).
#[must_use]
pub fn combo_matches(c: Combo, i: &InputState) -> bool {
    let m = i.modifiers;
    i.key_pressed(c.key)
        && c.ctrl == ctrl_down(m)
        && c.alt == m.alt
        && c.shift == m.shift
}

/// A catalog entry merged with the user's prefs.
pub struct ResolvedKeybind {
    /// Catalog id.
    pub id: &'static str,
    /// Display title.
    pub title: &'static str,
    /// Parsed combo (override or default); `None` = unbound.
    pub combo: Option<Combo>,
    /// Raw combo string as stored/displayed.
    pub combo_str: String,
    /// Whether the user overrode the default.
    pub overridden: bool,
    /// Action to dispatch (`None` = handled specially by the shell).
    pub action: Option<Action>,
}

/// Resolve the catalog against user preferences.
#[must_use]
pub fn resolve(prefs: &Preferences) -> Vec<ResolvedKeybind> {
    keybind_catalog()
        .into_iter()
        .map(|e| {
            let override_combo = prefs
                .keybinds
                .iter()
                .find(|k| k.action == e.id)
                .map(|k| k.combo.as_str());
            let raw = override_combo.or(e.default_combo).unwrap_or("");
            let overridden = override_combo.is_some();
            ResolvedKeybind {
                id: e.id,
                title: e.title,
                combo: if raw.is_empty() {
                    None
                } else {
                    parse_combo(raw)
                },
                combo_str: if raw.is_empty() {
                    "—".to_string()
                } else {
                    raw.to_string()
                },
                overridden,
                action: e.action,
            }
        })
        .collect()
}

