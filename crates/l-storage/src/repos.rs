//! Repository functions. Each takes `&mut Connection` and is invoked through
//! `Db::call` so all SQLite access stays on the DB thread.

use std::time::{SystemTime, UNIX_EPOCH};

use l_core::models::{Layout, LayoutId, Note, Notification, NotificationId};
use l_core::state::{AuditEntry, Preferences};
use rusqlite::{Connection, OptionalExtension, params};

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn to_json<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "null".into())
}

fn from_json<T: serde::de::DeserializeOwned>(s: &str) -> Option<T> {
    serde_json::from_str(s).ok()
}

// ---------- preferences ----------

const PREFS_KEY: &str = "app";

/// Load persisted preferences.
pub fn load_preferences(conn: &mut Connection) -> Result<Option<Preferences>, String> {
    let v: Option<String> = conn
        .query_row(
            "SELECT value FROM preferences WHERE key = ?1",
            params![PREFS_KEY],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    Ok(v.and_then(|s| from_json(&s)))
}

/// Save the whole preferences blob.
pub fn save_preferences(conn: &mut Connection, prefs: &Preferences) -> Result<(), String> {
    conn.execute(
        "INSERT INTO preferences(key, value) VALUES(?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![PREFS_KEY, to_json(prefs)],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- layouts ----------

/// Load all layouts.
pub fn load_layouts(conn: &mut Connection) -> Result<Vec<Layout>, String> {
    let mut stmt = conn
        .prepare("SELECT payload FROM layouts ORDER BY updated_at")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let s = row.map_err(|e| e.to_string())?;
        if let Some(l) = from_json::<Layout>(&s) {
            out.push(l);
        }
    }
    Ok(out)
}

/// Upsert a layout.
pub fn save_layout(conn: &mut Connection, layout: &Layout) -> Result<(), String> {
    conn.execute(
        "INSERT INTO layouts(id, name, is_default, preset, main_cols, dock_cols, payload, updated_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
         ON CONFLICT(id) DO UPDATE SET name=excluded.name, is_default=excluded.is_default,
             preset=excluded.preset, main_cols=excluded.main_cols, dock_cols=excluded.dock_cols,
             payload=excluded.payload, updated_at=excluded.updated_at",
        params![
            layout.id.to_string(),
            layout.name,
            i64::from(layout.is_default),
            layout.preset,
            i64::from(layout.main_columns),
            i64::from(layout.dock_columns),
            to_json(layout),
            now()
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Delete a layout.
pub fn delete_layout(conn: &mut Connection, id: LayoutId) -> Result<(), String> {
    conn.execute("DELETE FROM layouts WHERE id = ?1", params![id.to_string()])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- notes ----------

/// Load all notes.
pub fn load_notes(conn: &mut Connection) -> Result<Vec<Note>, String> {
    let mut stmt = conn
        .prepare("SELECT id,title,body,workspace,updated_at FROM notes ORDER BY updated_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (id, title, body, ws, ts) = row.map_err(|e| e.to_string())?;
        out.push(Note {
            id: id.parse().unwrap_or_else(|_| uuid::Uuid::nil()),
            title,
            body,
            workspace: ws,
            updated_at: ts,
        });
    }
    Ok(out)
}

/// Upsert a note.
pub fn save_note(conn: &mut Connection, note: &Note) -> Result<(), String> {
    conn.execute(
        "INSERT INTO notes(id,title,body,workspace,updated_at) VALUES(?1,?2,?3,?4,?5)
         ON CONFLICT(id) DO UPDATE SET title=excluded.title, body=excluded.body,
             workspace=excluded.workspace, updated_at=excluded.updated_at",
        params![
            note.id.to_string(),
            note.title,
            note.body,
            note.workspace,
            note.updated_at
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Delete a note.
pub fn delete_note(conn: &mut Connection, id: uuid::Uuid) -> Result<(), String> {
    conn.execute("DELETE FROM notes WHERE id = ?1", params![id.to_string()])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- notifications ----------

/// Load all notifications (newest first).
pub fn load_notifications(conn: &mut Connection) -> Result<Vec<Notification>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id,source,category,title,body,created_at,read FROM notifications
             ORDER BY created_at DESC LIMIT 500",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (id, source, category, title, body, ts, read) = row.map_err(|e| e.to_string())?;
        out.push(Notification {
            id: id.parse().unwrap_or_else(|_| uuid::Uuid::nil()),
            source,
            category,
            title,
            body,
            created_at: ts,
            read: read != 0,
        });
    }
    Ok(out)
}

/// Upsert a notification.
pub fn save_notification(conn: &mut Connection, n: &Notification) -> Result<(), String> {
    conn.execute(
        "INSERT INTO notifications(id,source,category,title,body,created_at,read)
         VALUES(?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(id) DO UPDATE SET read=excluded.read",
        params![
            n.id.to_string(),
            n.source,
            n.category,
            n.title,
            n.body,
            n.created_at,
            i64::from(n.read)
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Delete a notification.
pub fn delete_notification(conn: &mut Connection, id: NotificationId) -> Result<(), String> {
    conn.execute(
        "DELETE FROM notifications WHERE id = ?1",
        params![id.to_string()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Clear all notifications.
pub fn clear_notifications(conn: &mut Connection) -> Result<(), String> {
    conn.execute("DELETE FROM notifications", [])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- audit log ----------

/// Append an audit entry.
pub fn append_audit(conn: &mut Connection, entry: &AuditEntry) -> Result<(), String> {
    conn.execute(
        "INSERT INTO audit_log(at,actor,action,target,ok) VALUES(?1,?2,?3,?4,?5)",
        params![
            entry.at,
            entry.actor,
            entry.action,
            entry.target,
            i64::from(entry.ok)
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Load recent audit entries (oldest first).
pub fn load_audit(conn: &mut Connection, limit: i64) -> Result<Vec<AuditEntry>, String> {
    let mut stmt = conn
        .prepare("SELECT at,actor,action,target,ok FROM audit_log ORDER BY id DESC LIMIT ?1")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![limit], |r| {
            Ok(AuditEntry {
                at: r.get(0)?,
                actor: r.get(1)?,
                action: r.get(2)?,
                target: r.get(3)?,
                ok: r.get::<_, i64>(4)? != 0,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut out: Vec<AuditEntry> = rows.collect::<Result<_, _>>().map_err(|e| e.to_string())?;
    out.reverse();
    Ok(out)
}

// ---------- uptime checks ----------

/// Record an uptime probe result.
pub fn record_uptime_check(
    conn: &mut Connection,
    service: &str,
    url: &str,
    ok: bool,
    status_code: Option<u16>,
    latency_ms: u64,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO uptime_checks(service,url,checked_at,ok,status_code,latency_ms)
         VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            service,
            url,
            now(),
            i64::from(ok),
            status_code.map(i64::from),
            latency_ms as i64
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// One uptime check row: (`checked_at`, `ok`, `status_code`, `latency_ms`).
pub type UptimeRow = (i64, bool, Option<u16>, i64);

/// Recent checks for a service.
pub fn uptime_history(
    conn: &mut Connection,
    service: &str,
    limit: i64,
) -> Result<Vec<UptimeRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT checked_at, ok, status_code, latency_ms FROM uptime_checks
             WHERE service = ?1 ORDER BY checked_at DESC LIMIT ?2",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![service, limit], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)? != 0,
                r.get::<_, Option<i64>>(2)?.map(|v| v as u16),
                r.get::<_, i64>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}
