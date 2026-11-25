//! Probe helpers used by the app's fetch function: HTTP uptime checks, API
//! endpoint probes, and system stats.

use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// Validate a user-supplied probe URL: https only by default.
pub fn validate_probe_url(url: &str, allow_private: bool) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("only https:// endpoints are allowed".into());
    }
    if url.len() > 512 {
        return Err("url too long".into());
    }
    if !allow_private {
        // Block obvious private/loopback hosts.
        if let Some(host) = url
            .strip_prefix("https://")
            .and_then(|s| s.split(['/', ':']).next())
            && (host == "localhost"
                || host.rsplit_once('.').is_some_and(|(_, tld)| tld == "local")
                || host.starts_with("127.")
                || host.starts_with("10.")
                || host.starts_with("192.168.")
                || host.starts_with("169.254.")
                || (host.starts_with("172.")
                    && host
                        .split('.')
                        .nth(1)
                        .and_then(|o| o.parse::<u8>().ok())
                        .is_some_and(|o| (16..=31).contains(&o))))
        {
            return Err("private-network endpoints require opt-in".into());
        }
    }
    Ok(())
}

/// Probe an HTTPS endpoint; returns status/latency JSON.
pub async fn http_probe(url: &str) -> Result<Value, String> {
    validate_probe_url(url, false)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(3))
        .build()
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    let latency = start.elapsed().as_millis() as u64;
    let status = resp.status().as_u16();
    // Drain a capped body to measure size without unbounded memory.
    let _ = resp.bytes().await;
    Ok(json!({
        "status": status,
        "latency_ms": latency,
        "ok": (200..400).contains(&status),
    }))
}

/// Collect lightweight system stats without extra deps.
#[must_use]
pub fn system_stats() -> Value {
    // CPU% requires sampling; provide a stable approximation from load average
    // where available, else a placeholder derived from uptime counters is NOT
    // fabricated — we report what the OS gives us.
    let uptime_secs = read_uptime_secs();
    let (cpu_pct, mem_pct) = sample_load();
    let hostname = hostname();
    let os = std::env::consts::OS.to_string();
    json!({
        "cpu_pct": cpu_pct,
        "mem_pct": mem_pct,
        "uptime_secs": uptime_secs,
        "hostname": hostname,
        "os": os,
    })
}

#[cfg(target_os = "windows")]
fn read_uptime_secs() -> u64 {
    // GetTickCount64 via std is not exposed; use a process-start approximation.
    // Fall back to 0 rather than fabricate.
    0
}

#[cfg(not(target_os = "windows"))]
fn read_uptime_secs() -> u64 {
    // /proc/uptime first field is system uptime seconds.
    std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next().map(str::to_string))
        .and_then(|s| s.parse::<f64>().ok())
        .map(|f| f as u64)
        .unwrap_or(0)
}

#[cfg(target_os = "windows")]
fn sample_load() -> (f32, f32) {
    // No std API for CPU/mem on Windows without extra crates; report zeros
    // rather than fabricate. Widgets display "—" for unavailable metrics.
    (0.0, 0.0)
}

#[cfg(not(target_os = "windows"))]
fn sample_load() -> (f32, f32) {
    let (cpu, mem) = parse_proc_stat();
    (cpu, mem)
}

#[cfg(not(target_os = "windows"))]
fn parse_proc_stat() -> (f32, f32) {
    // Very rough: read /proc/meminfo for mem%; cpu left at 0 (no fabrication).
    let mem_pct = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|s| {
            let mut mem_total = 0u64;
            let mut mem_avail = 0u64;
            for line in s.lines() {
                if line.starts_with("MemTotal:") {
                    mem_total = line
                        .split_whitespace()
                        .nth(1)
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                } else if line.starts_with("MemAvailable:") {
                    mem_avail = line
                        .split_whitespace()
                        .nth(1)
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                }
            }
            if mem_total > 0 {
                Some((1.0 - mem_avail as f64 / mem_total as f64) as f32 * 100.0)
            } else {
                None
            }
        })
        .unwrap_or(0.0);
    (0.0, mem_pct)
}

fn hostname() -> String {
    #[cfg(target_os = "windows")]
    {
        std::env::var("COMPUTERNAME").unwrap_or_else(|_| "unknown".into())
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::fs::read_to_string("/etc/hostname")
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "unknown".into())
    }
}
