use super::{unavailable, used_percent, ProviderQuota, QuotaWindow};
use serde::Deserialize;
use std::process::Command;

const QUOTA_METHOD: &str = "exa.language_server_pb.LanguageServerService/RetrieveUserQuotaSummary";
const STATUS_METHOD: &str = "exa.language_server_pb.LanguageServerService/GetUserStatus";
const CSRF_HEADER: &str = "x-codeium-csrf-token";
const CLOUD_QUOTA_SUMMARY: &str =
    "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary";

pub fn fetch() -> Result<ProviderQuota, String> {
    if let Some(p) = fetch_local()? {
        return Ok(p);
    }
    if let Some(p) = fetch_cloud_code()? {
        return Ok(p);
    }
    Ok(unavailable(
        "antigravity",
        "Antigravity",
        "not_running",
        "未检测到运行中的 Antigravity，且本机 OAuth 无法拉取远程额度。请打开 Antigravity 或确认已登录",
    ))
}

fn fetch_local() -> Result<Option<ProviderQuota>, String> {
    let servers = locate_servers();
    if servers.is_empty() {
        return Ok(None);
    }

    let client = loopback_client()?;
    let mut something_answered = false;
    let mut answered_empty = false;

    for server in servers {
        for port in server.ports {
            match ask_quota(&client, port, &server.token) {
                Ok(windows) if !windows.is_empty() => {
                    let plan = ask_plan(&client, port, &server.token);
                    let headline = headline_from(&windows);
                    return Ok(Some(ProviderQuota {
                        id: "antigravity".into(),
                        name: "Antigravity".into(),
                        available: true,
                        status: "ok".into(),
                        message: None,
                        plan,
                        credit_balance: None,
                        source: Some("language_server".into()),
                        observed_at: Some(chrono::Utc::now().to_rfc3339()),
                        age_seconds: Some(0),
                        windows,
                        headline,
                    }));
                }
                Ok(_) => {
                    answered_empty = true;
                    something_answered = true;
                }
                Err(AskErr::Continue) => {
                    something_answered = true;
                }
                Err(AskErr::Skip) => {}
            }
        }
    }

    if answered_empty {
        return Ok(Some(unavailable(
            "antigravity",
            "Antigravity",
            "no_limits",
            "Antigravity 已响应但未返回额度窗口",
        )));
    }
    if something_answered {
        return Ok(Some(unavailable(
            "antigravity",
            "Antigravity",
            "not_answering",
            "Antigravity 进程在运行，但额度接口未返回有效数据",
        )));
    }
    Ok(None)
}

fn fetch_cloud_code() -> Result<Option<ProviderQuota>, String> {
    let token = read_oauth_access_token()?;
    let Some(token) = token else {
        return Ok(None);
    };

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let body = serde_json::json!({
        "ideName": "antigravity",
        "extensionName": "antigravity",
        "locale": "en",
        "ideVersion": "unknown"
    });

    let response = match client
        .post(CLOUD_QUOTA_SUMMARY)
        .bearer_auth(&token)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
    {
        Ok(r) => r,
        Err(_) => return Ok(None),
    };

    if !response.status().is_success() {
        return Ok(None);
    }

    let reply: QuotaReply = match response.json() {
        Ok(r) => r,
        Err(_) => return Ok(None),
    };

    let windows = windows_from_reply(&reply);
    if windows.is_empty() {
        return Ok(None);
    }

    let headline = headline_from(&windows);
    Ok(Some(ProviderQuota {
        id: "antigravity".into(),
        name: "Antigravity".into(),
        available: true,
        status: "ok".into(),
        message: None,
        plan: None,
        credit_balance: None,
        source: Some("cloud_code".into()),
        observed_at: Some(chrono::Utc::now().to_rfc3339()),
        age_seconds: Some(0),
        windows,
        headline,
    }))
}

fn read_oauth_access_token() -> Result<Option<String>, String> {
    let home = dirs::home_dir().ok_or_else(|| "无法定位 home".to_string())?;

    // Prefer ~/.gemini/oauth_creds.json when not expired.
    let creds_path = home.join(".gemini/oauth_creds.json");
    if creds_path.exists() {
        if let Ok(raw) = std::fs::read_to_string(&creds_path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                let token = v
                    .get("access_token")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string());
                let expired = v
                    .get("expiry_date")
                    .and_then(|e| e.as_i64())
                    .map(|ms| ms <= chrono::Utc::now().timestamp_millis() + 60_000)
                    .unwrap_or(false);
                if let Some(token) = token {
                    if !expired && !token.is_empty() {
                        return Ok(Some(token));
                    }
                }
            }
        }
    }

    // Fallback: jetski-standalone-oauth-token
    let jetski = home.join(".gemini/jetski-standalone-oauth-token");
    if jetski.exists() {
        if let Ok(raw) = std::fs::read_to_string(&jetski) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(token) = v
                    .pointer("/token/access_token")
                    .and_then(|t| t.as_str())
                    .or_else(|| v.get("access_token").and_then(|t| t.as_str()))
                {
                    if !token.is_empty() {
                        return Ok(Some(token.to_string()));
                    }
                }
            }
        }
    }

    Ok(None)
}

struct Server {
    ports: Vec<u16>,
    token: String,
}

fn locate_servers() -> Vec<Server> {
    language_server_processes()
        .into_iter()
        .filter_map(|(pid, token)| {
            let ports = listening_ports(pid);
            if ports.is_empty() {
                None
            } else {
                Some(Server { ports, token })
            }
        })
        .collect()
}

fn language_server_processes() -> Vec<(i32, String)> {
    let output = Command::new("/bin/ps")
        .args(["-axww", "-o", "pid=,command="])
        .output()
        .ok();
    let Some(output) = output else {
        return vec![];
    };
    let listing = String::from_utf8_lossy(&output.stdout);

    let origins = ["/Antigravity.app/", "/Antigravity IDE.app/"];
    let mut found = Vec::new();

    for origin in origins {
        for line in listing.lines().filter(|l| l.contains("/language_server")) {
            if !line.contains(origin) {
                continue;
            }
            let fields: Vec<&str> = line.split_whitespace().collect();
            let Some(pid) = fields.first().and_then(|p| p.parse::<i32>().ok()) else {
                continue;
            };
            let Some(idx) = fields.iter().position(|f| *f == "--csrf_token") else {
                continue;
            };
            if idx + 1 >= fields.len() {
                continue;
            }
            found.push((pid, fields[idx + 1].to_string()));
        }
    }

    // Also try agy CLI language_server if present (path may not include Antigravity.app)
    for line in listing.lines().filter(|l| l.contains("/language_server")) {
        if line.contains("/Antigravity.app/") || line.contains("/Antigravity IDE.app/") {
            continue;
        }
        if !(line.contains("antigravity") || line.contains("/agy") || line.contains("gemini")) {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        let Some(pid) = fields.first().and_then(|p| p.parse::<i32>().ok()) else {
            continue;
        };
        let Some(idx) = fields.iter().position(|f| *f == "--csrf_token") else {
            continue;
        };
        if idx + 1 >= fields.len() {
            continue;
        }
        found.push((pid, fields[idx + 1].to_string()));
    }

    found
}

fn listening_ports(pid: i32) -> Vec<u16> {
    let output = Command::new("/usr/sbin/lsof")
        .args([
            "-nP",
            "-a",
            "-p",
            &pid.to_string(),
            "-iTCP",
            "-sTCP:LISTEN",
            "-F",
            "n",
        ])
        .output()
        .ok();
    let Some(output) = output else {
        return vec![];
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let line = line.strip_prefix('n')?;
            line.rsplit(':').next()?.parse::<u16>().ok()
        })
        .collect()
}

fn loopback_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(6))
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())
}

enum AskErr {
    Skip,
    Continue,
}

fn ask_quota(
    client: &reqwest::blocking::Client,
    port: u16,
    token: &str,
) -> Result<Vec<QuotaWindow>, AskErr> {
    let data = post(client, QUOTA_METHOD, port, token)?;
    let reply: QuotaReply = serde_json::from_slice(&data).map_err(|_| AskErr::Continue)?;
    Ok(windows_from_reply(&reply))
}

fn ask_plan(client: &reqwest::blocking::Client, port: u16, token: &str) -> Option<String> {
    let data = post(client, STATUS_METHOD, port, token).ok()?;
    let reply: StatusReply = serde_json::from_slice(&data).ok()?;
    reply
        .user_status
        .and_then(|u| u.plan_status)
        .and_then(|p| p.plan_info)
        .and_then(|i| i.plan_name)
        .filter(|n| !n.is_empty())
}

fn post(
    client: &reqwest::blocking::Client,
    method: &str,
    port: u16,
    token: &str,
) -> Result<Vec<u8>, AskErr> {
    let url = format!("https://127.0.0.1:{port}/{method}");
    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header(CSRF_HEADER, token)
        .body("{}")
        .send()
        .map_err(|_| AskErr::Skip)?;

    match response.status().as_u16() {
        200 => response.bytes().map(|b| b.to_vec()).map_err(|_| AskErr::Skip),
        401 | 403 => Err(AskErr::Continue),
        _ => Err(AskErr::Skip),
    }
}

#[derive(Debug, Clone, Deserialize)]
struct QuotaReply {
    response: Option<QuotaResponse>,
    #[serde(default)]
    groups: Option<Vec<QuotaGroup>>,
}

#[derive(Debug, Clone, Deserialize)]
struct QuotaResponse {
    groups: Option<Vec<QuotaGroup>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaGroup {
    display_name: Option<String>,
    buckets: Option<Vec<QuotaBucket>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaBucket {
    bucket_id: Option<String>,
    window: Option<String>,
    remaining_fraction: Option<f64>,
    remaining: Option<RemainingNest>,
    reset_time: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemainingNest {
    remaining_fraction: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatusReply {
    user_status: Option<UserStatus>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserStatus {
    plan_status: Option<PlanStatus>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanStatus {
    plan_info: Option<PlanInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanInfo {
    plan_name: Option<String>,
}

fn windows_from_reply(reply: &QuotaReply) -> Vec<QuotaWindow> {
    let groups = reply
        .response
        .as_ref()
        .and_then(|r| r.groups.as_ref())
        .or(reply.groups.as_ref())
        .cloned()
        .unwrap_or_default();

    // Keep provider group order (Gemini → Claude/GPT). Within a group show the
    // shorter window first (5-hour, then weekly).
    let mut windows: Vec<QuotaWindow> = groups
        .into_iter()
        .flat_map(|group| {
            let scope = model_group(group.display_name.as_deref());
            let mut buckets = group
                .buckets
                .unwrap_or_default()
                .into_iter()
                .filter_map(|b| window_from_bucket(b, scope.clone()))
                .collect::<Vec<_>>();
            buckets.sort_by_key(|w| kind_within_group(&w.kind));
            buckets
        })
        .collect();

    windows.sort_by(|a, b| {
        scope_sort_key(a)
            .cmp(&scope_sort_key(b))
            .then_with(|| kind_within_group(&a.kind).cmp(&kind_within_group(&b.kind)))
    });
    windows
}

fn window_from_bucket(bucket: QuotaBucket, scope: Option<String>) -> Option<QuotaWindow> {
    let id = bucket.bucket_id?;
    let remaining = bucket
        .remaining_fraction
        .or_else(|| bucket.remaining.and_then(|r| r.remaining_fraction))?;
    let kind = length_kind(bucket.window.as_deref())?;
    let used_fraction = (1.0 - remaining).clamp(0.0, 1.0);
    Some(QuotaWindow {
        id,
        kind,
        scope,
        used_fraction,
        used_percent: used_percent(used_fraction),
        exhausted: remaining <= 0.0,
        resets_at: bucket.reset_time,
    })
}

fn length_kind(window: Option<&str>) -> Option<String> {
    let window = window?.to_lowercase();
    match window.as_str() {
        "5h" => Some("fiveHour".into()),
        "weekly" => Some("weekly".into()),
        "daily" => Some("daily".into()),
        "monthly" => Some("monthly".into()),
        _ => {
            let (n, unit) = window.split_at(window.len().saturating_sub(1));
            let count: u32 = n.parse().ok()?;
            match unit {
                "h" => Some(if count == 5 {
                    "fiveHour".into()
                } else {
                    format!("other_{count}h")
                }),
                "d" => Some(if count == 7 {
                    "weekly".into()
                } else {
                    format!("other_{count}d")
                }),
                _ => None,
            }
        }
    }
}

/// Gemini first, then Claude/GPT (3p), then anything else.
fn scope_sort_key(w: &QuotaWindow) -> u32 {
    let scope = w.scope.as_deref().unwrap_or("").to_lowercase();
    let id = w.id.to_lowercase();
    if scope.contains("gemini") || id.contains("gemini") {
        0
    } else if scope.contains("claude")
        || scope.contains("gpt")
        || id.starts_with("3p")
        || id.contains("claude")
    {
        1
    } else {
        2
    }
}

/// Within a model group, 5-hour above weekly — the shorter window bites first.
fn kind_within_group(kind: &str) -> u32 {
    match kind {
        "fiveHour" => 0,
        "daily" => 1,
        "weekly" => 2,
        "monthly" => 3,
        _ => 9,
    }
}

fn model_group(name: Option<&str>) -> Option<String> {
    let name = name?.trim();
    if name.is_empty() {
        return None;
    }
    let words: Vec<&str> = name.split_whitespace().collect();
    if words.len() > 1 && words.last().map(|w| w.eq_ignore_ascii_case("models")).unwrap_or(false)
    {
        return Some(words[..words.len() - 1].join(" "));
    }
    Some(name.to_string())
}

fn headline_from(windows: &[QuotaWindow]) -> Vec<String> {
    // Prefer Gemini 5h + weekly when present; else the two highest-urgency windows.
    let gemini_5h = windows.iter().find(|w| {
        w.id.contains("gemini") && w.kind == "fiveHour"
            || (w.scope.as_deref() == Some("Gemini") && w.kind == "fiveHour")
    });
    let gemini_week = windows.iter().find(|w| {
        w.id.contains("gemini") && w.kind == "weekly"
            || (w.scope.as_deref() == Some("Gemini") && w.kind == "weekly")
    });

    let mut parts = Vec::new();
    if let Some(w) = gemini_5h {
        parts.push(format!("5h {}%", w.used_percent));
    }
    if let Some(w) = gemini_week {
        parts.push(format!("wk {}%", w.used_percent));
    }
    if !parts.is_empty() {
        return parts;
    }

    windows.iter().take(2).map(|w| format!("{}%", w.used_percent)).collect()
}
