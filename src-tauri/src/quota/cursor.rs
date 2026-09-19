use super::{unavailable, used_percent, ProviderQuota, QuotaWindow};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;
use std::path::PathBuf;

const TOKEN_KEY: &str = "cursorAuth/accessToken";
const ENDPOINT: &str = "https://cursor.com/api/usage-summary";
const EXPIRY_HEADROOM_SECS: i64 = 60;

pub fn fetch() -> Result<ProviderQuota, String> {
    let token = match read_access_token() {
        Some(t) => t,
        None => {
            return Ok(unavailable(
                "cursor",
                "Cursor",
                "sign_in_required",
                "未检测到 Cursor 登录，请先在 Cursor 中登录",
            ));
        }
    };

    let cookie = match session_cookie(&token) {
        Some(c) => c,
        None => {
            return Ok(unavailable(
                "cursor",
                "Cursor",
                "login_expired",
                "Cursor 登录已过期，请打开 Cursor 以刷新登录",
            ));
        }
    };

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get(ENDPOINT)
        .header("Cookie", cookie)
        .header("Accept", "application/json")
        .send()
        .map_err(|_| "无法连接 cursor.com".to_string())?;

    match response.status().as_u16() {
        200 => {}
        401 | 403 => {
            return Ok(unavailable(
                "cursor",
                "Cursor",
                "login_expired",
                "Cursor 登录已失效，请打开 Cursor 重新登录",
            ));
        }
        429 => {
            return Ok(unavailable(
                "cursor",
                "Cursor",
                "rate_limited",
                "Cursor 额度接口限流，请稍后再试",
            ));
        }
        code => {
            return Ok(unavailable(
                "cursor",
                "Cursor",
                "server_error",
                format!("Cursor 额度接口返回 HTTP {code}"),
            ));
        }
    }

    let reply: Reply = response
        .json()
        .map_err(|_| "无法解析 Cursor 额度响应".to_string())?;

    let windows = windows_from(&reply);
    if windows.is_empty() {
        return Ok(unavailable(
            "cursor",
            "Cursor",
            "no_limits",
            "当前账号未返回可用额度池",
        ));
    }

    let headline = windows
        .iter()
        .filter(|w| w.id == "cursorModels" || w.id == "otherModels")
        .map(|w| format!("{}%", w.used_percent))
        .collect::<Vec<_>>();

    let headline = if headline.is_empty() {
        windows
            .first()
            .map(|w| vec![format!("{}%", w.used_percent)])
            .unwrap_or_default()
    } else {
        headline
    };

    Ok(ProviderQuota {
        id: "cursor".into(),
        name: "Cursor".into(),
        available: true,
        status: "ok".into(),
        message: None,
        plan: plan_name(reply.membership_type.as_deref()),
        credit_balance: remaining_balance(&reply),
        source: Some("usage_summary".into()),
        observed_at: Some(chrono::Utc::now().to_rfc3339()),
        age_seconds: Some(0),
        windows,
        headline,
    })
}

fn state_db_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join("Library/Application Support/Cursor/User/globalStorage/state.vscdb")
}

fn read_access_token() -> Option<String> {
    let path = state_db_path();
    if !path.exists() {
        return None;
    }

    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(&path, flags).ok()?;
    let mut stmt = conn
        .prepare("SELECT value FROM ItemTable WHERE key = ?1 LIMIT 1")
        .ok()?;
    let mut rows = stmt.query(rusqlite::params![TOKEN_KEY]).ok()?;
    let row = rows.next().ok()??;

    // Prefer text; fall back to blob (UTF-16LE without BOM has been seen).
    if let Ok(text) = row.get::<_, String>(0) {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    if let Ok(blob) = row.get::<_, Vec<u8>>(0) {
        return decode_token_blob(&blob);
    }
    None
}

fn decode_token_blob(data: &[u8]) -> Option<String> {
    if data.len() >= 2 && data.len() % 2 == 0 && data[0] != 0 && data[1] == 0 {
        let wide: Vec<u16> = data
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16(&wide)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
    }
    String::from_utf8(data.to_vec())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn session_cookie(token: &str) -> Option<String> {
    let claims = jwt_claims(token)?;
    let sub = claims.get("sub")?.as_str()?;
    let exp = claims.get("exp")?.as_f64()? as i64;
    let now = chrono::Utc::now().timestamp();
    if exp - now <= EXPIRY_HEADROOM_SECS {
        return None;
    }
    let account = sub.split('|').next_back().unwrap_or(sub);
    if account.is_empty() {
        return None;
    }
    // WorkosCursorSessionToken=account%3A%3Atoken  (:: → %3A%3A)
    Some(format!(
        "WorkosCursorSessionToken={account}%3A%3A{token}"
    ))
}

fn jwt_claims(token: &str) -> Option<serde_json::Value> {
    let mut parts = token.split('.');
    let _ = parts.next()?;
    let payload = parts.next()?;
    let _ = parts.next()?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice(&bytes).ok()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Allowance {
    enabled: Option<bool>,
    used: Option<f64>,
    limit: Option<f64>,
    remaining: Option<f64>,
    auto_percent_used: Option<f64>,
    api_percent_used: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Individual {
    plan: Option<Allowance>,
    on_demand: Option<Allowance>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Team {
    pooled: Option<Allowance>,
    on_demand: Option<Allowance>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Reply {
    billing_cycle_end: Option<String>,
    membership_type: Option<String>,
    individual_usage: Option<Individual>,
    team_usage: Option<Team>,
}

fn windows_from(reply: &Reply) -> Vec<QuotaWindow> {
    let resets = reply.billing_cycle_end.clone();
    let plan = reply
        .individual_usage
        .as_ref()
        .and_then(|i| i.plan.as_ref())
        .or_else(|| reply.team_usage.as_ref().and_then(|t| t.pooled.as_ref()));

    let mut found = Vec::new();

    if let Some(plan) = plan {
        if let Some(w) = pool_window(
            plan.auto_percent_used,
            "cursorModels",
            Some("Cursor Models"),
            resets.clone(),
        ) {
            found.push(w);
        }
        if let Some(w) = pool_window(
            plan.api_percent_used,
            "otherModels",
            Some("Other Models"),
            resets.clone(),
        ) {
            found.push(w);
        }
        if found.is_empty() {
            if let Some(w) = money_window(plan, "plan", "monthly", resets.clone()) {
                found.push(w);
            }
        }
    }

    let on_demand = reply
        .individual_usage
        .as_ref()
        .and_then(|i| i.on_demand.as_ref())
        .or_else(|| reply.team_usage.as_ref().and_then(|t| t.on_demand.as_ref()));
    if let Some(od) = on_demand {
        if let Some(w) = money_window(od, "onDemand", "spend", resets) {
            found.push(w);
        }
    }

    found
}

/// Cursor pool fields are percentages (0.0267 means 0.0267%), not fractions.
fn pool_window(
    percent: Option<f64>,
    id: &str,
    scope: Option<&str>,
    resets_at: Option<String>,
) -> Option<QuotaWindow> {
    let percent = percent?;
    let used_fraction = (percent / 100.0).clamp(0.0, 1.0);
    Some(QuotaWindow {
        id: id.into(),
        kind: "monthly".into(),
        scope: scope.map(|s| s.into()),
        used_fraction,
        used_percent: used_percent(used_fraction),
        exhausted: percent >= 100.0,
        resets_at,
    })
}

fn money_window(
    allowance: &Allowance,
    id: &str,
    kind: &str,
    resets_at: Option<String>,
) -> Option<QuotaWindow> {
    if allowance.enabled == Some(false) {
        return None;
    }
    let used = allowance.used?;
    let limit = allowance.limit?;
    if limit <= 0.0 {
        return None;
    }
    let used_fraction = (used / limit).clamp(0.0, 1.0);
    Some(QuotaWindow {
        id: id.into(),
        kind: kind.into(),
        scope: None,
        used_fraction,
        used_percent: used_percent(used_fraction),
        exhausted: used >= limit,
        resets_at,
    })
}

fn remaining_balance(reply: &Reply) -> Option<String> {
    let allowance = reply
        .individual_usage
        .as_ref()
        .and_then(|i| i.plan.as_ref())
        .or_else(|| reply.team_usage.as_ref().and_then(|t| t.pooled.as_ref()))?;
    let remaining = allowance.remaining?;
    Some(format!("${:.2}", remaining / 100.0))
}

fn plan_name(membership: Option<&str>) -> Option<String> {
    let membership = membership?.trim();
    if membership.is_empty() {
        return None;
    }
    let mut out = String::new();
    for part in membership.split('_') {
        let piece = if part.eq_ignore_ascii_case("plus") {
            "+".to_string()
        } else if part.is_empty() {
            continue;
        } else {
            let mut chars = part.chars();
            let first = chars.next()?.to_uppercase().collect::<String>();
            format!("{first}{}", chars.as_str().to_lowercase())
        };
        if out.is_empty() || piece == "+" {
            out.push_str(&piece);
        } else {
            out.push(' ');
            out.push_str(&piece);
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}
