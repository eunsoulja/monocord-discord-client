//! Local Discord RPC (rich presence) server.
//!
//! Implements the Discord IPC protocol used by games and tools
//! (`discord-rpc` / arRPC style): clients connect to a local endpoint
//! (`\\.\pipe\discord-ipc-N` on Windows, `/tmp/discord-ipc-N` elsewhere),
//! perform a handshake, then send `SET_ACTIVITY` / `CLEAR_ACTIVITY`
//! commands. Activities are merged and published to Discord through the
//! bot gateway as a `PRESENCE_UPDATE`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::backend::BackendShared;

/// Max clients tracked simultaneously.
const MAX_FRAME: usize = 1024 * 1024;

/// Spawn the IPC accept loop. Binds the first free `discord-ipc-N` endpoint
/// (N in 0..10), matching what Discord clients probe for.
pub fn spawn(shared: &Arc<BackendShared>) {
    let st = shared.clone();
    shared.tokio.spawn(async move {
        #[cfg(windows)]
        run_windows(st).await;
        #[cfg(not(windows))]
        run_unix(st).await;
    });
}

#[cfg(windows)]
async fn run_windows(shared: Arc<BackendShared>) {
    use tokio::net::windows::named_pipe::ServerOptions;
    let st = RpcState::new(shared);
    for i in 0..10u32 {
        let path = format!(r"\\.\pipe\discord-ipc-{i}");
        let Ok(mut pipe) = ServerOptions::new()
            .first_pipe_instance(true)
            .create(&path)
        else {
            continue;
        };
        loop {
            if pipe.connect().await.is_err() {
                break;
            }
            let _ = handle_conn(&mut pipe, &st).await;
            let _ = pipe.disconnect();
        }
    }
}

#[cfg(not(windows))]
async fn run_unix(shared: Arc<BackendShared>) {
    let st = RpcState::new(shared);
    for i in 0..10u32 {
        let path = format!("/tmp/discord-ipc-{i}");
        let Ok(listener) = tokio::net::UnixListener::bind(&path) else {
            continue;
        };
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let st = st.clone();
            tokio::spawn(async move {
                let _ = handle_conn(&mut stream, &st).await;
            });
        }
    }
}

/// Per-server state: which `client_id` currently owns which activity.
struct RpcState {
    shared: Arc<BackendShared>,
    /// `client_id` -> activity object (last `SET_ACTIVITY` wins per client).
    activities: Mutex<HashMap<String, Value>>,
}

impl RpcState {
    fn new(shared: Arc<BackendShared>) -> Arc<Self> {
        Arc::new(Self {
            shared,
            activities: Mutex::new(HashMap::new()),
        })
    }
}

/// Merge all client activities and push them to the gateway.
fn publish(st: &RpcState) {
    let list: Vec<Value> = st
        .activities
        .lock()
        .unwrap()
        .values()
        .filter(|v| !v.is_null())
        .cloned()
        .collect();
    let value = if list.is_empty() {
        None
    } else {
        Some(Value::Array(list))
    };
    let gw = st.shared.gw();
    *gw.activity.lock().unwrap() = value;
    gw.activity_notify.notify_one();
}

/// Serve one client connection until it closes.
async fn handle_conn<S>(stream: &mut S, st: &Arc<RpcState>) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut handshaken = false;
    loop {
        let mut header = [0u8; 8];
        if stream.read_exact(&mut header).await.is_err() {
            return Ok(());
        }
        let op = u32::from_le_bytes(header[0..4].try_into().unwrap());
        let len = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
        if len > MAX_FRAME {
            return Ok(());
        }
        let mut buf = vec![0u8; len];
        if stream.read_exact(&mut buf).await.is_err() {
            return Ok(());
        }
        let Ok(v) = serde_json::from_slice::<Value>(&buf) else {
            continue;
        };
        match op {
            0 => {
                // Handshake.
                handshaken = true;
                let resp = json!({
                    "v": 1,
                    "config": {
                        "api_endpoint": "http://localhost",
                        "env": { "L_RPC": "1" }
                    }
                });
                write_frame(stream, 1, &resp).await?;
            }
            1 => {
                if !handshaken {
                    continue;
                }
                let cmd = v.get("cmd").and_then(Value::as_str).unwrap_or("");
                let client_id = v
                    .get("client_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let nonce = v.get("nonce").cloned().unwrap_or(Value::Null);
                let mut data = json!({});
                match cmd {
                    "SET_ACTIVITY" => {
                        if st.shared.rpc_enabled.load(std::sync::atomic::Ordering::Relaxed) {
                            let activity = v
                                .get("args")
                                .and_then(|a| a.get("activity"))
                                .cloned()
                                .unwrap_or(Value::Null);
                            if activity.is_null() {
                                st.activities.lock().unwrap().remove(&client_id);
                            } else {
                                st.activities
                                    .lock()
                                    .unwrap()
                                    .insert(client_id.clone(), activity);
                            }
                            publish(st);
                        }
                    }
                    "CLEAR_ACTIVITY" => {
                        st.activities.lock().unwrap().remove(&client_id);
                        publish(st);
                    }
                    "GET_ACTIVITY" => {
                        data = st
                            .activities
                            .lock()
                            .unwrap()
                            .get(&client_id)
                            .cloned()
                            .unwrap_or(json!({}));
                    }
                    // Subscription/settings commands and anything else:
                    // acknowledge with empty data.
                    _ => {}
                }
                let resp = json!({
                    "v": 1,
                    "client_id": client_id,
                    "nonce": nonce,
                    "cmd": cmd,
                    "data": data,
                });
                write_frame(stream, 1, &resp).await?;
            }
            2 => return Ok(()), // Close.
            3 => {
                // Ping -> Pong.
                write_frame(stream, 4, &Value::Null).await?;
            }
            _ => {}
        }
    }
}

async fn write_frame<S>(stream: &mut S, op: u32, payload: &Value) -> std::io::Result<()>
where
    S: AsyncWrite + Unpin,
{
    let body = serde_json::to_vec(payload).unwrap_or_else(|_| b"{}".to_vec());
    let mut frame = Vec::with_capacity(8 + body.len());
    frame.extend_from_slice(&op.to_le_bytes());
    frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
    frame.extend_from_slice(&body);
    stream.write_all(&frame).await
}
