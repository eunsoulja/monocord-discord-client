//! Versioned, forward-only migrations.

use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};

/// Current schema version (informational; `rusqlite_migration` tracks it).
pub const SCHEMA_VERSION: i64 = 1;

fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(
        r"
        CREATE TABLE preferences (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE layouts (
            id         TEXT PRIMARY KEY,
            name       TEXT NOT NULL,
            is_default INTEGER NOT NULL DEFAULT 0,
            preset     TEXT,
            main_cols  INTEGER NOT NULL DEFAULT 12,
            dock_cols  INTEGER NOT NULL DEFAULT 4,
            payload    TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE notes (
            id         TEXT PRIMARY KEY,
            title      TEXT NOT NULL,
            body       TEXT NOT NULL,
            workspace  TEXT,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE notifications (
            id         TEXT PRIMARY KEY,
            source     TEXT NOT NULL,
            category   TEXT NOT NULL,
            title      TEXT NOT NULL,
            body       TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            read       INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX idx_notifications_created ON notifications(created_at DESC);

        CREATE TABLE audit_log (
            id      INTEGER PRIMARY KEY AUTOINCREMENT,
            at      INTEGER NOT NULL,
            actor   TEXT NOT NULL,
            action  TEXT NOT NULL,
            target  TEXT NOT NULL,
            ok      INTEGER NOT NULL
        );

        CREATE TABLE integrations (
            id         TEXT PRIMARY KEY,
            kind       TEXT NOT NULL,
            metadata   TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE widget_instances (
            id         TEXT PRIMARY KEY,
            widget_type TEXT NOT NULL,
            surface    TEXT NOT NULL,
            layout_id  TEXT,
            payload    TEXT NOT NULL,
            enabled    INTEGER NOT NULL DEFAULT 1,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE uptime_checks (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            service     TEXT NOT NULL,
            url         TEXT NOT NULL,
            checked_at  INTEGER NOT NULL,
            ok          INTEGER NOT NULL,
            status_code INTEGER,
            latency_ms  INTEGER
        );
        CREATE INDEX idx_uptime_service ON uptime_checks(service, checked_at DESC);
        ",
    )])
}

/// Run pending migrations inside a transaction. Refuses newer schemas.
pub fn run(conn: &mut Connection) -> Result<(), String> {
    // Refuse databases created by a newer version. rusqlite_migration stores
    // its own version table; user_version mirrors our schema version.
    let current: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap_or(0);
    if current > SCHEMA_VERSION {
        return Err(format!(
            "database schema v{current} is newer than supported v{SCHEMA_VERSION}"
        ));
    }
    migrations()
        .to_latest(conn)
        .map_err(|e| format!("migration failed: {e}"))?;
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)
        .map_err(|e| e.to_string())?;
    Ok(())
}
