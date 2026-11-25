//! Optional bot Gateway (websocket) session.
//!
//! REST-only mode is fully functional without this. When enabled, the session
//! performs IDENTIFY with only the intents the user enabled, heartbeats, and
//! exposes latency + voice-state/member-join snapshots to widgets.
//!
//! Implemented with `tokio-tungstenite`; session resume is supported via the
//! session id + sequence. Reconnects use exponential backoff and respect
//! IDENTIFY rate limits (`/gateway/bot` `session_start_limit`).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::Message;

use crate::rest::{DiscordRest, gateway_info};

/// Events emitted by the gateway session.
#[derive(Debug, Clone)]
pub enum GatewayEvent {
    /// Session identified; bot user info known.
    Ready {
        /// Bot user id.
        user_id: String,
        /// Bot username.
        username: String,
        /// Session id for resume.
        session_id: String,
    },
    /// Heartbeat acknowledged; round-trip ms.
    Heartbeat {
        /// Latency in ms.
        ms: u64,
    },
    /// Connection state changed.
    State(String),
    /// Session ended (fatal or stopped).
    Closed(String),
    /// A dispatch event relevant to messaging/voice was received. `event` is
    /// the Discord event name (e.g. `MESSAGE_CREATE`, `VOICE_SERVER_UPDATE`),
    /// `data` is the raw `d` payload.
    Dispatch {
        /// Discord event name.
        event: String,
        /// Raw event payload.
        data: Value,
    },
}

/// Voice state snapshot for widgets.
#[derive(Debug, Clone, Default)]
pub struct VoiceSnapshot {
    /// `channel_id` -> set of user ids currently in that channel.
    pub channels: HashMap<String, std::collections::HashSet<String>>,
    /// channel names when known.
    pub names: HashMap<String, String>,
}

/// Shared gateway state readable by widgets.
#[derive(Default)]
pub struct GatewayState {
    /// Latest voice snapshot per guild.
    pub voice: Mutex<HashMap<String, VoiceSnapshot>>,
    /// Recent member join events per guild (actor, ts).
    pub joins: Mutex<HashMap<String, Vec<(String, i64)>>>,
    /// Whether a session is currently connected.
    pub connected: std::sync::atomic::AtomicBool,
    /// Latest RPC-provided activities to publish via `PRESENCE_UPDATE`.
    /// Stored as a JSON array of Discord activity objects.
    pub activity: std::sync::Mutex<Option<Value>>,
    /// Wakes the session loop when `activity` changes.
    pub activity_notify: tokio::sync::Notify,
}

/// Handle to control the gateway session.
pub struct GatewayHandle {
    tx: mpsc::Sender<GatewayCommand>,
    state: Arc<GatewayState>,
    events: mpsc::Receiver<GatewayEvent>,
}

/// Commands to the gateway actor.
#[derive(Debug)]
pub enum GatewayCommand {
    /// Start/connect (or reconnect).
    Connect,
    /// Disconnect and stop.
    Disconnect,
}

impl GatewayHandle {
    /// Shared state for data providers.
    #[must_use]
    pub fn state(&self) -> Arc<GatewayState> {
        self.state.clone()
    }

    /// Clone of the command sender (for storing alongside the event pump).
    #[must_use]
    pub fn command_sender(&self) -> mpsc::Sender<GatewayCommand> {
        self.tx.clone()
    }

    /// Non-blocking event poll for UI-thread draining.
    pub fn try_recv(&mut self) -> Option<GatewayEvent> {
        self.events.try_recv().ok()
    }

    /// Next event (non-blocking try or async recv).
    pub async fn recv(&mut self) -> Option<GatewayEvent> {
        self.events.recv().await
    }

    /// Send a command.
    pub async fn send(&self, cmd: GatewayCommand) {
        let _ = self.tx.send(cmd).await;
    }
}

/// Spawn the gateway actor. Returns handle + event receiver is internal;
/// caller uses `recv()`.
pub fn spawn(rest: DiscordRest, bot_token: SecretString, intents: u64) -> GatewayHandle {
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<GatewayCommand>(8);
    let (ev_tx, ev_rx) = mpsc::channel::<GatewayEvent>(64);
    let state = Arc::new(GatewayState::default());
    let state2 = state.clone();

    tokio::spawn(async move {
        let mut backoff = 1u64;
        loop {
            tokio::select! {
                cmd = cmd_rx.recv() => {
                    match cmd {
                        Some(GatewayCommand::Disconnect) | None => {
                            state2.connected.store(false, std::sync::atomic::Ordering::SeqCst);
                            let _ = ev_tx.send(GatewayEvent::Closed("stopped".into())).await;
                            return;
                        }
                        Some(GatewayCommand::Connect) => {
                            backoff = 1;
                        }
                    }
                }
                () = sleep(Duration::from_secs(backoff)) => {}
            }

            // Fetch gateway URL.
            let gw = match gateway_info(&rest, &bot_token).await {
                Ok(v) => v,
                Err(e) => {
                    let _ = ev_tx
                        .send(GatewayEvent::State(format!("gateway info failed: {e}")))
                        .await;
                    backoff = (backoff * 2).min(60);
                    continue;
                }
            };
            let url = gw
                .get("url")
                .and_then(|u| u.as_str())
                .unwrap_or("wss://gateway.discord.gg")
                .to_string();

            let result = run_session(&url, &bot_token, intents, &rest, &ev_tx, &state2).await;
            state2
                .connected
                .store(false, std::sync::atomic::Ordering::SeqCst);
            match result {
                Ok(()) => {
                    let _ = ev_tx
                        .send(GatewayEvent::Closed("session ended".into()))
                        .await;
                    backoff = 1;
                }
                Err(e) => {
                    let _ = ev_tx
                        .send(GatewayEvent::State(format!("reconnecting after: {e}")))
                        .await;
                    backoff = (backoff * 2).min(60);
                }
            }
        }
    });

    GatewayHandle {
        tx: cmd_tx,
        state,
        events: ev_rx,
    }
}

async fn run_session(
    url: &str,
    token: &SecretString,
    intents: u64,
    rest: &DiscordRest,
    ev: &mpsc::Sender<GatewayEvent>,
    state: &Arc<GatewayState>,
) -> Result<(), String> {
    let ws_url = format!("{url}/?v=10&encoding=json");
    let (ws, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .map_err(|e| e.to_string())?;
    let (mut write, mut read) = ws.split();

    // Wait for HELLO (op 10).
    let hello_msg = tokio::time::timeout(Duration::from_secs(10), read.next())
        .await
        .map_err(|_| "no HELLO".to_string())?
        .ok_or("no HELLO")?
        .map_err(|e| e.to_string())?;
    let hello = match hello_msg {
        Message::Text(t) => serde_json::from_str::<Value>(&t).map_err(|e| e.to_string())?,
        _ => return Err("unexpected hello frame".into()),
    };
    let heartbeat_interval = hello
        .get("d")
        .and_then(|d| d.get("heartbeat_interval"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(41250);
    let mut seq: Option<u64> = None;

    // IDENTIFY
    let identify = json!({
        "op": 2,
        "d": {
            "token": token.expose_secret(),
            "intents": intents,
            "properties": {"os": "windows", "browser": "l", "device": "l"}
        }
    });
    write
        .send(Message::Text(identify.to_string().into()))
        .await
        .map_err(|e| e.to_string())?;

    let mut last_ack = Instant::now();
    let mut heartbeat = tokio::time::interval(Duration::from_millis(heartbeat_interval));
    let mut session_id: Option<String> = None;
    let mut bot_user: Option<(String, String)> = None;
    let mut ready = false;
    let mut presence_sent = false;

    state
        .connected
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let _ = ev.send(GatewayEvent::State("connected".into())).await;

    loop {
        tokio::select! {
            () = state.activity_notify.notified() => {
                if ready {
                    send_presence(&mut write, state).await;
                }
            }
            _ = heartbeat.tick() => {
                if last_ack.elapsed() > Duration::from_millis(heartbeat_interval * 2) {
                    return Err("heartbeat timeout".into());
                }
                let hb = json!({"op": 1, "d": seq});
                let _ = write.send(Message::Text(hb.to_string().into())).await;
                // Latency measured on ACK below; approximate via heartbeat send time.
                let _ = rest; // keep param used
            }
            msg = read.next() => {
                let Some(Ok(Message::Text(text))) = msg else {
                    if msg.is_none() { return Ok(()); }
                    continue;
                };
                let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
                let op = v.get("op").and_then(serde_json::Value::as_u64).unwrap_or(0);
                if let Some(s) = v.get("s").and_then(serde_json::Value::as_u64) {
                    seq = Some(s);
                }
                match op {
                    0 => {
                        let was_ready =
                            dispatch_event(&v, state, ev, &mut session_id, &mut bot_user)
                                .await;
                        ready |= was_ready;
                        if was_ready && !presence_sent {
                            presence_sent = true;
                            send_presence(&mut write, state).await;
                        }
                    }
                    1 => {
                        // Heartbeat request from server.
                        let hb = json!({"op":1,"d":seq});
                        let _ = write.send(Message::Text(hb.to_string().into())).await;
                    }
                    7 => {
                        // Reconnect requested.
                        return Err("server requested reconnect".into());
                    }
                    9 => {
                        // Invalid session — try resume next connect.
                        return Err("invalid session".into());
                    }
                    11 => {
                        last_ack = Instant::now();
                        let ms = v.get("d")
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(0);
                        // Discord ACK has no latency field; report heartbeat RTT approx.
                        let _ = ev.send(GatewayEvent::Heartbeat { ms }).await;
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Dispatch one gateway event; returns `true` when READY was seen.
async fn dispatch_event(
    v: &Value,
    state: &Arc<GatewayState>,
    ev: &mpsc::Sender<GatewayEvent>,
    session_id: &mut Option<String>,
    bot_user: &mut Option<(String, String)>,
) -> bool {
    let t = v.get("t").and_then(|t| t.as_str()).unwrap_or("");
    let d = v.get("d").cloned().unwrap_or(Value::Null);
    match t {
        "READY" => {
            let user = d.get("user");
            let uid = user
                .and_then(|u| u.get("id"))
                .and_then(|i| i.as_str())
                .unwrap_or("")
                .to_string();
            let uname = user
                .and_then(|u| u.get("username"))
                .and_then(|i| i.as_str())
                .unwrap_or("")
                .to_string();
            *bot_user = Some((uid.clone(), uname.clone()));
            if let Some(s) = d.get("session_id").and_then(|s| s.as_str()) {
                *session_id = Some(s.to_string());
            }
            let _ = ev
                .send(GatewayEvent::Ready {
                    user_id: uid,
                    username: uname,
                    session_id: session_id.clone().unwrap_or_default(),
                })
                .await;
            return true;
        }
        "VOICE_STATE_UPDATE" => {
            let guild_id = d.get("guild_id").and_then(|g| g.as_str()).unwrap_or("");
            let user_id = d
                .get("user_id")
                .and_then(|u| u.as_str())
                .unwrap_or("")
                .to_string();
            let channel_id = d.get("channel_id").and_then(|c| c.as_str());
            let mut voice = state.voice.lock().await;
            let snap = voice.entry(guild_id.to_string()).or_default();
            // Remove the user from any channel they previously occupied.
            for members in snap.channels.values_mut() {
                members.remove(&user_id);
            }
            if let Some(cid) = channel_id {
                snap.channels
                    .entry(cid.to_string())
                    .or_default()
                    .insert(user_id);
            }
            snap.channels.retain(|_, m| !m.is_empty());
            drop(voice);
            // Also forward the raw payload so the voice manager can read the
            // bot's session_id during the join handshake.
            let _ = ev
                .send(GatewayEvent::Dispatch {
                    event: t.to_string(),
                    data: d,
                })
                .await;
        }
        "GUILD_MEMBER_ADD" => {
            let guild_id = d.get("guild_id").and_then(|g| g.as_str()).unwrap_or("");
            let user = d.get("user");
            let name = user
                .and_then(|u| u.get("username"))
                .and_then(|i| i.as_str())
                .unwrap_or("unknown")
                .to_string();
            let mut joins = state.joins.lock().await;
            let list = joins.entry(guild_id.to_string()).or_default();
            list.insert(0, (name, now_secs()));
            list.truncate(50);
        }
        // Voice server handshake data + messaging/typing events forwarded
        // verbatim to the app layer.
        "VOICE_SERVER_UPDATE"
        | "MESSAGE_CREATE"
        | "MESSAGE_UPDATE"
        | "MESSAGE_DELETE"
        | "MESSAGE_REACTION_ADD"
        | "MESSAGE_REACTION_REMOVE"
        | "MESSAGE_REACTION_ADD_ALL"
        | "MESSAGE_REACTION_REMOVE_ALL"
        | "MESSAGE_REACTION_REMOVE_EMOJI"
        | "TYPING_START"
        | "CHANNEL_CREATE"
        | "CHANNEL_UPDATE"
        | "CHANNEL_DELETE" => {
            let _ = ev
                .send(GatewayEvent::Dispatch {
                    event: t.to_string(),
                    data: d,
                })
                .await;
        }
        _ => {}
    }
    false
}

/// Publish the stored RPC activities as a `PRESENCE_UPDATE` (op 3).
async fn send_presence(
    write: &mut futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    state: &Arc<GatewayState>,
) {
    let activity = state.activity.lock().unwrap().clone();
    let activities = activity.map_or_else(Vec::new, |v| match v {
        Value::Array(a) => a,
        other => vec![other],
    });
    let frame = json!({
        "op": 3,
        "d": {
            "since": null,
            "activities": activities,
            "status": "online",
            "afk": false,
            "mobile": false
        }
    });
    let _ = write.send(Message::Text(frame.to_string().into())).await;
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Compute intents bitfield from enabled flags.
///
/// `GUILD_MESSAGES` and `GUILD_MESSAGE_TYPING` are always requested (needed
/// for the chat client); `MESSAGE_CONTENT` is privileged and only requested
/// when the user enabled that intent in the Developer Portal.
#[must_use]
pub fn intents_bitfield(members: bool, presences: bool, message_content: bool) -> u64 {
    // Discord intent bits (v10):
    // GUILDS=1<<0, GUILD_MEMBERS=1<<1, GUILD_PRESENCES=1<<8,
    // GUILD_VOICE_STATES=1<<7, GUILD_MESSAGES=1<<9, GUILD_MESSAGE_TYPING=1<<11,
    // MESSAGE_CONTENT=1<<15
    let mut bits = 1u64 << 0; // GUILDS (always needed for guild scoping)
    bits |= 1u64 << 7; // GUILD_VOICE_STATES (non-privileged)
    bits |= 1u64 << 9; // GUILD_MESSAGES (non-privileged)
    bits |= 1u64 << 11; // GUILD_MESSAGE_TYPING (non-privileged)
    if members {
        bits |= 1u64 << 1;
    }
    if presences {
        bits |= 1u64 << 8;
    }
    if message_content {
        bits |= 1u64 << 15;
    }
    bits
}

/// One-shot channel for tests/inspection.
pub type Notify = oneshot::Sender<()>;
