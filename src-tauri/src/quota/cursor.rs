use super::{unavailable, used_percent, ProviderQuota, QuotaWindow};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;
use std::path::PathBuf;
use std::time::Duration;

const TOKEN_KEY: &str = "cursorAuth/accessToken";
const ENDPOINT: &str = "https://cursor.com/api/usage-summary";
const EXPIRY_HEADROOM_SECS: i64 = 60;
/// Cursor 在内存里刷新 token 后写 state.vscdb 有延迟，旧 token 会短暂吃 401；
/// 等一会儿重读 DB 再试一次，两次都被拒才判定登录失效
const AUTH_RETRY_DELAY: Duration = Duration::from_secs(5);

pub fn fetch() -> Result<ProviderQuota, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;

    let mut outcome = attempt(&client)?;
    if matches!(outcome, Outcome::AuthRejected) {
        std::thread::sleep(AUTH_RETRY_DELAY);
        outcome = attempt(&client)?;
    }

    let reply = match outcome {
        Outcome::Ok(reply) => reply,
        Outcome::AuthRejected => {
            return Ok(unavailable(
                "cursor",
                "Cursor",
                "login_expired",
                "Cursor 登录已失效，请打开 Cursor 重新登录",
            ));
        }
        Outcome::Unavailable(quota) => return Ok(quota),
    };

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

enum Outcome {
    Ok(Reply),
    AuthRejected,
    Unavailable(ProviderQuota),
}

/// 进程内诊断：token 读取状态 + 单次真实抓取结果（/api/quota/debug）
pub fn debug_probe() -> serde_json::Value {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let token = read_access_token();
    let (token_len, token_hash8, jwt_exp) = match token.as_deref() {
        Some(t) => {
            let mut hasher = DefaultHasher::new();
            t.hash(&mut hasher);
            let hash8 = format!("{:016x}", hasher.finish());
            let exp = jwt_claims(t)
                .and_then(|c| c.get("exp").and_then(|e| e.as_f64()))
                .and_then(|e| chrono::DateTime::from_timestamp(e as i64, 0))
                .map(|dt| dt.to_rfc3339());
            (Some(t.len()), Some(hash8), exp)
        }
        None => (None, None, None),
    };

    let attempt = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(client) => match attempt(&client) {
            Ok(Outcome::Ok(_)) => "ok".to_string(),
            Ok(Outcome::AuthRejected) => "auth_rejected".to_string(),
            Ok(Outcome::Unavailable(p)) => format!("unavailable:{}", p.status),
            Err(e) => format!("transport:{e}"),
        },
        Err(e) => format!("client:{e}"),
    };

    serde_json::json!({
        "token_present": token.is_some(),
        "token_len": token_len,
        "token_hash8": token_hash8,
        "jwt_exp": jwt_exp,
        "attempt": attempt,
    })
}

/// 单次抓取：每次都重读 state.vscdb，重试路径靠这一步拿到 Cursor 刚落盘的新 token
fn attempt(client: &reqwest::blocking::Client) -> Result<Outcome, String> {
    let token = match read_access_token() {
        Some(t) => t,
        None => {
            return Ok(Outcome::Unavailable(unavailable(
                "cursor",
                "Cursor",
                "sign_in_required",
                "未检测到 Cursor 登录，请先在 Cursor 中登录",
            )));
        }
    };

    let cookie = match session_cookie(&token) {
        Some(c) => c,
        None => {
            return Ok(Outcome::Unavailable(unavailable(
                "cursor",
                "Cursor",
                "login_expired",
                "Cursor 登录已过期，请打开 Cursor 以刷新登录",
            )));
        }
    };

    let response = client
        .get(ENDPOINT)
        .header("Cookie", cookie)
        .header("Accept", "application/json")
        .send()
        .map_err(|_| "无法连接 cursor.com".to_string())?;

    match response.status().as_u16() {
        200 => response
            .json::<Reply>()
            .map(Outcome::Ok)
            .map_err(|_| "无法解析 Cursor 额度响应".to_string()),
        // 401 = 真认证失败（token 作废），重试一次覆盖落盘延迟；
        // 403 = 边缘节点风控人机校验（TLS 指纹等），重试无意义，单独文案
        401 => Ok(Outcome::AuthRejected),
        403 => Ok(Outcome::Unavailable(unavailable(
            "cursor",
            "Cursor",
            "blocked",
            "被 Cursor 接口风控拦截，请稍后重试",
        ))),
        429 => Ok(Outcome::Unavailable(unavailable(
            "cursor",
            "Cursor",
            "rate_limited",
            "Cursor 额度接口限流，请稍后再试",
        ))),
        code => Ok(Outcome::Unavailable(unavailable(
            "cursor",
            "Cursor",
            "server_error",
            format!("Cursor 额度接口返回 HTTP {code}"),
        ))),
    }
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
    billing_cycle_start: Option<String>,
    membership_type: Option<String>,
    individual_usage: Option<Individual>,
    team_usage: Option<Team>,
}

fn windows_from(reply: &Reply) -> Vec<QuotaWindow> {
    let resets = reply.billing_cycle_end.clone();
    let starts = reply.billing_cycle_start.clone();
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
            starts.clone(),
        ) {
            found.push(w);
        }
        if let Some(w) = pool_window(
            plan.api_percent_used,
            "otherModels",
            Some("Other Models"),
            resets.clone(),
            starts.clone(),
        ) {
            found.push(w);
        }
        if found.is_empty() {
            if let Some(w) = money_window(plan, "plan", "monthly", resets.clone(), starts.clone()) {
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
        if let Some(w) = money_window(od, "onDemand", "spend", resets, starts) {
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
    starts_at: Option<String>,
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
        starts_at,
    })
}

fn money_window(
    allowance: &Allowance,
    id: &str,
    kind: &str,
    resets_at: Option<String>,
    starts_at: Option<String>,
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
        starts_at,
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
