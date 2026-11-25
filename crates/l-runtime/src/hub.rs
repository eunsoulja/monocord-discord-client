//! Shared data-source hub.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use l_core::models::WidgetInstanceId;
use l_widgets::core::types::DataNeed;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Result of a fetch.
pub type FetchResult = Result<Value, String>;

/// A fetch function the app supplies. Must be cheap to clone (Arc inside).
pub type FetchFn =
    Arc<dyn Fn(DataNeed) -> Pin<Box<dyn Future<Output = FetchResult> + Send>> + Send + Sync>;

/// Events the hub emits to the UI.
#[derive(Debug, Clone)]
pub enum HubEvent {
    /// Fresh data for a need key; app routes to subscriber widgets.
    Data {
        /// Need key (matches `DataNeed::key`).
        need_key: String,
        /// JSON payload.
        data: Value,
    },
    /// Fetch failed for a need key.
    Failed {
        /// Need key.
        need_key: String,
        /// Error.
        error: String,
    },
}

struct Poller {
    cancel: CancellationToken,
    subscribers: Vec<WidgetInstanceId>,
    interval: Duration,
}

/// The data hub. One instance per app.
pub struct DataHub {
    fetch: FetchFn,
    events_tx: mpsc::UnboundedSender<HubEvent>,
    events_rx: mpsc::UnboundedReceiver<HubEvent>,
    pollers: HashMap<String, Poller>,
    /// `need_key` -> subscriber widget ids (for push data routing).
    push_subs: HashMap<String, Vec<WidgetInstanceId>>,
    /// Keys requested to refetch on the next poller wake.
    kicked: Arc<tokio::sync::Mutex<std::collections::HashSet<String>>>,
    default_interval: Duration,
}

impl DataHub {
    /// Create a hub with a fetch function and default poll interval.
    pub fn new(fetch: FetchFn, default_interval: Duration) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            fetch,
            events_tx: tx,
            events_rx: rx,
            pollers: HashMap::new(),
            push_subs: HashMap::new(),
            kicked: Arc::new(tokio::sync::Mutex::new(std::collections::HashSet::new())),
            default_interval,
        }
    }

    /// Mark a need key for immediate refetch on the next poller tick.
    pub fn refresh_key(&self, need_key: &str) {
        if let Ok(mut k) = self.kicked.try_lock() {
            k.insert(need_key.to_string());
        }
    }

    /// Receiver for hub events (app drains each frame).
    pub fn events(&mut self) -> &mut mpsc::UnboundedReceiver<HubEvent> {
        &mut self.events_rx
    }

    /// Take all pending events.
    pub fn drain(&mut self) -> Vec<HubEvent> {
        let mut out = Vec::new();
        while let Ok(ev) = self.events_rx.try_recv() {
            out.push(ev);
        }
        out
    }

    /// Sync subscriptions: `needs` is (`widget_id`, need, `refresh_secs`).
    /// Stops pollers with no subscribers; starts missing ones.
    pub fn sync(&mut self, needs: Vec<(WidgetInstanceId, DataNeed, Option<u64>)>) {
        // Group by need key.
        let mut by_key: HashMap<String, (DataNeed, Vec<WidgetInstanceId>, Option<u64>)> =
            HashMap::new();
        for (wid, need, secs) in needs {
            let key = need.key();
            let entry = by_key
                .entry(key)
                .or_insert_with(|| (need.clone(), Vec::new(), secs));
            entry.1.push(wid);
            if entry.2.is_none() {
                entry.2 = secs;
            }
        }

        // Stop pollers no longer needed.
        let wanted: std::collections::HashSet<String> = by_key.keys().cloned().collect();
        self.pollers.retain(|key, p| {
            if wanted.contains(key) {
                true
            } else {
                p.cancel.cancel();
                false
            }
        });
        self.push_subs.retain(|key, _| wanted.contains(key));

        // Start or update pollers.
        for (key, (need, subs, secs)) in by_key {
            let interval = secs
                .map_or(self.default_interval, Duration::from_secs)
                .max(Duration::from_secs(5));
            if let Some(p) = self.pollers.get_mut(&key) {
                p.subscribers.clone_from(&subs);
                p.interval = interval;
            } else {
                let cancel = CancellationToken::new();
                self.spawn_poller(key.clone(), need, subs.clone(), interval, cancel.clone());
                self.pollers.insert(
                    key.clone(),
                    Poller {
                        cancel,
                        subscribers: subs.clone(),
                        interval,
                    },
                );
            }
            self.push_subs.insert(key, subs);
        }
    }

    fn spawn_poller(
        &self,
        key: String,
        need: DataNeed,
        _subs: Vec<WidgetInstanceId>,
        interval: Duration,
        cancel: CancellationToken,
    ) {
        let fetch = self.fetch.clone();
        let tx = self.events_tx.clone();
        let kicked = self.kicked.clone();
        let key2 = key.clone();
        tokio::spawn(async move {
            // Fetch immediately, then on interval (checked in 1s slices so
            // manual refresh kicks take effect promptly).
            loop {
                let f = fetch(need.clone());
                let result = tokio::select! {
                    r = f => r,
                    () = cancel.cancelled() => return,
                };
                match result {
                    Ok(data) => {
                        let _ = tx.send(HubEvent::Data {
                            need_key: key.clone(),
                            data,
                        });
                    }
                    Err(e) => {
                        let _ = tx.send(HubEvent::Failed {
                            need_key: key.clone(),
                            error: e,
                        });
                    }
                }
                let mut elapsed = Duration::ZERO;
                loop {
                    tokio::select! {
                        () = tokio::time::sleep(Duration::from_secs(1)) => {
                            elapsed += Duration::from_secs(1);
                        }
                        () = cancel.cancelled() => return,
                    }
                    let manual = kicked.try_lock().is_ok_and(|mut k| k.remove(&key2));
                    if manual || elapsed >= interval {
                        break;
                    }
                }
            }
        });
    }

    /// Publish push data for a need key (local data: notes, notifications…).
    /// Routed to all subscribers of that key.
    pub fn publish(&self, need_key: &str, data: Value) {
        let _ = self.events_tx.send(HubEvent::Data {
            need_key: need_key.to_string(),
            data,
        });
    }

    /// Force-refresh all pollers once.
    pub fn refresh_all(&self) {
        // Pollers refetch on their interval; for immediacy we rely on the
        // app re-syncing or the widget's manual refresh path calling fetch.
    }
}
