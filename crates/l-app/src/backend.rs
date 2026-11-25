//! Service layer: storage, secrets, Discord REST/OAuth/gateway, and the
//! `FetchFn` that backs the data hub.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use l_core::events::{AppEvent, HydratedData};
use l_core::models::{ChannelEntry, DiscordAccount, GuildInfo};
use l_discord::gateway::{self, GatewayState};
use l_discord::oauth::{self, OAuthTokens};
use l_discord::voice::{self, VoicePhase};
use l_discord::{DemoBackend, DiscordRest};
use l_plugins::PluginHost;
use l_runtime::hub::{FetchFn, FetchResult};
use l_runtime::probes;
use l_storage::repos;
use l_storage::{Db, SecretStore};
use l_widgets::core::types::DataNeed;
use rusqlite::Connection;
use secrecy::SecretString;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Shared mutable backend state read by fetchers (all on tokio tasks).
pub struct BackendShared {
    /// Discord REST client.
    pub rest: DiscordRest,
    /// Database handle.
    pub db: Arc<Db>,
    /// OS keychain.
    pub secrets: SecretStore,
    /// Tokio handle for spawning from sync contexts.
    pub tokio: tokio::runtime::Handle,
    /// App event channel back to the UI.
    pub app_tx: mpsc::UnboundedSender<AppEvent>,

    /// OAuth user access token (refreshed in place).
    pub user_token: Mutex<Option<SecretString>>,
    /// OAuth refresh token.
    pub user_refresh: Mutex<Option<SecretString>>,
    /// Access-token expiry instant.
    pub token_expiry: Mutex<Option<Instant>>,
    /// Granted OAuth scopes (space separated).
    pub token_scope: Mutex<String>,
    /// OAuth client id.
    pub client_id: Mutex<Option<String>>,
    /// OAuth cancellation token.
    pub oauth_cancel: Mutex<Option<CancellationToken>>,

    /// Bot token (verified separately).
    pub bot_token: Mutex<Option<SecretString>>,
    /// Guild ids where the bot is present.
    pub bot_guild_ids: Mutex<Vec<String>>,
    /// Gateway command sender (present while a session is managed).
    pub gateway: Mutex<Option<tokio::sync::mpsc::Sender<gateway::GatewayCommand>>>,
    /// Shared gateway state (voice snapshots, joins). Replaced when a
    /// gateway session starts (the session owns the canonical Arc).
    pub gateway_state: Mutex<Arc<GatewayState>>,

    /// Currently selected guild id for `@selected` resolution.
    pub selected_guild: Mutex<Option<String>>,
    /// Demo mode flag (mirrors prefs).
    pub demo: AtomicBool,
    /// Gateway feature enabled flag (mirrors prefs).
    pub gateway_enabled: AtomicBool,
    /// Discord RPC server enabled flag (mirrors prefs).
    pub rpc_enabled: AtomicBool,
    /// Privileged intents (members, presences, message content).
    pub intents: Mutex<(bool, bool, bool)>,

    /// Last-published local payloads for `local.*` needs.
    pub local_cache: Mutex<HashMap<String, Value>>,
    /// Uptime/api probe history: name -> ring of recent ok flags.
    pub probe_history: Mutex<HashMap<String, VecDeque<bool>>>,
    /// Channel id -> name cache for voice widgets.
    pub channel_names: Mutex<HashMap<String, String>>,
    /// Bot username cached from verification.
    pub bot_username: Mutex<Option<String>>,
    /// Bot user id (snowflake), needed for voice `ConnectionInfo`.
    pub bot_user_id: Mutex<Option<String>>,
    /// Songbird voice runtime (driver + join handshake).
    pub voice_rt: voice::SharedVoice,
    /// Last gateway heartbeat latency.
    pub bot_latency: Mutex<Option<u64>>,
    /// Pending export destination chosen via file dialog.
    pub export_path: Mutex<Option<PathBuf>>,
    /// External plugin host (`<data>/plugins`).
    pub plugins: Arc<PluginHost>,
}

impl BackendShared {
    fn new(tokio: tokio::runtime::Handle, app_tx: mpsc::UnboundedSender<AppEvent>) -> Arc<Self> {
        Arc::new(Self {
            rest: DiscordRest::new(),
            db: Arc::new(open_db()),
            secrets: SecretStore::new(),
            tokio,
            app_tx,
            user_token: Mutex::new(None),
            user_refresh: Mutex::new(None),
            token_expiry: Mutex::new(None),
            token_scope: Mutex::new(String::new()),
            client_id: Mutex::new(None),
            oauth_cancel: Mutex::new(None),
            bot_token: Mutex::new(None),
            bot_guild_ids: Mutex::new(Vec::new()),
            gateway: Mutex::new(None),
            gateway_state: Mutex::new(Arc::new(GatewayState::default())),
            selected_guild: Mutex::new(None),
            demo: AtomicBool::new(true),
            gateway_enabled: AtomicBool::new(false),
            rpc_enabled: AtomicBool::new(true),
            intents: Mutex::new((false, false, false)),
            local_cache: Mutex::new(HashMap::new()),
            probe_history: Mutex::new(HashMap::new()),
            channel_names: Mutex::new(HashMap::new()),
            bot_username: Mutex::new(None),
            bot_user_id: Mutex::new(None),
            voice_rt: voice::shared(),
            bot_latency: Mutex::new(None),
            export_path: Mutex::new(None),
            plugins: {
                let dir = data_dir().join("plugins");
                let _ = std::fs::create_dir_all(&dir);
                Arc::new(PluginHost::new(dir))
            },
        })
    }

    /// Shared gateway state (empty until a session starts).
    pub fn gw(&self) -> Arc<GatewayState> {
        self.gateway_state.lock().unwrap().clone()
    }
}

/// App data directory (`%APPDATA%/l` on Windows, `~/.local/share/l` on Linux).
fn data_dir() -> PathBuf {
    directories::ProjectDirs::from("dev", "l", "l").map_or_else(
        || PathBuf::from("."),
        |d| d.data_dir().to_path_buf(),
    )
}

fn open_db() -> Db {
    let dir = directories::ProjectDirs::from("dev", "l", "l").map_or_else(
        || PathBuf::from("l.db"),
        |d| {
            let _ = std::fs::create_dir_all(d.data_dir());
            d.data_dir().join("l.db")
        },
    );
    match Db::open(dir) {
        Ok(db) => db,
        Err(e) => {
            tracing::error!("db open failed: {e}; falling back to memory");
            Db::open_in_memory().expect("in-memory db")
        }
    }
}

/// Build the backend and return (shared, `fetch_fn`).
pub fn build(
    tokio: tokio::runtime::Handle,
    app_tx: mpsc::UnboundedSender<AppEvent>,
) -> (Arc<BackendShared>, FetchFn) {
    let shared = BackendShared::new(tokio, app_tx);
    // Load persisted secrets into memory.
    load_secrets(&shared);
    let fetch = make_fetch_fn(shared.clone());
    (shared, fetch)
}

fn load_secrets(shared: &BackendShared) {
    let s = &shared.secrets;
    if let Ok(Some(t)) = s.get(l_storage::secrets::slot::DISCORD_ACCESS_TOKEN) {
        *shared.user_token.lock().unwrap() = Some(t);
        if let Ok(Some(r)) = s.get(l_storage::secrets::slot::DISCORD_REFRESH_TOKEN) {
            *shared.user_refresh.lock().unwrap() = Some(r);
        }
    }
    if let Ok(Some(t)) = s.get(l_storage::secrets::slot::DISCORD_BOT_TOKEN) {
        *shared.bot_token.lock().unwrap() = Some(t);
    }
}

/// Spawn the initial hydration load.
pub fn hydrate(shared: &Arc<BackendShared>) {
    let db = shared.db.clone();
    let tx = shared.app_tx.clone();
    std::thread::spawn(move || {
        let data = db
            .call(|conn| -> HydratedData {
                HydratedData {
                    prefs: repos::load_preferences(conn).ok().flatten(),
                    layouts: repos::load_layouts(conn).unwrap_or_default(),
                    notes: repos::load_notes(conn).unwrap_or_default(),
                    notifications: repos::load_notifications(conn).unwrap_or_default(),
                    audit_log: repos::load_audit(conn, 200).unwrap_or_default(),
                    widget_instances: Vec::new(),
                }
            })
            .unwrap_or_default();
        let _ = tx.send(AppEvent::Hydrated(Box::new(data)));
    });
}

/// Run a closure on the DB thread without blocking the caller.
pub fn db_call<R, F>(db: &Arc<Db>, f: F)
where
    R: Send + 'static,
    F: FnOnce(&mut Connection) -> R + Send + 'static,
{
    let db = db.clone();
    std::thread::spawn(move || {
        let _ = db.call(f);
    });
}

// ---------------------------------------------------------------------------
// Fetch function for the data hub.
// ---------------------------------------------------------------------------

fn make_fetch_fn(shared: Arc<BackendShared>) -> FetchFn {
    Arc::new(move |need: DataNeed| {
        let shared = shared.clone();
        Box::pin(async move { fetch_need(&shared, need).await })
    })
}

async fn fetch_need(shared: &BackendShared, need: DataNeed) -> FetchResult {
    let key = need.key();
    // Local needs always read the published cache (real data even in demo).
    if key.starts_with("local.") {
        let cache = shared.local_cache.lock().unwrap();
        return Ok(cache
            .get(&key)
            .cloned()
            .unwrap_or_else(|| match key.as_str() {
                "local.notifications" => json!({"notifications": [], "unread": 0}),
                _ => json!({"notes": []}),
            }));
    }

    // Plugin views are local processes — they work even in demo mode.
    if let DataNeed::Plugin {
        plugin,
        view,
        params,
    } = &need
    {
        let host = shared.plugins.clone();
        let (plugin, view, params) = (plugin.clone(), view.clone(), params.clone());
        return tokio::task::spawn_blocking(move || {
            let config = serde_json::from_str::<Value>(&params).unwrap_or(Value::Null);
            host.poll(&plugin, &view, &config)
        })
        .await
        .unwrap_or_else(|e| Err(format!("plugin poll join: {e}")));
    }

    if shared.demo.load(Ordering::Relaxed) {
        return Ok(DemoBackend.fetch(&key));
    }

    match need {
        DataNeed::DiscordAccount => discord_account(shared).await,
        DataNeed::DiscordGuilds => discord_guilds(shared).await,
        DataNeed::GuildStats { guild_id } => guild_stats(shared, &guild_id).await,
        DataNeed::GuildActivity { guild_id } => guild_activity(shared, &guild_id).await,
        DataNeed::GuildVoice { guild_id } => guild_voice(shared, &guild_id).await,
        DataNeed::BotStatus => Ok(bot_status(shared)),
        DataNeed::Notes { .. } | DataNeed::Notifications | DataNeed::Plugin { .. } => {
            Ok(Value::Null)
        }
        DataNeed::UptimeServices { spec_key } => uptime_services(shared, &spec_key).await,
        DataNeed::ApiProbes { endpoints_key } => api_probes(shared, &endpoints_key).await,
        DataNeed::SystemStats => Ok(probes::system_stats()),
    }
}

// ---- Discord fetchers -----------------------------------------------------

/// Get a fresh user token, refreshing if near expiry.
async fn user_token(shared: &BackendShared) -> Result<SecretString, String> {
    let token = shared.user_token.lock().unwrap().clone();
    let Some(token) = token else {
        return Err("not connected".into());
    };
    let expired = shared
        .token_expiry
        .lock()
        .unwrap()
        .map(|e| e.duration_since(Instant::now()) < Duration::from_secs(60))
        .unwrap_or(false);
    if !expired {
        return Ok(token);
    }
    let refresh = shared.user_refresh.lock().unwrap().clone();
    let Some(refresh) = refresh else {
        return Ok(token);
    };
    let cid = shared.client_id.lock().unwrap().clone().unwrap_or_default();
    if cid.is_empty() {
        return Ok(token);
    }
    match oauth::refresh_token(&cid, &refresh).await {
        Ok(t) => {
            let new_token = t.access_token.clone();
            *shared.user_token.lock().unwrap() = Some(t.access_token);
            if let Some(r) = t.refresh_token {
                let _ = shared
                    .secrets
                    .set(l_storage::secrets::slot::DISCORD_REFRESH_TOKEN, &r);
                *shared.user_refresh.lock().unwrap() = Some(r);
            }
            *shared.token_expiry.lock().unwrap() =
                Some(Instant::now() + Duration::from_secs(t.expires_in));
            *shared.token_scope.lock().unwrap() = t.scope;
            let _ = shared
                .secrets
                .set(l_storage::secrets::slot::DISCORD_ACCESS_TOKEN, &new_token);
            Ok(new_token)
        }
        Err(_) => Ok(token), // fall back to possibly-still-valid token
    }
}

async fn discord_account(shared: &BackendShared) -> FetchResult {
    let token = user_token(shared).await?;
    let v = shared
        .rest
        .user_get(&token, "/users/@me")
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "id": v.get("id").and_then(|x| x.as_str()).unwrap_or(""),
        "username": v.get("username").and_then(|x| x.as_str()).unwrap_or(""),
        "global_name": v.get("global_name"),
        "avatar": v.get("avatar"),
        "scopes": shared.token_scope.lock().unwrap().split(' ').filter(|s| !s.is_empty()).collect::<Vec<_>>(),
    }))
}

async fn discord_guilds(shared: &BackendShared) -> FetchResult {
    let token = user_token(shared).await?;
    let v = shared
        .rest
        .user_get(&token, "/users/@me/guilds")
        .await
        .map_err(|e| e.to_string())?;
    let arr = v.as_array().cloned().unwrap_or_default();
    let bot_ids: HashSet<String> = {
        let g = shared.bot_guild_ids.lock().unwrap();
        g.iter().cloned().collect()
    };
    let guilds: Vec<Value> = arr
        .iter()
        .map(|g| {
            let id = g
                .get("id")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            json!({
                "id": id,
                "name": g.get("name").and_then(|x| x.as_str()).unwrap_or("unknown"),
                "icon": g.get("icon"),
                "owner": g.get("owner").and_then(serde_json::Value::as_bool).unwrap_or(false),
                "permissions": g.get("permissions").and_then(|x| x.as_str()).unwrap_or("0"),
                "bot_present": bot_ids.contains(&id),
                "member_count": g.get("approximate_member_count").and_then(serde_json::Value::as_u64),
                "online_count": g.get("approximate_presence_count").and_then(serde_json::Value::as_u64),
            })
        })
        .collect();
    Ok(Value::Array(guilds))
}

fn resolve_guild(shared: &BackendShared, guild_id: &str) -> Result<String, String> {
    match guild_id {
        "" | "@me" | "@selected" => shared
            .selected_guild
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "no guild selected".to_string()),
        other => Ok(other.to_string()),
    }
}

fn snowflake_unix(id: &str) -> Option<i64> {
    let v: u64 = id.parse().ok()?;
    Some(((v >> 22) as i64 + 1_420_070_400_000) / 1000)
}

async fn guild_stats(shared: &BackendShared, guild_id: &str) -> FetchResult {
    let Ok(gid) = resolve_guild(shared, guild_id) else {
        return Ok(json!({
            "name": "no guild selected",
            "member_count": null, "online_count": null,
            "channel_count": null, "role_count": null,
            "created_at": null, "approximate": true
        }));
    };

    // Prefer bot token for accurate counts.
    let bot = shared.bot_token.lock().unwrap().clone();
    if let Some(bot) = bot {
        let g = shared
            .rest
            .bot_get(&bot, &format!("/guilds/{gid}?with_counts=true"))
            .await;
        if let Ok(g) = g {
            let channels = shared
                .rest
                .bot_get(&bot, &format!("/guilds/{gid}/channels"))
                .await
                .ok()
                .and_then(|v| v.as_array().map(std::vec::Vec::len))
                .map(|n| n as u64);
            let roles = shared
                .rest
                .bot_get(&bot, &format!("/guilds/{gid}/roles"))
                .await
                .ok()
                .and_then(|v| v.as_array().map(|a| a.len().saturating_sub(1) as u64));
            return Ok(json!({
                "name": g.get("name").and_then(|x| x.as_str()).unwrap_or("unknown"),
                "member_count": g.get("approximate_member_count").and_then(serde_json::Value::as_u64),
                "online_count": g.get("approximate_presence_count").and_then(serde_json::Value::as_u64),
                "channel_count": channels,
                "role_count": roles,
                "created_at": snowflake_unix(&gid),
                "approximate": true,
            }));
        }
    }

    // Fallback: user guild list (names only, counts usually absent).
    if let Ok(token) = user_token(shared).await
        && let Ok(v) = shared.rest.user_get(&token, "/users/@me/guilds").await
        && let Some(g) = v.as_array().and_then(|a| {
            a.iter()
                .find(|g| g.get("id").and_then(|x| x.as_str()) == Some(gid.as_str()))
                .cloned()
        })
    {
        return Ok(json!({
            "name": g.get("name").and_then(|x| x.as_str()).unwrap_or("unknown"),
            "member_count": g.get("approximate_member_count").and_then(serde_json::Value::as_u64),
            "online_count": g.get("approximate_presence_count").and_then(serde_json::Value::as_u64),
            "channel_count": null, "role_count": null,
            "created_at": snowflake_unix(&gid),
            "approximate": true,
        }));
    }
    Err("guild stats unavailable".into())
}

async fn guild_activity(shared: &BackendShared, guild_id: &str) -> FetchResult {
    let gid = match resolve_guild(shared, guild_id) {
        Ok(g) => g,
        Err(e) => return Err(e),
    };
    let intents = *shared.intents.lock().unwrap();
    let gw = shared.gw();
    let connected = gw.connected.load(Ordering::SeqCst);
    if intents.0 && connected {
        let joins = gw.joins.lock().await;
        let events: Vec<Value> = joins.get(&gid).map_or_default(|list| {
            list.iter()
                .take(20)
                .map(|(actor, ts)| json!({"kind":"join","actor":actor,"ts":ts,"detail":null}))
                .collect()
        });
        return Ok(json!({"members_available": true, "events": events}));
    }
    Ok(json!({"members_available": false, "events": []}))
}

async fn guild_voice(shared: &BackendShared, guild_id: &str) -> FetchResult {
    let gid = match resolve_guild(shared, guild_id) {
        Ok(g) => g,
        Err(e) => return Err(e),
    };
    let gw = shared.gw();
    let connected = gw.connected.load(Ordering::SeqCst);
    if !connected {
        return Ok(json!({"available": false, "reason": "gateway not connected", "channels": []}));
    }
    let voice = gw.voice.lock().await;
    let Some(snap) = voice.get(&gid) else {
        return Ok(json!({"available": true, "channels": []}));
    };
    // Resolve channel names via bot REST when possible.
    let bot = shared.bot_token.lock().unwrap().clone();
    if snap.names.is_empty()
        && let Some(bot) = bot
        && let Ok(v) = shared
            .rest
            .bot_get(&bot, &format!("/guilds/{gid}/channels"))
            .await
        && let Some(arr) = v.as_array()
    {
        let mut names = shared.channel_names.lock().unwrap();
        for c in arr {
            if let (Some(id), Some(name)) = (
                c.get("id").and_then(|x| x.as_str()),
                c.get("name").and_then(|x| x.as_str()),
            ) {
                names.insert(id.to_string(), name.to_string());
            }
        }
    }
    let names = shared.channel_names.lock().unwrap();
    let channels: Vec<Value> = snap
        .channels
        .iter()
        .map(|(cid, members)| {
            json!({
                "name": snap.names.get(cid).cloned()
                    .or_else(|| names.get(cid).cloned())
                    .unwrap_or_else(|| format!("channel {}", &cid[cid.len().saturating_sub(4)..])),
                "participants": members.len(),
            })
        })
        .collect();
    Ok(json!({"available": true, "channels": channels}))
}

fn bot_status(shared: &BackendShared) -> Value {
    let bot = shared.bot_token.lock().unwrap().clone();
    let connected = shared.gw().connected.load(Ordering::SeqCst);
    let status = match (&bot, connected) {
        (None, _) => "not_configured",
        (Some(_), true) => "online",
        (Some(_), false) => "configured",
    };
    let username = shared.bot_username.lock().unwrap().clone();
    let latency = *shared.bot_latency.lock().unwrap();
    json!({
        "status": status,
        "username": username,
        "latency_ms": latency,
        "gateway": if connected { "connected" } else { "disconnected" },
        "error": null,
    })
}

// ---- Probes ---------------------------------------------------------------

fn parse_spec(spec_key: &str) -> Vec<(String, String)> {
    spec_key
        .split('|')
        .filter_map(|pair| {
            let (n, u) = pair.split_once('=')?;
            Some((n.to_string(), u.to_string()))
        })
        .collect()
}

fn record_probe(
    shared: &BackendShared,
    name: &str,
    url: &str,
    ok: bool,
    status: Option<u16>,
    latency_ms: u64,
) {
    // In-memory ring for uptime %.
    {
        let mut hist = shared.probe_history.lock().unwrap();
        let entry = hist.entry(name.to_string()).or_default();
        entry.push_back(ok);
        while entry.len() > 100 {
            entry.pop_front();
        }
    }
    // Persist to SQLite (fire-and-forget on the db thread).
    let db = shared.db.clone();
    let (n, u2, ok2, st, lat) = (name.to_string(), url.to_string(), ok, status, latency_ms);
    let () = db_call_owned(&db, move |conn| {
        repos::record_uptime_check(conn, &n, &u2, ok2, st, lat)
    });
}

fn db_call_owned<R, F>(db: &Arc<Db>, f: F)
where
    R: Send + 'static,
    F: FnOnce(&mut Connection) -> R + Send + 'static,
{
    let db = db.clone();
    std::thread::spawn(move || {
        let _ = db.call(f);
    });
}

fn uptime_pct(shared: &BackendShared, name: &str) -> f64 {
    let hist = shared.probe_history.lock().unwrap();
    match hist.get(name) {
        Some(q) if !q.is_empty() => {
            q.iter().filter(|&&b| b).count() as f64 / q.len() as f64 * 100.0
        }
        _ => 100.0,
    }
}

async fn uptime_services(shared: &BackendShared, spec_key: &str) -> FetchResult {
    let services = parse_spec(spec_key);
    let mut out = Vec::new();
    for (name, url) in services {
        let now = now_secs();
        if let Ok(v) = probes::http_probe(&url).await {
            let status = v
                .get("status")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as u16;
            let latency = v
                .get("latency_ms")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            let ok = v
                .get("ok")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            record_probe(shared, &name, &url, ok, Some(status), latency);
            out.push(json!({
                "name": name, "url": url,
                "uptime_pct": uptime_pct(shared, &name),
                "last_check": now, "last_ok": ok,
                "last_status": status, "last_latency_ms": latency,
            }));
        } else {
            record_probe(shared, &name, &url, false, None, 0);
            out.push(json!({
                "name": name, "url": url,
                "uptime_pct": uptime_pct(shared, &name),
                "last_check": now, "last_ok": false,
                "last_status": null, "last_latency_ms": null,
            }));
        }
    }
    Ok(json!({"services": out}))
}

async fn api_probes(shared: &BackendShared, endpoints_key: &str) -> FetchResult {
    let endpoints = parse_spec(endpoints_key);
    let mut out = Vec::new();
    for (name, url) in endpoints {
        let now = now_secs();
        match probes::http_probe(&url).await {
            Ok(v) => {
                let status = v
                    .get("status")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0) as u16;
                let latency = v
                    .get("latency_ms")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                let ok = v
                    .get("ok")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                record_probe(shared, &name, &url, ok, Some(status), latency);
                out.push(json!({
                    "name": name, "url": url, "status": status,
                    "latency_ms": latency, "last_ok": now, "ok": ok,
                }));
            }
            Err(_) => {
                out.push(json!({
                    "name": name, "url": url, "status": 0,
                    "latency_ms": 0, "last_ok": 0, "ok": false,
                }));
            }
        }
    }
    Ok(json!({"results": out}))
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

// ---------------------------------------------------------------------------
// Effect execution.
// ---------------------------------------------------------------------------

/// Execute reducer effects. Called on the UI thread; heavy work is spawned.
pub fn execute_effects(shared: &Arc<BackendShared>, effects: Vec<l_core::effects::Effect>) {
    for fx in effects {
        execute_one(shared, fx);
    }
}

fn execute_one(shared: &Arc<BackendShared>, fx: l_core::effects::Effect) {
    use l_core::effects::Effect as E;
    use l_storage::repos as r;
    match fx {
        E::SavePreferences(p) => {
            let db = shared.db.clone();
            db_call(&db, move |c| r::save_preferences(c, &p));
        }
        E::SaveWorkspace
        | E::PickImportFile // File dialogs are handled by the app layer.
        | E::PickExportFile
        | E::SyncWidgetSubscriptions
        | E::RefreshWidget(_)
        | E::SaveWidgetInstance(_)
        | E::DeleteWidgetInstance(_)
        | E::VoiceStatus(_) => {
            // The app layer handles these (needs widget manager / UI context).
        }
        E::SaveLayout(l) => {
            let db = shared.db.clone();
            db_call(&db, move |c| r::save_layout(c, &l));
        }
        E::DeleteLayoutRow(id) => {
            let db = shared.db.clone();
            db_call(&db, move |c| r::delete_layout(c, id));
        }
        E::Hydrate => hydrate(shared),
        E::StartOAuth {
            client_id,
            port,
            redirect_uri,
        } => start_oauth(shared, client_id, port, redirect_uri),
        E::CancelOAuth => {
            if let Some(ct) = shared.oauth_cancel.lock().unwrap().take() {
                ct.cancel();
            }
        }
        E::DisconnectDiscord => disconnect_discord(shared),
        E::StoreBotToken(token) => {
            let s = SecretString::from(token);
            *shared.bot_token.lock().unwrap() = Some(s.clone());
            let secrets = SecretStore::new();
            std::mem::drop(tokio::task::spawn_blocking(move || {
                let _ = secrets.set(l_storage::secrets::slot::DISCORD_BOT_TOKEN, &s);
            }));
        }
        E::DeleteBotToken => {
            let _ = shared
                .secrets
                .delete(l_storage::secrets::slot::DISCORD_BOT_TOKEN);
            *shared.bot_token.lock().unwrap() = None;
            *shared.bot_guild_ids.lock().unwrap() = Vec::new();
            stop_gateway(shared);
        }
        E::VerifyBot => verify_bot(shared),
        E::FetchDiscordData => fetch_discord_data(shared),
        E::JoinInvite { input } => join_invite(shared, &input),
        E::FetchMessages { channel_id } => fetch_messages(shared, channel_id),
        E::ChatSend { channel_id, content } => chat_send(shared, channel_id, content),
        E::ChatEdit {
            channel_id,
            message_id,
            content,
        } => chat_edit(shared, channel_id, message_id, content),
        E::ChatDelete {
            channel_id,
            message_id,
        } => chat_delete(shared, channel_id, message_id),
        E::ChatReact {
            channel_id,
            message_id,
            emoji,
            add,
        } => chat_react(shared, channel_id, message_id, emoji, add),
        E::ChatTyping { channel_id } => chat_typing(shared, channel_id),
        E::VoiceJoin {
            guild_id,
            channel_id,
        } => voice_join(shared, guild_id, channel_id),
        E::VoiceLeave => voice_leave(shared),
        E::VoiceFlags { muted, deafened } => voice_flags(shared, muted, deafened),
        E::DiscordRest(req) => discord_rest(shared, req),
        E::OsNotification { title, body } => {
            // No OS-notification crate wired; surface as in-app toast only.
            let _ = (title, body);
        }
        E::OpenUrl(url) => {
            if url.starts_with("https://") {
                let _ = open::that(&url);
            }
        }
        E::OpenDiscordClient => {
            if let Err(e) = crate::discord_web::open_or_focus() {
                tracing::error!("discord web open failed: {e}");
            }
        }
        E::RestartDiscordClient => {
            if let Err(e) = crate::discord_web::restart() {
                tracing::error!("discord web restart failed: {e}");
            }
        }
        E::WriteExportFile { name, contents } => {
            write_export(shared, &name, contents);
        }
        E::SaveNote(n) => {
            let db = shared.db.clone();
            db_call(&db, move |c| r::save_note(c, &n));
        }
        E::DeleteNoteRow(id) => {
            let db = shared.db.clone();
            db_call(&db, move |c| r::delete_note(c, id));
        }
        E::SaveNotification(n) => {
            let db = shared.db.clone();
            db_call(&db, move |c| r::save_notification(c, &n));
        }
        E::DeleteNotificationRow(id) => {
            let db = shared.db.clone();
            db_call(&db, move |c| r::delete_notification(c, id));
        }
        E::ClearNotificationsRow => {
            let db = shared.db.clone();
            db_call(&db, r::clear_notifications);
        }
        E::ServiceToast { text, kind } => {
            let _ = shared.app_tx.send(AppEvent::ServiceToast { text, kind });
        }
        E::AppendAudit(entry) => {
            let db = shared.db.clone();
            db_call(&db, move |c| r::append_audit(c, &entry));
        }
        E::PluginInvoke { plugin, command } => plugin_invoke(shared, plugin, command),
    }
}

/// Run a plugin palette command off the UI thread; surface the result as a
/// service toast.
fn plugin_invoke(shared: &Arc<BackendShared>, plugin: String, command: String) {
    let host = shared.plugins.clone();
    let app_tx = shared.app_tx.clone();
    shared.tokio.spawn(async move {
        let label = command.clone();
        let result = tokio::task::spawn_blocking(move || {
            host.invoke(&plugin, &command, &json!({}))
        })
        .await;
        let (text, kind) = match result {
            Ok(Ok(v)) => {
                let msg = v
                    .get("message")
                    .and_then(Value::as_str)
                    .map_or_else(|| v.to_string(), str::to_string);
                (format!("{label}: {msg}"), "success".to_string())
            }
            Ok(Err(e)) => (format!("{label}: {e}"), "error".to_string()),
            Err(e) => (format!("{label}: {e}"), "error".to_string()),
        };
        let _ = app_tx.send(AppEvent::ServiceToast { text, kind });
    });
}

// -- OAuth ------------------------------------------------------------------

fn start_oauth(
    shared: &Arc<BackendShared>,
    client_id: String,
    port: u16,
    redirect_uri: Option<String>,
) {
    // Cancel any in-flight flow.
    if let Some(ct) = shared.oauth_cancel.lock().unwrap().take() {
        ct.cancel();
    }
    let ct = CancellationToken::new();
    *shared.oauth_cancel.lock().unwrap() = Some(ct.clone());
    *shared.client_id.lock().unwrap() = Some(client_id.clone());

    let shared2 = shared.clone();
    shared.tokio.spawn(async move {
        let cfg = oauth::OAuthConfig {
            client_id,
            port,
            redirect_uri,
        };
        let (url, fut) = match oauth::run_flow(cfg).await {
            Ok(v) => v,
            Err(e) => {
                let _ = shared2.app_tx.send(AppEvent::OAuthCompleted {
                    ok: false,
                    error: Some(e.to_string()),
                });
                return;
            }
        };
        let _ = open::that(&url);
        let result = tokio::select! {
            r = fut => r,
            () = ct.cancelled() => {
                Err(oauth::OAuthError::Denied("cancelled".into()))
            }
        };
        match result {
            Ok(tokens) => {
                persist_tokens(&shared2, tokens).await;
                let _ = shared2.app_tx.send(AppEvent::OAuthCompleted {
                    ok: true,
                    error: None,
                });
            }
            Err(e) => {
                let _ = shared2.app_tx.send(AppEvent::OAuthCompleted {
                    ok: false,
                    error: Some(e.to_string()),
                });
            }
        }
    });
}

async fn persist_tokens(shared: &Arc<BackendShared>, tokens: OAuthTokens) {
    let access = tokens.access_token.clone();
    let refresh = tokens.refresh_token.clone();
    shared.token_scope.lock().unwrap().clone_from(&tokens.scope);
    *shared.token_expiry.lock().unwrap() =
        Some(Instant::now() + Duration::from_secs(tokens.expires_in));
    *shared.user_token.lock().unwrap() = Some(tokens.access_token);
    *shared.user_refresh.lock().unwrap() = tokens.refresh_token;
    // Keychain writes are blocking; run on a thread.
    let secrets = SecretStore::new();
    let acc = access.clone();
    let ref_tok = refresh.clone();
    let _ = tokio::task::spawn_blocking(move || {
        let _ = secrets.set(l_storage::secrets::slot::DISCORD_ACCESS_TOKEN, &acc);
        if let Some(r) = &ref_tok {
            let _ = secrets.set(l_storage::secrets::slot::DISCORD_REFRESH_TOKEN, r);
        }
    })
    .await;
}

fn disconnect_discord(shared: &Arc<BackendShared>) {
    // Revoke best-effort.
    let token = shared.user_token.lock().unwrap().take();
    let cid = shared.client_id.lock().unwrap().clone();
    if let (Some(t), Some(cid)) = (token, cid) {
        shared.tokio.spawn(async move {
            oauth::revoke_token(&cid, &t).await;
        });
    }
    *shared.user_refresh.lock().unwrap() = None;
    *shared.token_expiry.lock().unwrap() = None;
    *shared.token_scope.lock().unwrap() = String::new();
    let secrets = SecretStore::new();
    std::mem::drop(tokio::task::spawn_blocking(move || {
        let _ = secrets.delete(l_storage::secrets::slot::DISCORD_ACCESS_TOKEN);
        let _ = secrets.delete(l_storage::secrets::slot::DISCORD_REFRESH_TOKEN);
    }));
    stop_gateway(shared);
}

// -- Discord data fetch / bot verify / REST -------------------------------

/// Extract an invite code from a Discord invite URL or bare code.
fn invite_code(input: &str) -> Option<String> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    let last = s.rsplit('/').next().unwrap_or(s);
    let last = last.split(['?', '#']).next().unwrap_or(last);
    let code = if last.is_empty() { s } else { last };
    let valid = !code.is_empty()
        && code.len() <= 64
        && code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    valid.then(|| code.to_string())
}

fn join_invite(shared: &Arc<BackendShared>, input: &str) {
    let fail = |error: String| {
        let _ = shared.app_tx.send(AppEvent::InviteJoinResult {
            ok: false,
            guild_name: None,
            error: Some(error),
        });
    };
    let Some(code) = invite_code(input) else {
        fail("not a valid invite link or code".into());
        return;
    };
    let has_scope = shared
        .token_scope
        .lock()
        .unwrap()
        .split(' ')
        .any(|s| s == "guilds.join");
    if !has_scope {
        fail("reconnect Discord first — the guilds.join scope is required".into());
        return;
    }
    let shared2 = shared.clone();
    shared.tokio.spawn(async move {
        let token = match user_token(&shared2).await {
            Ok(t) => t,
            Err(e) => {
                let _ = shared2.app_tx.send(AppEvent::InviteJoinResult {
                    ok: false,
                    guild_name: None,
                    error: Some(format!("not connected: {e}")),
                });
                return;
            }
        };
        // Resolve the invite first for a friendly guild name (best effort).
        let name = shared2
            .rest
            .user_get(&token, &format!("/invites/{code}?with_counts=true"))
            .await
            .ok()
            .and_then(|v| {
                v.pointer("/guild/name")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            });
        match shared2
            .rest
            .user_post(&token, &format!("/invites/{code}"), &serde_json::json!({}))
            .await
        {
            Ok(v) => {
                let guild_name = name.or_else(|| {
                    v.get("name")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string)
                });
                let _ = shared2.app_tx.send(AppEvent::InviteJoinResult {
                    ok: true,
                    guild_name,
                    error: None,
                });
                fetch_discord_data(&shared2);
            }
            Err(e) => {
                let _ = shared2.app_tx.send(AppEvent::InviteJoinResult {
                    ok: false,
                    guild_name: None,
                    error: Some(e.to_string()),
                });
            }
        }
    });
}

// ---- Chat (bot messaging) -----------------------------------------------

fn bot_token_opt(shared: &BackendShared) -> Option<SecretString> {
    shared.bot_token.lock().unwrap().clone()
}

fn bot_user_id(shared: &BackendShared) -> String {
    shared
        .bot_user_id
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_default()
}

fn snowflake_ts(id: &str) -> i64 {
    id.parse::<u64>()
        .map_or(0, |v| ((v >> 22) as i64 + 1_420_070_400_000) / 1000)
}

/// Map a Discord message payload to our `ChatMessage` model.
fn map_chat_message(
    v: &Value,
    fallback_channel: &str,
    bot_id: &str,
) -> Option<l_core::models::ChatMessage> {
    use l_core::models::{ChatAttachment, ChatMessage, MessageReaction};
    let id = v.get("id").and_then(Value::as_str)?.to_string();
    let author = v.get("author");
    let author_id = author
        .and_then(|a| a.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let author_name = author
        .and_then(|a| a.get("global_name"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            author
                .and_then(|a| a.get("username"))
                .and_then(Value::as_str)
        })
        .unwrap_or("unknown")
        .to_string();
    let attachments: Vec<ChatAttachment> = v
        .get("attachments")
        .and_then(Value::as_array)
        .map_or_else(Vec::new, |a| {
            a.iter()
                .filter_map(|x| {
                    Some(ChatAttachment {
                        url: x.get("url")?.as_str()?.to_string(),
                        filename: x
                            .get("filename")
                            .and_then(Value::as_str)
                            .unwrap_or("file")
                            .to_string(),
                        content_type: x
                            .get("content_type")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    })
                })
                .collect()
        });
    let reactions: Vec<MessageReaction> =
        v.get("reactions")
            .and_then(Value::as_array)
            .map_or_else(Vec::new, |a| {
                a.iter()
                    .filter_map(|r| {
                        let e = r.get("emoji")?;
                        let name = e.get("name").and_then(Value::as_str)?;
                        let cid = e.get("id").and_then(Value::as_str);
                        let emoji = match cid {
                            Some(id) => format!("{name}:{id}"),
                            None => name.to_string(),
                        };
                        Some(MessageReaction {
                            emoji,
                            count: r
                                .get("count")
                                .and_then(serde_json::Value::as_u64)
                                .unwrap_or(0),
                            me: r
                                .get("me")
                                .and_then(serde_json::Value::as_bool)
                                .unwrap_or(false),
                        })
                    })
                    .collect()
            });
    Some(ChatMessage {
        id: id.clone(),
        channel_id: v
            .get("channel_id")
            .and_then(Value::as_str)
            .unwrap_or(fallback_channel)
            .to_string(),
        guild_id: v
            .get("guild_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        author_id: author_id.clone(),
        author_name,
        content: v
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        timestamp: snowflake_ts(&id),
        edited: v.get("edited").and_then(Value::as_bool).unwrap_or(false),
        attachments,
        reactions,
        is_self: !author_id.is_empty() && author_id == bot_id,
    })
}

fn chat_error(shared: &Arc<BackendShared>, error: &str) {
    let _ = shared.app_tx.send(AppEvent::ServiceToast {
        text: format!("chat: {error}"),
        kind: "error".into(),
    });
}

fn fetch_messages(shared: &Arc<BackendShared>, channel_id: String) {
    let Some(bot) = bot_token_opt(shared) else {
        chat_error(shared, "no bot token configured");
        return;
    };
    let shared2 = shared.clone();
    shared.tokio.spawn(async move {
        let bot_id = bot_user_id(&shared2);
        match l_discord::rest::channel_messages(&shared2.rest, &bot, &channel_id, 50).await {
            Ok(v) => {
                let mut msgs: Vec<l_core::models::ChatMessage> =
                    v.as_array().map_or_else(Vec::new, |a| {
                        a.iter()
                            .filter_map(|m| map_chat_message(m, &channel_id, &bot_id))
                            .collect()
                    });
                // Discord returns newest-first; store oldest-first.
                msgs.reverse();
                let _ = shared2.app_tx.send(AppEvent::MessagesLoaded {
                    channel_id,
                    messages: msgs,
                });
            }
            Err(e) => chat_error(&shared2, &e.to_string()),
        }
    });
}

fn chat_send(shared: &Arc<BackendShared>, channel_id: String, content: String) {
    let Some(bot) = bot_token_opt(shared) else {
        chat_error(shared, "no bot token configured");
        return;
    };
    let shared2 = shared.clone();
    shared.tokio.spawn(async move {
        match l_discord::rest::send_message(&shared2.rest, &bot, &channel_id, &content).await {
            Ok(v) => {
                let bid = bot_user_id(&shared2);
                if let Some(m) = map_chat_message(&v, &channel_id, &bid) {
                    let _ = shared2
                        .app_tx
                        .send(AppEvent::ChatMessageUpsert { message: m });
                }
            }
            Err(e) => chat_error(&shared2, &e.to_string()),
        }
    });
}

fn chat_edit(shared: &Arc<BackendShared>, channel_id: String, message_id: String, content: String) {
    let Some(bot) = bot_token_opt(shared) else {
        chat_error(shared, "no bot token configured");
        return;
    };
    let shared2 = shared.clone();
    shared.tokio.spawn(async move {
        match l_discord::rest::edit_message(&shared2.rest, &bot, &channel_id, &message_id, &content)
            .await
        {
            Ok(v) => {
                let bid = bot_user_id(&shared2);
                if let Some(m) = map_chat_message(&v, &channel_id, &bid) {
                    let _ = shared2
                        .app_tx
                        .send(AppEvent::ChatMessageUpsert { message: m });
                }
            }
            Err(e) => chat_error(&shared2, &e.to_string()),
        }
    });
}

fn chat_delete(shared: &Arc<BackendShared>, channel_id: String, message_id: String) {
    let Some(bot) = bot_token_opt(shared) else {
        chat_error(shared, "no bot token configured");
        return;
    };
    let shared2 = shared.clone();
    shared.tokio.spawn(async move {
        match l_discord::rest::delete_message(&shared2.rest, &bot, &channel_id, &message_id).await {
            Ok(_) => {
                let _ = shared2.app_tx.send(AppEvent::ChatMessageDeleted {
                    channel_id,
                    message_id,
                });
            }
            Err(e) => chat_error(&shared2, &e.to_string()),
        }
    });
}

fn chat_react(
    shared: &Arc<BackendShared>,
    channel_id: String,
    message_id: String,
    emoji: String,
    add: bool,
) {
    let Some(bot) = bot_token_opt(shared) else {
        chat_error(shared, "no bot token configured");
        return;
    };
    let shared2 = shared.clone();
    shared.tokio.spawn(async move {
        let res =
            l_discord::rest::react(&shared2.rest, &bot, &channel_id, &message_id, &emoji, add)
                .await;
        match res {
            Ok(_) => {
                // Refresh the message so reaction counts/me flags update.
                if let Ok(v) =
                    l_discord::rest::get_message(&shared2.rest, &bot, &channel_id, &message_id)
                        .await
                {
                    let bid = bot_user_id(&shared2);
                    if let Some(m) = map_chat_message(&v, &channel_id, &bid) {
                        let _ = shared2
                            .app_tx
                            .send(AppEvent::ChatMessageUpsert { message: m });
                    }
                }
            }
            Err(e) => chat_error(&shared2, &e.to_string()),
        }
    });
}

fn chat_typing(shared: &Arc<BackendShared>, channel_id: String) {
    let Some(bot) = bot_token_opt(shared) else {
        return;
    };
    let shared2 = shared.clone();
    shared.tokio.spawn(async move {
        let _ = l_discord::rest::trigger_typing(&shared2.rest, &bot, &channel_id).await;
    });
}

// ---- Voice (songbird bot voice) ------------------------------------------

/// Build the `VoiceInfo` snapshot for the UI from the runtime.
fn voice_info(shared: &BackendShared) -> l_core::models::VoiceInfo {
    use l_core::models::VoiceInfo;
    let rt = shared.voice_rt.lock().unwrap();
    let mut info = VoiceInfo {
        guild_id: rt.guild_id.clone(),
        channel_id: rt.channel_id.clone(),
        channel_name: shared
            .channel_names
            .lock()
            .unwrap()
            .get(&rt.channel_id)
            .cloned()
            .unwrap_or_default(),
        muted: rt.muted,
        deafened: rt.deafened,
        ..VoiceInfo::default()
    };
    match rt.phase() {
        VoicePhase::Connected => info.connected = true,
        VoicePhase::Connecting => info.connecting = true,
        VoicePhase::Failed(e) => info.error = Some(e),
        VoicePhase::Disconnected => {
            info.guild_id.clear();
            info.channel_id.clear();
            info.channel_name.clear();
        }
    }
    info
}

fn push_voice_update(shared: &Arc<BackendShared>) {
    let info = voice_info(shared);
    let _ = shared.app_tx.send(AppEvent::VoiceUpdate { info });
}

fn voice_join(shared: &Arc<BackendShared>, guild_id: String, channel_id: String) {
    let Some(bot) = bot_token_opt(shared) else {
        chat_error(shared, "no bot token configured");
        return;
    };
    let uid = bot_user_id(shared);
    if uid.is_empty() {
        chat_error(shared, "bot not identified yet — check gateway status");
        return;
    }
    if !shared.gw().connected.load(Ordering::SeqCst) {
        chat_error(
            shared,
            "gateway not connected — enable the gateway in settings",
        );
        return;
    }
    let shared2 = shared.clone();
    shared.tokio.spawn(async move {
        if let Err(e) =
            l_discord::rest::join_voice(&shared2.rest, &bot, &guild_id, &uid, &channel_id).await
        {
            shared2.voice_rt.lock().unwrap().fail(e.to_string());
            push_voice_update(&shared2);
            chat_error(&shared2, &format!("voice join: {e}"));
            return;
        }
        {
            let mut rt = shared2.voice_rt.lock().unwrap();
            rt.bot_user_id = Some(uid.clone());
            rt.begin_join(&guild_id, &channel_id);
        }
        push_voice_update(&shared2);
        // Handshake watchdog: if the gateway doesn't deliver
        // VOICE_STATE_UPDATE + VOICE_SERVER_UPDATE in time, fail and undo
        // the REST join.
        let shared3 = shared2.clone();
        let gid = guild_id.clone();
        let cid = channel_id.clone();
        let bot2 = bot.clone();
        let uid2 = uid.clone();
        shared2.tokio.spawn(async move {
            tokio::time::sleep(Duration::from_secs(15)).await;
            let still_connecting = shared3.voice_rt.lock().unwrap().is_connecting();
            if still_connecting {
                shared3
                    .voice_rt
                    .lock()
                    .unwrap()
                    .fail("voice handshake timed out".into());
                let _ = l_discord::rest::leave_voice(&shared3.rest, &bot2, &gid, &uid2).await;
                push_voice_update(&shared3);
            }
            let _ = cid;
        });
    });
}

fn voice_leave(shared: &Arc<BackendShared>) {
    let (guild_id, channel_id) = {
        let mut rt = shared.voice_rt.lock().unwrap();
        let ids = (rt.guild_id.clone(), rt.channel_id.clone());
        rt.disconnect();
        ids
    };
    let bot = bot_token_opt(shared);
    let uid = bot_user_id(shared);
    if let (Some(bot), false) = (bot, guild_id.is_empty()) {
        let shared2 = shared.clone();
        shared.tokio.spawn(async move {
            let _ = l_discord::rest::leave_voice(&shared2.rest, &bot, &guild_id, &uid).await;
        });
    }
    push_voice_update(shared);
    let _ = channel_id;
}

fn voice_flags(shared: &Arc<BackendShared>, muted: Option<bool>, deafened: Option<bool>) {
    {
        let mut rt = shared.voice_rt.lock().unwrap();
        if let Some(m) = muted {
            rt.set_muted(m);
        }
        if let Some(d) = deafened {
            rt.deafened = d;
        }
    }
    push_voice_update(shared);
    // Mirror self_mute/self_deaf server-side via REST when connected.
    let (guild_id, channel_id) = {
        let rt = shared.voice_rt.lock().unwrap();
        (rt.guild_id.clone(), rt.channel_id.clone())
    };
    let (Some(bot), false) = (bot_token_opt(shared), guild_id.is_empty()) else {
        return;
    };
    let uid = bot_user_id(shared);
    let m = muted.unwrap_or_else(|| shared.voice_rt.lock().unwrap().muted);
    let d = deafened.unwrap_or_else(|| shared.voice_rt.lock().unwrap().deafened);
    let shared2 = shared.clone();
    shared.tokio.spawn(async move {
        let _ = l_discord::rest::set_voice_flags(
            &shared2.rest,
            &bot,
            &guild_id,
            &uid,
            &channel_id,
            m,
            d,
        )
        .await;
    });
}

/// Route a gateway dispatch event to chat state / voice handshake.
fn handle_gateway_dispatch(shared: &Arc<BackendShared>, event: &str, data: &Value) {
    match event {
        "VOICE_STATE_UPDATE" | "VOICE_SERVER_UPDATE" => {
            let info = {
                let mut rt = shared.voice_rt.lock().unwrap();
                if rt.bot_user_id.is_none() {
                    rt.bot_user_id
                        .clone_from(&shared.bot_user_id.lock().unwrap());
                }
                rt.feed_gateway(event, data)
            };
            if let Some(info) = info {
                let shared2 = shared.clone();
                shared.tokio.spawn(async move {
                    let connect = {
                        let mut rt = shared2.voice_rt.lock().unwrap();
                        rt.start_connect(info)
                    };
                    match connect {
                        Ok(fut) => match fut.await {
                            Ok(()) => {
                                shared2.voice_rt.lock().unwrap().mark_connected();
                                push_voice_update(&shared2);
                            }
                            Err(e) => {
                                shared2
                                    .voice_rt
                                    .lock()
                                    .unwrap()
                                    .fail(format!("voice connect: {e}"));
                                push_voice_update(&shared2);
                            }
                        },
                        Err(e) => {
                            shared2.voice_rt.lock().unwrap().fail(e);
                            push_voice_update(&shared2);
                        }
                    }
                });
            }
        }
        "MESSAGE_CREATE" | "MESSAGE_UPDATE" => {
            let channel = data
                .get("channel_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let bid = bot_user_id(shared);
            if let Some(m) = map_chat_message(data, &channel, &bid) {
                let _ = shared
                    .app_tx
                    .send(AppEvent::ChatMessageUpsert { message: m });
            }
        }
        "MESSAGE_DELETE" => {
            let channel_id = data
                .get("channel_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let message_id = data
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if !message_id.is_empty() {
                let _ = shared.app_tx.send(AppEvent::ChatMessageDeleted {
                    channel_id,
                    message_id,
                });
            }
        }
        "TYPING_START" => {
            let channel_id = data
                .get("channel_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let user = data
                .pointer("/member/user/global_name")
                .and_then(Value::as_str)
                .or_else(|| {
                    data.pointer("/member/user/username")
                        .and_then(Value::as_str)
                })
                .unwrap_or("someone")
                .to_string();
            if !channel_id.is_empty() {
                let _ = shared
                    .app_tx
                    .send(AppEvent::TypingEvent { channel_id, user });
            }
        }
        "CHANNEL_CREATE" | "CHANNEL_UPDATE" | "CHANNEL_DELETE" => {
            // Keep the sidebar channel list in sync.
            if let Some(gid) = data.get("guild_id").and_then(Value::as_str) {
                fetch_channels(shared, gid.to_string());
            }
        }
        _ => {}
    }
}

fn fetch_discord_data(shared: &Arc<BackendShared>) {
    let shared2 = shared.clone();
    shared.tokio.spawn(async move {
        let token = match user_token(&shared2).await {
            Ok(t) => t,
            Err(e) => {
                let _ = shared2.app_tx.send(AppEvent::DiscordDataFetched {
                    account: None,
                    guilds: Vec::new(),
                    scopes: Vec::new(),
                });
                let _ = e;
                return;
            }
        };
        let account_v = shared2.rest.user_get(&token, "/users/@me").await.ok();
        let guilds_v = shared2
            .rest
            .user_get(&token, "/users/@me/guilds")
            .await
            .ok();

        let account = account_v.map(|v| DiscordAccount {
            id: v
                .get("id")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
            username: v
                .get("username")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
            global_name: v
                .get("global_name")
                .and_then(|x| x.as_str())
                .map(str::to_string),
            avatar: v.get("avatar").and_then(|x| x.as_str()).map(str::to_string),
            scopes: shared2
                .token_scope
                .lock()
                .unwrap()
                .split(' ')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
        });
        let guilds: Vec<GuildInfo> = guilds_v
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .map(|g| GuildInfo {
                id: g
                    .get("id")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                name: g
                    .get("name")
                    .and_then(|x| x.as_str())
                    .unwrap_or("?")
                    .to_string(),
                icon: g.get("icon").and_then(|x| x.as_str()).map(str::to_string),
                owner: g
                    .get("owner")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                permissions: g
                    .get("permissions")
                    .and_then(|x| x.as_str())
                    .unwrap_or("0")
                    .to_string(),
                bot_present: false,
                member_count: g
                    .get("approximate_member_count")
                    .and_then(serde_json::Value::as_u64),
                online_count: g
                    .get("approximate_presence_count")
                    .and_then(serde_json::Value::as_u64),
            })
            .collect();
        let scopes: Vec<String> = shared2
            .token_scope
            .lock()
            .unwrap()
            .split(' ')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        let _ = shared2.app_tx.send(AppEvent::DiscordDataFetched {
            account,
            guilds,
            scopes,
        });
        refresh_bot_guilds(&shared2).await;
    });
}

async fn refresh_bot_guilds(shared: &Arc<BackendShared>) {
    let Some(bot) = shared.bot_token.lock().unwrap().clone() else {
        return;
    };
    if let Ok(v) = shared.rest.bot_get(&bot, "/users/@me/guilds").await {
        let ids: Vec<String> = v.as_array().map_or_default(|a| {
            a.iter()
                .filter_map(|g| g.get("id").and_then(|x| x.as_str()).map(str::to_string))
                .collect()
        });
        *shared.bot_guild_ids.lock().unwrap() = ids;
        // Mark bot_present on guild list via a fresh fetch.
        let _ = shared.app_tx.send(AppEvent::ServiceToast {
            text: "bot guilds refreshed".into(),
            kind: "info".into(),
        });
    }
}

fn verify_bot(shared: &Arc<BackendShared>) {
    let Some(bot) = shared.bot_token.lock().unwrap().clone() else {
        let _ = shared.app_tx.send(AppEvent::BotVerified {
            ok: false,
            bot_user_id: None,
            bot_username: None,
            error: Some("no token".into()),
        });
        return;
    };
    let tx = shared.app_tx.clone();
    let shared3 = shared.clone();
    shared.tokio.spawn(async move {
        match l_discord::rest::verify_bot(&shared3.rest, &bot).await {
            Ok(v) => {
                let uid = v.get("id").and_then(|x| x.as_str()).map(str::to_string);
                let uname = v
                    .get("username")
                    .and_then(|x| x.as_str())
                    .map(str::to_string);
                shared3.bot_username.lock().unwrap().clone_from(&uname);
                let _ = tx.send(AppEvent::BotVerified {
                    ok: true,
                    bot_user_id: uid,
                    bot_username: uname,
                    error: None,
                });
                refresh_bot_guilds(&shared3).await;
                maybe_start_gateway(&shared3);
            }
            Err(e) => {
                let _ = tx.send(AppEvent::BotVerified {
                    ok: false,
                    bot_user_id: None,
                    bot_username: None,
                    error: Some(e.to_string()),
                });
            }
        }
    });
}

fn discord_rest(shared: &Arc<BackendShared>, req: l_core::effects::DiscordRestRequest) {
    use l_core::effects::DiscordRestRequest as R;
    let shared2 = shared.clone();
    shared.tokio.spawn(async move {
        let res: Result<(u16, Value), String> = match req {
            R::Get(path) => {
                let token = user_token(&shared2).await;
                match token {
                    Ok(t) => shared2
                        .rest
                        .user_get(&t, &path)
                        .await
                        .map(|v| (200u16, v))
                        .map_err(|e| e.to_string()),
                    Err(e) => Err(e),
                }
            }
            R::GetQuery { path, query } => {
                let full = if query.is_empty() {
                    path
                } else {
                    format!("{path}?{query}")
                };
                let token = user_token(&shared2).await;
                match token {
                    Ok(t) => shared2
                        .rest
                        .user_get(&t, &full)
                        .await
                        .map(|v| (200u16, v))
                        .map_err(|e| e.to_string()),
                    Err(e) => Err(e),
                }
            }
            R::BotGet(path) => {
                let bot = shared2.bot_token.lock().unwrap().clone();
                match bot {
                    Some(b) => shared2
                        .rest
                        .bot_get(&b, &path)
                        .await
                        .map(|v| (200u16, v))
                        .map_err(|e| e.to_string()),
                    None => Err("no bot token".into()),
                }
            }
            R::BotPost { path, body } => {
                let bot = shared2.bot_token.lock().unwrap().clone();
                match bot {
                    Some(b) => shared2
                        .rest
                        .bot_post(&b, &path, &body)
                        .await
                        .map(|v| (200u16, v))
                        .map_err(|e| e.to_string()),
                    None => Err("no bot token".into()),
                }
            }
        };
        match res {
            Ok((status, body)) => {
                let _ = shared2.app_tx.send(AppEvent::RestResponse {
                    tag: "rest".into(),
                    status,
                    body,
                });
            }
            Err(e) => {
                let _ = shared2.app_tx.send(AppEvent::RestFailed {
                    tag: "rest".into(),
                    error: e,
                });
            }
        }
    });
}

// -- Gateway -----------------------------------------------------------------

fn maybe_start_gateway(shared: &Arc<BackendShared>) {
    if !shared.gateway_enabled.load(Ordering::Relaxed) {
        return;
    }
    let Some(bot) = shared.bot_token.lock().unwrap().clone() else {
        return;
    };
    if shared.gateway.lock().unwrap().is_some() {
        return;
    }
    let intents = *shared.intents.lock().unwrap();
    let bitfield = gateway::intents_bitfield(intents.0, intents.1, intents.2);
    let mut handle = gateway::spawn(DiscordRest::new(), bot, bitfield);
    // Carry over any RPC-provided activity so presence survives reconnects.
    let new_state = handle.state();
    {
        let prev = shared.gateway_state.lock().unwrap();
        new_state
            .activity
            .lock()
            .unwrap()
            .clone_from(&prev.activity.lock().unwrap());
    }
    *shared.gateway_state.lock().unwrap() = new_state;
    let cmd_tx = handle.command_sender();
    let tx = shared.app_tx.clone();
    let shared3 = shared.clone();
    shared.tokio.spawn(async move {
        while let Some(ev) = handle.recv().await {
            match ev {
                gateway::GatewayEvent::Heartbeat { ms } => {
                    *shared3.bot_latency.lock().unwrap() = Some(ms);
                    let _ = tx.send(AppEvent::GatewayUpdate {
                        state: "connected".into(),
                        latency_ms: Some(ms),
                        connected: true,
                    });
                }
                gateway::GatewayEvent::State(s) => {
                    let _ = tx.send(AppEvent::GatewayUpdate {
                        state: s,
                        latency_ms: None,
                        connected: true,
                    });
                }
                gateway::GatewayEvent::Ready {
                    user_id, username, ..
                } => {
                    *shared3.bot_user_id.lock().unwrap() = Some(user_id.clone());
                    shared3.voice_rt.lock().unwrap().bot_user_id = Some(user_id);
                    let _ = tx.send(AppEvent::GatewayUpdate {
                        state: format!("ready ({username})"),
                        latency_ms: None,
                        connected: true,
                    });
                }
                gateway::GatewayEvent::Dispatch { event, data } => {
                    handle_gateway_dispatch(&shared3, &event, &data);
                }
                gateway::GatewayEvent::Closed(reason) => {
                    *shared3.bot_latency.lock().unwrap() = None;
                    let _ = tx.send(AppEvent::GatewayUpdate {
                        state: reason,
                        latency_ms: None,
                        connected: false,
                    });
                }
            }
        }
    });
    *shared.gateway.lock().unwrap() = Some(cmd_tx);
}

fn stop_gateway(shared: &Arc<BackendShared>) {
    if let Some(tx) = shared.gateway.lock().unwrap().take() {
        let _ = tx.try_send(gateway::GatewayCommand::Disconnect);
    }
}

// -- Channels / gateway triggers ---------------------------------------------

/// Fetch the channel list for a guild (bot token preferred, user fallback)
/// and deliver it as `AppEvent::ChannelsLoaded`.
pub fn fetch_channels(shared: &Arc<BackendShared>, guild_id: String) {
    let shared2 = shared.clone();
    shared.tokio.spawn(async move {
        let mut channels: Option<Vec<ChannelEntry>> = None;
        let bot = shared2.bot_token.lock().unwrap().clone();
        if let Some(bot) = bot
            && let Ok(v) = shared2
                .rest
                .bot_get(&bot, &format!("/guilds/{guild_id}/channels"))
                .await
        {
            channels = Some(map_channels(&guild_id, &v));
        }
        if channels.is_none()
            && let Ok(token) = user_token(&shared2).await
            && let Ok(v) = shared2
                .rest
                .user_get(&token, &format!("/guilds/{guild_id}/channels"))
                .await
        {
            channels = Some(map_channels(&guild_id, &v));
        }
        let _ = shared2.app_tx.send(AppEvent::ChannelsLoaded {
            guild_id,
            channels: channels.unwrap_or_default(),
        });
    });
}

fn map_channels(guild_id: &str, v: &Value) -> Vec<ChannelEntry> {
    v.as_array().map_or_default(|a| {
        a.iter()
            .filter_map(|c| {
                Some(ChannelEntry {
                    id: c.get("id")?.as_str()?.to_string(),
                    name: c
                        .get("name")
                        .and_then(|x| x.as_str())
                        .unwrap_or("channel")
                        .to_string(),
                    guild_id: guild_id.to_string(),
                    kind: c
                        .get("type")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0) as u8,
                })
            })
            .collect()
    })
}

/// Start the gateway session if enabled and a bot token exists.
pub fn ensure_gateway(shared: &Arc<BackendShared>) {
    maybe_start_gateway(shared);
}

// -- Export file --------------------------------------------------------------

fn write_export(shared: &Arc<BackendShared>, name: &str, contents: String) {
    // Prefer the user-picked path if the app stored one; otherwise write to
    // the data dir.
    let pending = shared
        .export_path
        .lock()
        .unwrap()
        .take()
        .unwrap_or_else(|| {
            directories::ProjectDirs::from("dev", "l", "l")
                .map_or_else(|| PathBuf::from(name), |d| d.data_dir().join(name))
        });
    let _ = std::fs::write(&pending, contents);
    let _ = shared.app_tx.send(AppEvent::ServiceToast {
        text: format!("exported to {}", pending.display()),
        kind: "success".into(),
    });
}
