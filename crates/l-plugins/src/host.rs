//! Plugin host: discovers plugins, spawns them, and speaks JSON-RPC over
//! newline-delimited JSON on the plugin's stdin/stdout.
//!
//! Protocol (host -> plugin):
//! ```text
//! {"jsonrpc":"2.0","id":1,"method":"initialize","params":{...}}
//! {"jsonrpc":"2.0","id":2,"method":"poll","params":{"view":"cpu","config":{...}}}
//! {"jsonrpc":"2.0","id":3,"method":"command","params":{"command":"refresh","args":{...}}}
//! {"jsonrpc":"2.0","method":"shutdown","params":{}}
//! ```
//! Plugin -> host responses echo the request `id` with `result` or `error`.
//! Plugin -> host notifications use `{"jsonrpc":"2.0","method":"notify",...}`
//! and surface as [`PluginEvent`]s.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

use crate::manifest::{DiscoveredPlugin, PluginWidgetDecl, scan_dir};

/// Events pushed by plugins to the host.
#[derive(Debug, Clone)]
pub enum PluginEvent {
    /// A plugin asked the host to show a notification/toast.
    Notify {
        /// Plugin id.
        plugin: String,
        /// Title.
        title: String,
        /// Body.
        body: String,
    },
    /// A plugin log line.
    Log {
        /// Plugin id.
        plugin: String,
        /// Level (`info`, `warn`, `error`).
        level: String,
        /// Message.
        message: String,
    },
}

/// State of one plugin slot.
#[derive(Debug)]
enum SlotState {
    /// Not running.
    Stopped,
    /// Spawn handshake in progress.
    Starting,
    /// Ready to serve requests.
    Ready,
    /// Failed; reason.
    Failed(String),
}

/// A running plugin process.
struct PluginProcess {
    child: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    pending: Mutex<HashMap<u64, std::sync::mpsc::Sender<Result<Value, String>>>>,
    next_id: AtomicU64,
}

impl PluginProcess {
    fn request(
        &self,
        method: &str,
        params: &Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = std::sync::mpsc::channel();
        self.pending.lock().unwrap().insert(id, tx);
        let msg = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        let send = {
            let mut stdin = self.stdin.lock().unwrap();
            writeln!(stdin, "{msg}").and_then(|()| stdin.flush())
        };
        if let Err(e) = send {
            self.pending.lock().unwrap().remove(&id);
            return Err(format!("plugin stdin: {e}"));
        }
        if let Ok(res) = rx.recv_timeout(timeout) {
            res
        } else {
            self.pending.lock().unwrap().remove(&id);
            Err("plugin request timed out".into())
        }
    }

    fn kill(&self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// One discovered plugin.
struct Slot {
    manifest: crate::manifest::PluginManifestFile,
    dir: PathBuf,
    enabled: bool,
    process: Mutex<Option<Arc<PluginProcess>>>,
    state: Mutex<SlotState>,
}

/// The plugin host. Cheap to share via `Arc`; blocking methods should be
/// called from `spawn_blocking`/worker threads, never the UI thread.
pub struct PluginHost {
    plugins_dir: PathBuf,
    state_file: PathBuf,
    slots: Mutex<HashMap<String, Slot>>,
    events: Arc<Mutex<Vec<PluginEvent>>>,
    init_timeout: Duration,
    request_timeout: Duration,
}

impl PluginHost {
    /// Create a host rooted at `plugins_dir` (`<data>/plugins`). The disabled
    /// state persists in `<data>/plugin-state.json`.
    #[must_use]
    pub fn new(plugins_dir: PathBuf) -> Self {
        let state_file = plugins_dir
            .parent()
            .map_or_else(|| plugins_dir.join("plugin-state.json"), |p| p.join("plugin-state.json"));
        let host = Self {
            plugins_dir,
            state_file,
            slots: Mutex::new(HashMap::new()),
            events: Arc::new(Mutex::new(Vec::new())),
            init_timeout: Duration::from_secs(15),
            request_timeout: Duration::from_secs(10),
        };
        host.rescan();
        host
    }

    fn disabled_set(&self) -> HashSet<String> {
        std::fs::read_to_string(&self.state_file)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v.get("disabled")?.as_array().cloned())
            .map_or_else(HashSet::new, |arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
    }

    fn save_state(&self, disabled: &HashSet<String>) {
        let v = json!({"disabled": disabled.iter().collect::<Vec<_>>()});
        if let Ok(text) = serde_json::to_string_pretty(&v) {
            let _ = std::fs::write(&self.state_file, text);
        }
    }

    /// Rescan the plugins directory and (re)build the slot table. Running
    /// processes for removed plugins are killed; new plugins start lazily.
    pub fn rescan(&self) {
        let disabled = self.disabled_set();
        let found = scan_dir(&self.plugins_dir);
        let mut slots = self.slots.lock().unwrap();
        let mut seen = HashSet::new();
        for d in found {
            let id = d.manifest.id.clone();
            seen.insert(id.clone());
            let enabled = !disabled.contains(&id);
            match slots.get_mut(&id) {
                Some(slot) => {
                    slot.manifest = d.manifest;
                    slot.dir = d.dir;
                    slot.enabled = enabled;
                }
                None => {
                    slots.insert(
                        id.clone(),
                        Slot {
                            manifest: d.manifest,
                            dir: d.dir,
                            enabled,
                            process: Mutex::new(None),
                            state: Mutex::new(SlotState::Stopped),
                        },
                    );
                }
            }
        }
        // Drop slots for removed plugins.
        let removed: Vec<String> = slots
            .keys()
            .filter(|k| !seen.contains(*k))
            .cloned()
            .collect();
        for id in removed {
            if let Some(slot) = slots.remove(&id)
                && let Ok(proc) = slot.process.lock()
                && let Some(p) = proc.as_ref()
            {
                p.kill();
            }
        }
    }

    /// All discovered plugins (enabled flag applied).
    #[must_use]
    pub fn plugins(&self) -> Vec<DiscoveredPlugin> {
        self.slots
            .lock()
            .unwrap()
            .values()
            .map(|s| DiscoveredPlugin {
                manifest: s.manifest.clone(),
                dir: s.dir.clone(),
                enabled: s.enabled,
            })
            .collect()
    }

    /// Widget views across all enabled plugins.
    #[must_use]
    pub fn widget_views(&self) -> Vec<(String, PluginWidgetDecl)> {
        self.slots
            .lock()
            .unwrap()
            .values()
            .filter(|s| s.enabled)
            .flat_map(|s| {
                s.manifest
                    .widgets
                    .iter()
                    .map(|w| (s.manifest.id.clone(), w.clone()))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Palette commands across all enabled plugins.
    #[must_use]
    pub fn commands(&self) -> Vec<(String, crate::manifest::PluginCommandDecl)> {
        self.slots
            .lock()
            .unwrap()
            .values()
            .filter(|s| s.enabled)
            .flat_map(|s| {
                s.manifest
                    .commands
                    .iter()
                    .map(|c| (s.manifest.id.clone(), c.clone()))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Whether a plugin is enabled.
    #[must_use]
    pub fn is_enabled(&self, id: &str) -> bool {
        self.slots
            .lock()
            .unwrap()
            .get(id)
            .is_some_and(|s| s.enabled)
    }

    /// Enable/disable a plugin; persists to disk. Disabling kills the process.
    pub fn set_enabled(&self, id: &str, enabled: bool) {
        let mut disabled = self.disabled_set();
        if enabled {
            disabled.remove(id);
        } else {
            disabled.insert(id.to_string());
        }
        let slot = self.slots.lock().unwrap().get_mut(id).map(|s| {
            s.enabled = enabled;
            // Take the process out so we can kill it after releasing the lock.
            if enabled {
                None
            } else {
                s.process.lock().unwrap().take()
            }
        });
        if let Some(Some(p)) = slot {
            p.kill();
        }
        self.save_state(&disabled);
    }

    /// Plugin directory path (for "open folder" UI).
    #[must_use]
    pub fn plugins_dir(&self) -> &Path {
        &self.plugins_dir
    }

    /// Drain queued plugin events.
    #[must_use]
    pub fn drain_events(&self) -> Vec<PluginEvent> {
        std::mem::take(&mut *self.events.lock().unwrap())
    }

    /// Ensure the plugin process is running and initialized.
    fn ensure_running(&self, id: &str) -> Result<(), String> {
        // Snapshot slot info.
        let (dir, entry, args) = {
            let slots = self.slots.lock().unwrap();
            let s = slots
                .get(id)
                .ok_or_else(|| format!("unknown plugin: {id}"))?;
            if !s.enabled {
                return Err(format!("plugin disabled: {id}"));
            }
            (s.dir.clone(), s.manifest.entry.clone(), s.manifest.args.clone())
        };

        // Fast path: already ready.
        {
            let slots = self.slots.lock().unwrap();
            if let Some(s) = slots.get(id)
                && let Ok(st) = s.state.lock()
                && matches!(*st, SlotState::Ready)
                && s.process.lock().unwrap().is_some()
            {
                return Ok(());
            }
        }

        // Spawn the entry point.
        let entry_path = if Path::new(&entry).is_absolute() {
            PathBuf::from(&entry)
        } else {
            dir.join(&entry)
        };
        if !entry_path.exists() {
            return Err(format!("plugin entry not found: {}", entry_path.display()));
        }
        let mut cmd = Command::new(&entry_path);
        cmd.args(&args)
            .current_dir(&dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // Don't flash a console window for console-subsystem plugins.
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("spawn {}: {e}", entry_path.display()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "plugin stdin unavailable".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "plugin stdout unavailable".to_string())?;

        let proc = Arc::new(PluginProcess {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        });

        // Reader thread: route responses by id, notifications to events.
        {
            let proc2 = proc.clone();
            let events = self.events.clone();
            let plugin_id = id.to_string();
            std::thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let Ok(v) = serde_json::from_str::<Value>(trimmed) else {
                        continue;
                    };
                    if let Some(rid) = v.get("id").and_then(Value::as_u64) {
                        let sender = proc2.pending.lock().unwrap().remove(&rid);
                        if let Some(tx) = sender {
                            if let Some(err) = v.get("error") {
                                let _ = tx.send(Err(err.to_string()));
                            } else {
                                let _ =
                                    tx.send(Ok(v.get("result").cloned().unwrap_or(Value::Null)));
                            }
                        }
                    } else {
                        match v.get("method").and_then(Value::as_str) {
                            Some("notify") => {
                                let params = v.get("params").cloned().unwrap_or(Value::Null);
                                let mut evs = events.lock().unwrap();
                                evs.push(PluginEvent::Notify {
                                    plugin: plugin_id.clone(),
                                    title: params
                                        .get("title")
                                        .and_then(Value::as_str)
                                        .unwrap_or("")
                                        .to_string(),
                                    body: params
                                        .get("body")
                                        .and_then(Value::as_str)
                                        .unwrap_or("")
                                        .to_string(),
                                });
                            }
                            Some("log") => {
                                let params = v.get("params").cloned().unwrap_or(Value::Null);
                                let mut evs = events.lock().unwrap();
                                evs.push(PluginEvent::Log {
                                    plugin: plugin_id.clone(),
                                    level: params
                                        .get("level")
                                        .and_then(Value::as_str)
                                        .unwrap_or("info")
                                        .to_string(),
                                    message: params
                                        .get("message")
                                        .and_then(Value::as_str)
                                        .unwrap_or("")
                                        .to_string(),
                                });
                            }
                            _ => {}
                        }
                    }
                }
                // EOF: process exited — fail pending requests and reset state.
                let pending = std::mem::take(&mut *proc2.pending.lock().unwrap());
                for (_, tx) in pending {
                    let _ = tx.send(Err("plugin exited".into()));
                }
            });
        }

        // Store the process, then run the initialize handshake.
        {
            let mut slots = self.slots.lock().unwrap();
            if let Some(s) = slots.get_mut(id) {
                *s.process.lock().unwrap() = Some(proc.clone());
                *s.state.lock().unwrap() = SlotState::Starting;
            }
        }
        let init = proc.request(
            "initialize",
            &json!({"host":"l","host_version":env!("CARGO_PKG_VERSION")}),
            self.init_timeout,
        );
        match init {
            Ok(_) => {
                let mut slots = self.slots.lock().unwrap();
                if let Some(s) = slots.get_mut(id) {
                    *s.state.lock().unwrap() = SlotState::Ready;
                }
                Ok(())
            }
            Err(e) => {
                proc.kill();
                let mut slots = self.slots.lock().unwrap();
                if let Some(s) = slots.get_mut(id) {
                    *s.process.lock().unwrap() = None;
                    *s.state.lock().unwrap() = SlotState::Failed(e.clone());
                }
                Err(e)
            }
        }
    }

    /// Poll a widget view. Blocking — call via `spawn_blocking`.
    pub fn poll(&self, plugin: &str, view: &str, config: &Value) -> Result<Value, String> {
        self.ensure_running(plugin)?;
        let proc = {
            let slots = self.slots.lock().unwrap();
            slots
                .get(plugin)
                .and_then(|s| s.process.lock().unwrap().clone())
                .ok_or_else(|| "plugin not running".to_string())?
        };
        proc.request(
            "poll",
            &json!({"view": view, "config": config}),
            self.request_timeout,
        )
    }

    /// Invoke a plugin command. Blocking — call via `spawn_blocking`.
    pub fn invoke(&self, plugin: &str, command: &str, args: &Value) -> Result<Value, String> {
        self.ensure_running(plugin)?;
        let proc = {
            let slots = self.slots.lock().unwrap();
            slots
                .get(plugin)
                .and_then(|s| s.process.lock().unwrap().clone())
                .ok_or_else(|| "plugin not running".to_string())?
        };
        proc.request(
            "command",
            &json!({"command": command, "args": args}),
            self.request_timeout,
        )
    }

    /// Plugin runtime state for the settings UI.
    #[must_use]
    pub fn state_of(&self, id: &str) -> String {
        let slots = self.slots.lock().unwrap();
        slots.get(id).map_or_else(
            || "missing".to_string(),
            |s| match &*s.state.lock().unwrap() {
                SlotState::Stopped => "stopped".to_string(),
                SlotState::Starting => "starting".to_string(),
                SlotState::Ready => "ready".to_string(),
                SlotState::Failed(e) => format!("failed: {e}"),
            },
        )
    }

    /// Kill every plugin process (app shutdown).
    pub fn shutdown_all(&self) {
        let slots = self.slots.lock().unwrap();
        for s in slots.values() {
            if let Ok(proc) = s.process.lock()
                && let Some(p) = proc.as_ref()
            {
                let _ = p.request(
                    "shutdown",
                    &json!({}),
                    Duration::from_millis(500),
                );
                p.kill();
            }
        }
    }
}
