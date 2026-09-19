//! Cursor / Antigravity account quota snapshots (not session token counts).
//!
//! Routes follow the same ideas as Pulse / OpenUsage / CodexBar:
//! - Cursor: editor `state.vscdb` token → cookie → `cursor.com/api/usage-summary`
//! - Antigravity: local language_server RPC, then Cloud Code OAuth fallback

mod antigravity;
mod cursor;

use serde::Serialize;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize)]
pub struct QuotaWindow {
    pub id: String,
    pub kind: String,
    pub scope: Option<String>,
    /// 0..=1 used share (never invent when missing — omit the window instead)
    pub used_fraction: f64,
    /// Display percent 0..=100 (anything used rounds to at least 1)
    pub used_percent: u32,
    pub exhausted: bool,
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderQuota {
    pub id: String,
    pub name: String,
    pub available: bool,
    pub status: String,
    pub message: Option<String>,
    pub plan: Option<String>,
    pub credit_balance: Option<String>,
    pub source: Option<String>,
    pub observed_at: Option<String>,
    pub age_seconds: Option<u64>,
    pub windows: Vec<QuotaWindow>,
    /// Compact headline fragments for the top-bar capsule
    pub headline: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuotaSnapshot {
    pub generated_at: String,
    pub providers: Vec<ProviderQuota>,
}

struct CacheEntry {
    at: Instant,
    snapshot: QuotaSnapshot,
}

static CACHE: Mutex<Option<CacheEntry>> = Mutex::new(None);
/// Non-forced reads reuse the last network snapshot for 2 minutes.
const CACHE_TTL: Duration = Duration::from_secs(120);

pub fn fetch_quota_snapshot(force: bool) -> QuotaSnapshot {
    if !force {
        if let Ok(guard) = CACHE.lock() {
            if let Some(entry) = guard.as_ref() {
                if entry.at.elapsed() < CACHE_TTL {
                    return stamp_ages(entry.snapshot.clone());
                }
            }
        }
    }

    let generated_at = chrono::Utc::now().to_rfc3339();
    let mut providers = Vec::with_capacity(2);

    match cursor::fetch() {
        Ok(p) => providers.push(p),
        Err(msg) => providers.push(ProviderQuota {
            id: "cursor".into(),
            name: "Cursor".into(),
            available: false,
            status: "error".into(),
            message: Some(msg),
            plan: None,
            credit_balance: None,
            source: None,
            observed_at: None,
            age_seconds: None,
            windows: vec![],
            headline: vec![],
        }),
    }

    match antigravity::fetch() {
        Ok(p) => providers.push(p),
        Err(msg) => providers.push(ProviderQuota {
            id: "antigravity".into(),
            name: "Antigravity".into(),
            available: false,
            status: "error".into(),
            message: Some(msg),
            plan: None,
            credit_balance: None,
            source: None,
            observed_at: None,
            age_seconds: None,
            windows: vec![],
            headline: vec![],
        }),
    }

    let snapshot = QuotaSnapshot {
        generated_at,
        providers,
    };

    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some(CacheEntry {
            at: Instant::now(),
            snapshot: snapshot.clone(),
        });
    }

    stamp_ages(snapshot)
}

fn stamp_ages(mut snapshot: QuotaSnapshot) -> QuotaSnapshot {
    let now = chrono::Utc::now();
    for p in &mut snapshot.providers {
        if let Some(obs) = &p.observed_at {
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(obs) {
                let age = (now - dt.with_timezone(&chrono::Utc))
                    .num_seconds()
                    .max(0) as u64;
                p.age_seconds = Some(age);
            }
        }
    }
    snapshot
}

pub(crate) fn used_percent(fraction: f64) -> u32 {
    let clamped = fraction.clamp(0.0, 1.0);
    if clamped <= 0.0 {
        0
    } else if clamped >= 1.0 {
        100
    } else {
        ((clamped * 100.0).round() as u32).clamp(1, 99)
    }
}

pub(crate) fn unavailable(
    id: &str,
    name: &str,
    status: &str,
    message: impl Into<String>,
) -> ProviderQuota {
    ProviderQuota {
        id: id.into(),
        name: name.into(),
        available: false,
        status: status.into(),
        message: Some(message.into()),
        plan: None,
        credit_balance: None,
        source: None,
        observed_at: None,
        age_seconds: None,
        windows: vec![],
        headline: vec![],
    }
}
