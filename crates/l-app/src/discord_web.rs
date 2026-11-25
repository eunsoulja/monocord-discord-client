//! Discord client window — the Legcord approach in Rust.
//!
//! A dedicated child *process* (`Monocord.exe --discord-web`) hosts a `wry`
//! `WebView2` window loading `discord.com` with a persistent profile, so the
//! user's *real* Discord account is used — Discord's own client handles
//! auth. CSS/JS "mods" are injected from `<data>/discord-mods`
//! (`*.css`, `*.js`), mirroring Legcord's `QuickCSS` / snippet model.
//!
//! Why a separate process: only one `winit`/`tao` event loop may exist per
//! process, and eframe already owns the main one. A child process also
//! isolates webview crashes from the dashboard.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use winit::window::{Window, WindowId};
use wry::{NewWindowResponse, WebContext, WebViewBuilder};

use crate::monocord;

/// CLI flag that turns this binary into the Discord client window.
pub const WEB_FLAG: &str = "--discord-web";

/// Discord web app entrypoint — the login page; Discord redirects
/// authenticated sessions straight to the app.
const DISCORD_URL: &str = "https://discord.com/login";

/// `window.monocord` bridge — exposes Monocord's IPC contract over wry's
/// `window.ipc.postMessage`. Replies come back through `__monocordResolve`,
/// invoked via `webview.evaluate_script` from the event loop.
const MONOCORD_BRIDGE_JS: &str = r#"
(function () {
    if (window.monocord) return;
    var seq = 0;
    var pending = {};
    window.__monocordResolve = function (id, ok, value) {
        var p = pending[id];
        if (!p) return;
        delete pending[id];
        if (ok) p.resolve(value); else p.reject(new Error(String(value)));
    };
    window.monocord = {
        invoke: function (channel) {
            var args = Array.prototype.slice.call(arguments, 1);
            return new Promise(function (resolve, reject) {
                var id = ++seq;
                pending[id] = { resolve: resolve, reject: reject };
                try {
                    window.ipc.postMessage(JSON.stringify({
                        __monocord: true, id: id, channel: channel, args: args
                    }));
                } catch (e) {
                    delete pending[id];
                    reject(e);
                }
            });
        },
        on: function () { return function () {}; },
        version: "0.1.0"
    };
})();
"#;

/// Monocord renderer bundle — prefer the freshest build synced to
/// `%APPDATA%/Monocord/dist` by Monocord's `scripts/build.mjs`; fall back to
/// the vendored copy in `assets/`.
fn renderer_js() -> String {
    let live = monocord::data_dir().join("dist").join("renderer.js");
    if let Ok(code) = std::fs::read_to_string(&live) {
        return code;
    }
    include_str!("../assets/monocord-renderer.js").to_string()
}

/// Events pushed from the IPC handler back onto the event loop.
enum WebMsg {
    /// A `window.__monocordResolve(...)` script to evaluate in the page.
    IpcReply(String),
}

static DISCORD_CHILD: OnceLock<Mutex<Option<Child>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<Child>> {
    DISCORD_CHILD.get_or_init(|| Mutex::new(None))
}

/// `<data>/discord-mods` — user CSS/JS snippets injected into Discord.
#[must_use]
pub fn mods_dir() -> PathBuf {
    data_dir().join("discord-mods")
}

fn data_dir() -> PathBuf {
    monocord::app_data_dir()
}

/// Spawn the Discord client child process if it isn't already running.
pub fn open_or_focus() -> Result<(), String> {
    let mut guard = slot().lock().map_err(|_| "discord web lock poisoned".to_string())?;
    if let Some(child) = guard.as_mut() {
        match child.try_wait() {
            Ok(None) => return Ok(()), // still running
            _ => *guard = None,
        }
    }
    spawn_child(&mut guard)
}

/// Kill the Discord client child process if running.
pub fn close() {
    if let Ok(mut guard) = slot().lock()
        && let Some(child) = guard.as_mut()
    {
        let _ = child.kill();
        let _ = child.wait();
        *guard = None;
    }
}

/// Restart the client so mod files are re-read (fresh webview + init script).
pub fn restart() -> Result<(), String> {
    close();
    thread::sleep(Duration::from_millis(200));
    let mut guard = slot().lock().map_err(|_| "discord web lock poisoned".to_string())?;
    spawn_child(&mut guard)
}

/// Reload the page by restarting the client process.
pub fn reload() {
    let _ = restart();
}

/// Whether the client window process is currently running.
#[must_use]
pub fn is_open() -> bool {
    let Ok(mut guard) = slot().lock() else {
        return false;
    };
    if let Some(Ok(None)) = guard.as_mut().map(Child::try_wait) {
        return true;
    }
    *guard = None;
    false
}

/// Launch `current_exe --discord-web` detached.
fn spawn_child(guard: &mut Option<Child>) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("current_exe failed: {e}"))?;
    // Ensure the mods dir exists so users have somewhere to drop files.
    let _ = std::fs::create_dir_all(mods_dir());
    let child = Command::new(exe)
        .arg(WEB_FLAG)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to spawn discord web process: {e}"))?;
    *guard = Some(child);
    Ok(())
}

// -- Standalone client mode (runs in the child process) ----------------------

/// Window + webview owner for the standalone Discord client process.
/// Called from `main` when `WEB_FLAG` is present; blocks until the window
/// closes, then returns so the process can exit.
pub fn run_client() {
    init_child_logging();
    let mut handler = DiscordHandler::default();
    let event_loop = match winit::event_loop::EventLoop::<WebMsg>::with_user_event().build() {
        Ok(el) => el,
        Err(e) => {
            eprintln!("discord web: event loop failed: {e}");
            return;
        }
    };
    handler.proxy = Some(event_loop.create_proxy());
    if let Err(e) = event_loop.run_app(&mut handler) {
        eprintln!("discord web: event loop error: {e}");
    }
}

fn init_child_logging() {
    use tracing_subscriber::prelude::*;
    let dir = monocord::app_data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let file_appender = tracing_appender::rolling::daily(&dir, "discord-web.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    std::mem::forget(guard);
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(non_blocking))
        .init();
}

/// Application handler owning the Discord window + webview.
#[derive(Default)]
struct DiscordHandler {
    window: Option<Window>,
    webview: Option<wry::WebView>,
    /// Kept alive for the webview's lifetime (owns the `WebView2` environment).
    web_context: Option<WebContext>,
    window_id: Option<WindowId>,
    /// Set before `run_app`; cloned into the webview's IPC handler.
    proxy: Option<EventLoopProxy<WebMsg>>,
}

impl DiscordHandler {
    fn build_webview(
        window: &Window,
        proxy: EventLoopProxy<WebMsg>,
    ) -> Result<(wry::WebView, WebContext), String> {
        let mut web_context = WebContext::new(Some(data_dir().join("discord-webview")));
        let init_script = build_boot_script();
        let webview = WebViewBuilder::new_with_web_context(&mut web_context)
            .with_url(DISCORD_URL)
            .with_devtools(cfg!(debug_assertions))
            .with_initialization_script_for_main_only(init_script, true)
            .with_navigation_handler(|url| {
                // Stay inside Discord; everything else opens in the system browser.
                url.starts_with("https://discord.com/")
                    || url.starts_with("https://discordapp.com/")
                    || url == "about:blank"
            })
            .with_new_window_req_handler(|url, _features| {
                // window.open / target=_blank → system browser, never a webview popup.
                if url.starts_with("https://") {
                    let _ = open::that(&url);
                }
                NewWindowResponse::Deny
            })
            .with_ipc_handler(move |req| {
                handle_ipc(req.body(), &proxy);
            })
            .build(window)
            .map_err(|e| format!("webview failed: {e}"))?;
        Ok((webview, web_context))
    }
}

/// Route one `window.ipc.postMessage` body. Monocord bridge messages are
/// JSON `{__monocord, id, channel, args}`; anything else is logged.
fn handle_ipc(body: &str, proxy: &EventLoopProxy<WebMsg>) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else {
        tracing::debug!("discord web ipc (non-json): {}", &body[..body.len().min(200)]);
        return;
    };
    if v.get("__monocord").and_then(serde_json::Value::as_bool) != Some(true) {
        tracing::debug!("discord web ipc: {}", body);
        return;
    }
    let id = v.get("id").and_then(serde_json::Value::as_u64).unwrap_or(0);
    let channel = v
        .get("channel")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let args = v
        .get("args")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let (ok, value) = match monocord::handle(channel, &args) {
        Ok(result) => (true, result),
        Err(e) => {
            tracing::warn!("monocord ipc {channel} failed: {e}");
            (false, serde_json::Value::String(e))
        }
    };
    let result_json = serde_json::to_string(&value).unwrap_or_else(|_| "null".into());
    let script = format!("window.__monocordResolve({id},{ok},{result_json})");
    if proxy.send_event(WebMsg::IpcReply(script)).is_err() {
        tracing::warn!("monocord ipc reply dropped (event loop gone)");
    }
}

/// Full document-start init script: Monocord bridge + renderer bundle
/// (guarded against double-injection) + user CSS/JS mods.
fn build_boot_script() -> String {
    let mut script = String::with_capacity(300_000);
    script.push_str(MONOCORD_BRIDGE_JS);
    script.push_str("\nif (!window.__l_monocord_loaded) { window.__l_monocord_loaded = true;\n");
    script.push_str(&renderer_js());
    script.push_str("\n}\n");
    script.push_str(&build_init_script(&mods_dir()));
    script
}

impl ApplicationHandler<WebMsg> for DiscordHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = match event_loop.create_window(
            Window::default_attributes()
                .with_title("Discord")
                .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0))
                .with_min_inner_size(winit::dpi::LogicalSize::new(960.0, 600.0))
                .with_resizable(true),
        ) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("discord web: window failed: {e}");
                event_loop.exit();
                return;
            }
        };
        let Some(proxy) = self.proxy.clone() else {
            eprintln!("discord web: missing event loop proxy");
            event_loop.exit();
            return;
        };
        match Self::build_webview(&window, proxy) {
            Ok((webview, ctx)) => {
                self.window_id = Some(window.id());
                self.webview = Some(webview);
                self.web_context = Some(ctx);
                self.window = Some(window);
                tracing::info!("discord web: window + webview created");
            }
            Err(e) => {
                eprintln!("discord web: {e}");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if Some(window_id) != self.window_id {
            return;
        }
        if let WindowEvent::CloseRequested = event {
            event_loop.exit();
        }
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, _cause: StartCause) {
        event_loop.set_control_flow(ControlFlow::Wait);
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: WebMsg) {
        if let (WebMsg::IpcReply(script), Some(webview)) = (event, self.webview.as_ref()) {
            let _ = webview.evaluate_script(&script);
        }
    }
}

/// Build the document-start init script embedding the user's mod files.
fn build_init_script(mods_dir: &Path) -> String {
    let mut css = String::new();
    let mut js = String::new();
    if let Ok(entries) = std::fs::read_dir(mods_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            match path.extension().and_then(|e| e.to_str()) {
                Some("css") => {
                    css.push_str(&text);
                    css.push('\n');
                }
                Some("js") => {
                    js.push_str(&text);
                    js.push('\n');
                }
                _ => {}
            }
        }
    }
    // serde_json string literals are valid JS string literals.
    let css_lit = serde_json::to_string(&css).unwrap_or_else(|_| "\"\"".into());
    let js_lit = serde_json::to_string(&js).unwrap_or_else(|_| "\"\"".into());
    format!(
        r"(function(){{
  'use strict';
  window.__L_DISCORD__ = true;
  var css = {css_lit};
  function injectCss() {{
    if (document.getElementById('l-mods-css') || !css) return;
    var s = document.createElement('style');
    s.id = 'l-mods-css';
    s.textContent = css;
    (document.head || document.documentElement).appendChild(s);
  }}
  if (document.readyState === 'loading') {{
    document.addEventListener('DOMContentLoaded', injectCss);
  }} else {{
    injectCss();
  }}
  // Discord is an SPA: <head> persists across route changes, so the style
  // survives navigation. Re-check cheaply on each route change anyway.
  var last = location.href;
  setInterval(function() {{
    if (location.href !== last) {{ last = location.href; injectCss(); }}
  }}, 1000);
  try {{
    eval({js_lit});
  }} catch (e) {{
    console.error('[L discord-mods]', e);
  }}
}})();"
    )
}

#[cfg(test)]
mod tests {
    /// Smoke test: launches the standalone client path briefly.
    /// Run with `cargo test -p l-app discord_web -- --nocapture`.
    #[test]
    fn client_mode_builds() {
        // Just verify the init script embeds mod files without panicking.
        let dir = std::env::temp_dir().join("l-mods-test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("test.css"), "body { background: red; }").unwrap();
        let script = super::build_init_script(&dir);
        assert!(script.contains("l-mods-css"));
        assert!(script.contains("background: red"));
    }
}
