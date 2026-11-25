//! Demo data provider: clearly labeled sample data behind the same trait
//! surface as the real Discord backend. Never calls Discord, never mutates
//! real resources.

use serde_json::{Value, json};

/// Deterministic-ish demo backend producing widget payloads.
pub struct DemoBackend;

impl DemoBackend {
    /// Fetch a demo payload for a data need key (e.g. `discord.guild_stats.X`).
    #[must_use]
    pub fn fetch(&self, need_key: &str) -> Value {
        if need_key.starts_with("discord.guild_stats") {
            guild_stats()
        } else if need_key.starts_with("discord.guild_activity") {
            guild_activity()
        } else if need_key.starts_with("discord.guild_voice") {
            guild_voice()
        } else if need_key == "discord.bot_status" {
            bot_status()
        } else if need_key == "system.stats" {
            system_stats()
        } else if need_key.starts_with("uptime.services") {
            uptime()
        } else if need_key.starts_with("api.probes") {
            api_probes()
        } else if need_key.starts_with("discord.account") {
            account()
        } else if need_key.starts_with("discord.guilds") {
            guilds()
        } else if need_key.starts_with("local.notes") {
            notes()
        } else if need_key.starts_with("local.notifications") {
            notifications()
        } else {
            Value::Null
        }
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn guild_stats() -> Value {
    json!({
        "name": "DEMO GUILD",
        "member_count": 1284,
        "online_count": 312,
        "channel_count": 24,
        "role_count": 18,
        "created_at": 1_700_000_000i64,
        "approximate": true
    })
}

fn guild_activity() -> Value {
    json!({
        "members_available": true,
        "events": [
            {"kind":"join","actor":"demo_user_1","ts":now()-95,"detail":null},
            {"kind":"join","actor":"demo_user_2","ts":now()-340,"detail":null},
            {"kind":"boost","actor":"demo_boost","ts":now()-1500,"detail":"server boosted"},
            {"kind":"leave","actor":"demo_user_3","ts":now()-4200,"detail":null},
        ]
    })
}

fn guild_voice() -> Value {
    json!({
        "available": true,
        "channels": [
            {"name":"General","participants":3},
            {"name":"AFK","participants":1},
        ]
    })
}

fn bot_status() -> Value {
    json!({
        "status":"online",
        "username":"L-DemoBot",
        "latency_ms":42,
        "gateway":"READY",
        "error":null
    })
}

fn system_stats() -> Value {
    json!({
        "cpu_pct": 12.4,
        "mem_pct": 47.0,
        "uptime_secs": 86400 * 3 + 7200,
        "hostname":"demo-host",
        "os":"demo-os"
    })
}

fn uptime() -> Value {
    json!({
        "services": [
            {"name":"api.example.com","url":"https://api.example.com","uptime_pct":99.7,"last_check":now()-30,"last_ok":true,"last_status":200,"last_latency_ms":84},
            {"name":"status.example.com","url":"https://status.example.com","uptime_pct":98.1,"last_check":now()-45,"last_ok":false,"last_status":503,"last_latency_ms":1200},
        ]
    })
}

fn api_probes() -> Value {
    json!({
        "results": [
            {"name":"discord","url":"https://discord.com/api/v10/gateway","status":200,"latency_ms":120,"last_ok":now()-20,"ok":true},
            {"name":"example","url":"https://example.com","status":200,"latency_ms":95,"last_ok":now()-25,"ok":true},
        ]
    })
}

fn account() -> Value {
    json!({
        "id":"demo","username":"demo_user","global_name":"Demo User","avatar":null
    })
}

fn guilds() -> Value {
    json!([
        {"id":"g1","name":"Demo Guild One","icon":null,"owner":true,"permissions":"8","bot_present":true,"member_count":1284,"online_count":312},
        {"id":"g2","name":"Demo Guild Two","icon":null,"owner":false,"permissions":"32","bot_present":false,"member_count":560,"online_count":98},
    ])
}

fn notes() -> Value {
    json!({
        "notes": [
            {"id":"00000000-0000-0000-0000-000000000001","title":"scratch","body":"Demo note — real notes persist locally.","workspace":null,"updated_at":now()},
        ]
    })
}

fn notifications() -> Value {
    json!({
        "unread":2,
        "notifications": [
            {"id":"00000000-0000-0000-0000-0000000000a1","source":"system","category":"system","title":"Demo mode active","body":"Sample data is being shown; no Discord calls are made.","created_at":now()-600,"read":false},
            {"id":"00000000-0000-0000-0000-0000000000a2","source":"discord","category":"discord","title":"Demo event","body":"Member joined (demo).","created_at":now()-3600,"read":false},
        ]
    })
}
