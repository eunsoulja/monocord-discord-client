//! Monocord IPC bridge — Rust side.
//!
//! Monocord's renderer bundle (`dist/renderer.js`, injected into the Discord
//! webview by `discord_web`) expects an Electron-style `window.monocord`
//! bridge. This module implements the same `monocord:*` channel contract
//! against the filesystem at `%APPDATA%/Monocord`, so plugins, themes, and
//! settings are shared with the standalone Monocord install.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

/// Monocord data dir — same location the Monocord desktop app uses
/// (`%APPDATA%/Monocord` on Windows).
#[must_use]
pub fn data_dir() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata).join("Monocord");
    }
    directories::ProjectDirs::from("", "", "Monocord").map_or_else(
        || PathBuf::from("Monocord-data"),
        |d| d.data_dir().to_path_buf(),
    )
}

fn settings_file() -> PathBuf {
    data_dir().join("renderer-settings.json")
}

fn themes_dir() -> PathBuf {
    data_dir().join("themes")
}

fn plugins_dir() -> PathBuf {
    data_dir().join("plugins")
}

fn messages_log() -> PathBuf {
    data_dir().join("logs").join("messages.jsonl")
}

/// Built-in themes shipped inside Monocord (`assets/monocord-themes.json`,
/// generated from `src/core/bootstrap/builtin-themes.ts`).
const BUILTIN_THEMES: &str = include_str!("../assets/monocord-themes.json");

#[derive(serde::Deserialize)]
struct BuiltinTheme {
    name: String,
    label: String,
    authors: Vec<String>,
    #[serde(default)]
    css: String,
}

fn builtin_themes() -> Vec<BuiltinTheme> {
    serde_json::from_str(BUILTIN_THEMES).unwrap_or_default()
}

/// Dispatch one `monocord:<channel>` invoke. `args` are the arguments after
/// the channel name. Mirrors `registerIpc` in Monocord's `bootstrap/main.ts`.
pub fn handle(channel: &str, args: &[Value]) -> Result<Value, String> {
    match channel {
        "settings-get" => Ok(read_settings()),
        "settings-save" => {
            let data = args.first().cloned().unwrap_or(Value::Null);
            if data.is_object() {
                write_settings(&data)?;
            }
            Ok(json!(true))
        }
        "settings-set" => {
            let path = args.first().and_then(Value::as_str).unwrap_or("");
            let value = args.get(1).cloned().unwrap_or(Value::Null);
            let mut data = read_settings_object();
            set_path(&mut data, path, value);
            write_settings(&Value::Object(data))?;
            Ok(json!(true))
        }
        "themes-list" => Ok(themes_list()),
        "theme-get" => {
            let name = args.first().and_then(Value::as_str).unwrap_or("");
            Ok(theme_css(name).map_or(Value::Null, Value::String))
        }
        "plugins-list" => Ok(plugins_list()),
        "plugin-get" => {
            let name = args.first().and_then(Value::as_str).unwrap_or("");
            Ok(plugin_code(name).map_or(Value::Null, Value::Object))
        }
        "rpc-set" | "rpc-clear" | "track-block" | "badge-block" => Ok(json!(true)),
        "msglog" => {
            append_message_log(args.first().unwrap_or(&Value::Null));
            Ok(json!(true))
        }
        "open-external" => {
            if let Some(url) = args.first().and_then(Value::as_str)
                && (url.starts_with("https://") || url.starts_with("http://"))
            {
                let _ = open::that(url);
            }
            Ok(json!(true))
        }
        "log" => {
            let level = args.first().and_then(Value::as_str).unwrap_or("info");
            let scope = args.get(1).and_then(Value::as_str).unwrap_or("");
            let message = args.get(2).and_then(Value::as_str).unwrap_or("");
            match level {
                "error" => tracing::error!("{scope}: {message}"),
                "warn" | "warning" => tracing::warn!("{scope}: {message}"),
                "debug" | "trace" => tracing::debug!("{scope}: {message}"),
                _ => tracing::info!("{scope}: {message}"),
            }
            Ok(Value::Null)
        }
        "version" => Ok(json!({ "monocord": "0.1.0", "discord": "webview" })),
        other => Err(format!("unknown monocord channel: {other}")),
    }
}

// -- settings ----------------------------------------------------------------

fn read_settings_object() -> Map<String, Value> {
    std::fs::read_to_string(settings_file())
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

fn read_settings() -> Value {
    Value::Object(read_settings_object())
}

fn write_settings(data: &Value) -> Result<(), String> {
    let path = settings_file();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// Dot-path set mirroring Monocord's `setPath`.
fn set_path(obj: &mut Map<String, Value>, path: &str, value: Value) {
    let mut parts = path.split('.').peekable();
    let mut cur = obj;
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            cur.insert(part.to_string(), value);
            return;
        }
        let entry = cur
            .entry(part.to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        // SAFETY-FREE: is_object checked above.
        cur = entry.as_object_mut().expect("object checked");
    }
}

// -- themes ------------------------------------------------------------------

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

fn theme_entry(mut map: Map<String, Value>) -> Value {
    // Ensure `name` present; drop nulls for a clean payload.
    map.retain(|_, v| !v.is_null());
    Value::Object(map)
}

fn themes_list() -> Value {
    let mut out: Vec<Value> = builtin_themes()
        .iter()
        .map(|t| {
            theme_entry(Map::from_iter([
                ("name".into(), json!(t.name)),
                ("label".into(), json!(t.label)),
                ("authors".into(), json!(t.authors)),
            ]))
        })
        .collect();
    let mut seen: std::collections::HashSet<String> =
        builtin_themes().into_iter().map(|t| t.name).collect();

    let Ok(entries) = std::fs::read_dir(themes_dir()) else {
        return Value::Array(out);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            // Vencord-style loose .css / .theme.css files.
            let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !file_name.to_ascii_lowercase().ends_with(".css") {
                continue;
            }
            // Mirror Monocord: name = filename minus the trailing ".css"
            // (case-insensitive, already verified above).
            let theme_name = file_name[..file_name.len() - 4].to_string();
            if !seen.insert(theme_name.clone()) {
                continue;
            }
            let css = std::fs::read_to_string(&path).unwrap_or_default();
            let info = parse_theme_header(&css, file_name);
            let mut map = Map::new();
            map.insert("name".into(), json!(theme_name));
            if let Some(label) = info.label {
                map.insert("label".into(), json!(label));
            }
            if let Some(authors) = info.authors {
                map.insert("authors".into(), json!(authors));
            }
            if let Some(d) = info.description {
                map.insert("description".into(), json!(d));
            }
            if let Some(v) = info.version {
                map.insert("version".into(), json!(v));
            }
            if let Some(w) = info.website {
                map.insert("website".into(), json!(w));
            }
            if let Some(i) = info.invite {
                map.insert("invite".into(), json!(i));
            }
            out.push(theme_entry(map));
            continue;
        }
        if !path.is_dir() {
            continue;
        }
        let css_path = path.join("theme.css");
        if !css_path.exists() {
            continue;
        }
        // Optional manifest.json supplies metadata.
        let mut meta = Map::new();
        if let Ok(text) = std::fs::read_to_string(path.join("manifest.json"))
            && let Ok(Value::Object(m)) = serde_json::from_str::<Value>(&text)
        {
            meta = m;
        }
        let dir_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let theme_name = meta
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&dir_name)
            .to_string();
        if !seen.insert(theme_name.clone()) {
            continue;
        }
        // Fall back to the css header for metadata the manifest lacks.
        let css = std::fs::read_to_string(&css_path).unwrap_or_default();
        let info = parse_theme_header(&css, &format!("{theme_name}.css"));
        let mut map = Map::new();
        map.insert("name".into(), json!(theme_name));
        for key in ["label", "description", "version", "website", "invite"] {
            if let Some(v) = meta.get(key).filter(|v| !v.is_null()) {
                map.insert(key.into(), v.clone());
            } else if key == "label"
                && let Some(v) = info.label.clone()
            {
                map.insert(key.into(), json!(v));
            } else if key == "description"
                && let Some(v) = info.description.clone()
            {
                map.insert(key.into(), json!(v));
            } else if key == "version"
                && let Some(v) = info.version.clone()
            {
                map.insert(key.into(), json!(v));
            } else if key == "website"
                && let Some(v) = info.website.clone()
            {
                map.insert(key.into(), json!(v));
            } else if key == "invite"
                && let Some(v) = info.invite.clone()
            {
                map.insert(key.into(), json!(v));
            }
        }
        if let Some(authors) = meta
            .get("authors")
            .filter(|v| !v.is_null())
            .cloned()
            .or_else(|| info.authors.map(Value::from))
        {
            map.insert("authors".into(), authors);
        }
        out.push(theme_entry(map));
    }
    Value::Array(out)
}

fn theme_css(name: &str) -> Option<String> {
    if !valid_name(name) {
        return None;
    }
    // Directory theme: <name>/theme.css.
    let dir_css = themes_dir().join(name).join("theme.css");
    if dir_css.is_file()
        && let Ok(css) = std::fs::read_to_string(&dir_css)
    {
        return Some(css);
    }
    // Loose files: <name>.css then <name>.theme.css.
    for file in [format!("{name}.css"), format!("{name}.theme.css")] {
        let path = themes_dir().join(&file);
        if path.is_file()
            && let Ok(css) = std::fs::read_to_string(&path)
        {
            return Some(css);
        }
    }
    builtin_themes()
        .into_iter()
        .find(|t| t.name == name)
        .map(|t| t.css)
}

// -- plugins -----------------------------------------------------------------

fn plugins_list() -> Value {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(plugins_dir()) else {
        return Value::Array(out);
    };
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let manifest_path = entry.path().join("monocord-plugin.json");
        if let Ok(text) = std::fs::read_to_string(&manifest_path)
            && let Ok(v @ Value::Object(_)) = serde_json::from_str::<Value>(&text)
            && v.get("name").and_then(Value::as_str).is_some()
        {
            out.push(v);
        }
    }
    Value::Array(out)
}

fn plugin_code(name: &str) -> Option<Map<String, Value>> {
    if !valid_name(name) {
        return None;
    }
    let dir = plugins_dir().join(name);
    let manifest_text = std::fs::read_to_string(dir.join("monocord-plugin.json")).ok()?;
    let manifest = serde_json::from_str::<Value>(&manifest_text).ok()?;
    let entry = manifest
        .get("main")
        .and_then(Value::as_str)
        .unwrap_or("index.js");
    if entry.contains("..") || Path::new(entry).is_absolute() {
        return None;
    }
    let code = std::fs::read_to_string(dir.join(entry)).ok()?;
    let mut map = Map::new();
    map.insert("code".into(), json!(code));
    map.insert("manifest".into(), manifest);
    Some(map)
}

// -- misc --------------------------------------------------------------------

fn append_message_log(line: &Value) {
    let path = messages_log();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        use std::io::Write;
        let _ = writeln!(f, "{}", serde_json::to_string(&line).unwrap_or_default());
    }
}

/// BetterDiscord/Vencord-style `/** @name … @author … */` header parser,
/// ported from Monocord's `parseThemeHeader`.
struct ThemeHeader {
    label: Option<String>,
    authors: Option<Vec<String>>,
    description: Option<String>,
    version: Option<String>,
    website: Option<String>,
    invite: Option<String>,
}

fn parse_theme_header(css: &str, file_name: &str) -> ThemeHeader {
    let fallback_name = if file_name.len() > 4 {
        file_name[..file_name.len() - 4].to_string()
    } else {
        file_name.to_string()
    };
    let Some(start) = css.find("/**") else {
        return ThemeHeader::default_named(fallback_name);
    };
    let rest = &css[start + 3..];
    let Some(end) = rest.find("*/") else {
        return ThemeHeader::default_named(fallback_name);
    };
    let block = &rest[..end];

    // Split into `*` continuation segments: each line, stripped of leading
    // whitespace + '*', either starts a new @field or continues the last.
    let mut fields: Vec<(String, String)> = Vec::new();
    let mut field = String::new();
    let mut accum = String::new();
    for raw in block.lines() {
        let line = raw.trim_start().trim_start_matches('*').trim_start();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('@') && !line[1..].starts_with(' ') {
            if !field.is_empty() || !accum.is_empty() {
                fields.push((std::mem::take(&mut field), std::mem::take(&mut accum)));
            }
            let (name, value) = line[1..]
                .split_once(' ')
                .map_or((line[1..].to_string(), String::new()), |(a, b)| {
                    (a.to_string(), b.to_string())
                });
            field = name;
            accum = value;
        } else {
            let cleaned = line.replace("\\n", "\n").replace("\\@", "@");
            accum.push(' ');
            accum.push_str(&cleaned);
        }
    }
    if !field.is_empty() || !accum.is_empty() {
        fields.push((field, accum));
    }

    let get = |key: &str| -> Option<String> {
        fields
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let author = get("author").unwrap_or_else(|| "Unknown Author".into());
    ThemeHeader {
        label: get("name"),
        authors: if author.is_empty() {
            None
        } else {
            Some(vec![author])
        },
        description: get("description"),
        version: get("version"),
        website: get("website"),
        invite: get("invite"),
    }
}

impl ThemeHeader {
    fn default_named(name: String) -> Self {
        Self {
            label: Some(name),
            authors: Some(vec!["Unknown Author".into()]),
            description: None,
            version: None,
            website: None,
            invite: None,
        }
    }
}
