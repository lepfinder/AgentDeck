//! Built-in local dev service lifecycle manager (start / stop / status / health).

use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

use crate::service_config::{
    build_port_owners, find_runtime, iter_runtimes, make_service_key, migrate_v1_json,
    parse_config_json, parse_service_key, ProjectDefinition, ServiceDefinition,
    ServiceRegistrationPayload, ServiceRuntime, ServicesConfig, StartConfig, HealthConfig,
    StopConfig, PreStartConfig,
};

pub use crate::service_config::{
    HealthConfig as ServiceHealthConfig, ServiceDefinition as ServiceSpec,
    ServiceRegistrationPayload as ServiceUpsertPayload, StartConfig as ServiceStartConfig,
};

/// HTTP timeout for status-page probes (keep startup wait timeouts separate).
const PROBE_HTTP_MAX_SECS: &str = "1";

struct ProbeContext {
    listening_ports: HashSet<u32>,
}

// ── API response types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceMeta {
    pub id: String,
    pub project_id: String,
    pub service_id: String,
    pub project_name: String,
    pub name: String,
    pub ports: Vec<u32>,
    pub open_url: String,
    pub project_dir: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortStatus {
    pub port: u32,
    pub listening: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceStatus {
    pub id: String,
    pub project_id: String,
    pub service_id: String,
    pub project_name: String,
    pub name: String,
    pub state: String,
    pub pid: Option<u32>,
    pub ports: Vec<PortStatus>,
    pub health: String,
    pub open_url: String,
    pub log_file: String,
    pub project_dir: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceActionEvent {
    pub id: String,
    pub success: bool,
    pub message: String,
}

fn pending_ops() -> &'static Mutex<HashMap<String, String>> {
    static OPS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    OPS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn set_pending(id: &str, op: &str) {
    if let Ok(mut ops) = pending_ops().lock() {
        ops.insert(id.to_string(), op.to_string());
    }
}

fn clear_pending(id: &str) {
    if let Ok(mut ops) = pending_ops().lock() {
        ops.remove(id);
    }
}

fn pending_op(id: &str) -> Option<String> {
    pending_ops().lock().ok()?.get(id).cloned()
}

fn emit_action_complete(app: &AppHandle, id: &str, success: bool, message: String) {
    let _ = app.emit(
        "service:action-complete",
        ServiceActionEvent {
            id: id.to_string(),
            success,
            message,
        },
    );
}

fn run_start_and_wait(app: AppHandle, rt: ServiceRuntime, op_label: String) {
    let key = rt.service_key.clone();
    let name = rt.display_name.clone();
    let restarting = op_label == "restarting";
    thread::spawn(move || {
        let (success, message) = match spawn_service(&rt) {
            Ok(pid) => match wait_until_ready(&rt, pid) {
                Ok(()) => (
                    true,
                    if restarting {
                        format!("{} 已重启并就绪", name)
                    } else {
                        format!("{} 已就绪", name)
                    },
                ),
                Err(e) => (false, e),
            },
            Err(e) => (false, e),
        };
        clear_pending(&key);
        emit_action_complete(&app, &key, success, message);
    });
}

fn run_stop_async(app: AppHandle, rt: ServiceRuntime) {
    let key = rt.service_key.clone();
    thread::spawn(move || {
        let result = stop_service_impl(&rt, false, PortSweep::AllListeners);
        clear_pending(&key);
        emit_action_complete(&app, &key, result.success, result.message);
    });
}

fn run_restart_async(app: AppHandle, rt: ServiceRuntime) {
    let key = rt.service_key.clone();
    let name = rt.display_name.clone();
    thread::spawn(move || {
        stop_service_impl(&rt, false, PortSweep::AllListeners);
        thread::sleep(Duration::from_secs(1));
        let (success, message) = match spawn_service(&rt) {
            Ok(pid) => match wait_until_ready(&rt, pid) {
                Ok(()) => (true, format!("{} 已重启并就绪", name)),
                Err(e) => (false, e),
            },
            Err(e) => (false, e),
        };
        clear_pending(&key);
        emit_action_complete(&app, &key, success, message);
    });
}

// ── Path helpers ─────────────────────────────────────────────────────────────

fn agentdeck_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".agentdeck")
}

fn config_path(_app: &AppHandle) -> PathBuf {
    agentdeck_dir().join("services.json")
}

fn expand_template(value: &str, project_dir: &Path) -> String {
    let home = dirs::home_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    value
        .replace("{projectDir}", &project_dir.to_string_lossy())
        .replace('~', &home)
}

fn push_path_dir(result_parts: &mut Vec<String>, seen: &mut HashSet<String>, d: PathBuf) {
    let s = d.to_string_lossy().to_string();
    if d.exists() && seen.insert(s.clone()) {
        result_parts.push(s);
    }
}

/// 构建包含常见开发工具路径（Homebrew、NVM、FNM、Bun、Cargo 等）的增强 PATH。
/// 优先级：`~/.local/bin` 等用户 shim → 进程原 PATH → Homebrew/系统 → NVM/FNM 各版本回退。
/// 不要把「最新 NVM/FNM」插到最前，否则会覆盖用户默认 Node，打挂 native addon。
pub fn get_augmented_path() -> String {
    let mut result_parts: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    if let Some(home) = dirs::home_dir() {
        for d in [
            home.join(".local/bin"),
            home.join(".cargo/bin"),
            home.join(".bun/bin"),
            home.join("miniconda3/bin"),
            home.join("miniconda3/condabin"),
            home.join(".local/share/fnm/aliases/default/bin"),
            home.join(".nvm/current/bin"),
        ] {
            push_path_dir(&mut result_parts, &mut seen, d);
        }
    }

    let existing_path = std::env::var("PATH").unwrap_or_default();
    for p in existing_path.split(':') {
        if !p.is_empty() && seen.insert(p.to_string()) {
            result_parts.push(p.to_string());
        }
    }

    for d in [
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/opt/homebrew/sbin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/local/sbin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
        PathBuf::from("/usr/sbin"),
        PathBuf::from("/sbin"),
    ] {
        push_path_dir(&mut result_parts, &mut seen, d);
    }

    if let Some(home) = dirs::home_dir() {
        let mut version_bins: Vec<PathBuf> = Vec::new();
        let nvm_versions = home.join(".nvm/versions/node");
        if nvm_versions.is_dir() {
            if let Ok(entries) = fs::read_dir(&nvm_versions) {
                version_bins.extend(
                    entries
                        .filter_map(|e| e.ok())
                        .map(|e| e.path().join("bin"))
                        .filter(|p| p.is_dir()),
                );
            }
        }
        let fnm_versions = home.join(".local/share/fnm/node-versions");
        if fnm_versions.is_dir() {
            if let Ok(entries) = fs::read_dir(&fnm_versions) {
                version_bins.extend(
                    entries
                        .filter_map(|e| e.ok())
                        .map(|e| e.path().join("installation/bin"))
                        .filter(|p| p.is_dir()),
                );
            }
        }
        version_bins.sort();
        version_bins.reverse();
        for d in version_bins {
            push_path_dir(&mut result_parts, &mut seen, d);
        }
    }

    result_parts.join(":")
}

/// 解析可执行命令的绝对路径（当命令为相对名称如 `npm` 时在增强 PATH 中搜索）
pub fn resolve_executable(cmd: &str, path_var: &str) -> String {
    let p = Path::new(cmd);
    if p.is_absolute() || cmd.contains('/') {
        return cmd.to_string();
    }
    for dir in path_var.split(':') {
        if dir.is_empty() {
            continue;
        }
        let candidate = Path::new(dir).join(cmd);
        if candidate.is_file() {
            return candidate.to_string_lossy().to_string();
        }
    }
    cmd.to_string()
}


fn miloco_service_def() -> ServiceDefinition {
    ServiceDefinition {
        id: "main".into(),
        name: "Xiaomi Miloco".into(),
        start: StartConfig {
            command: vec![
                "{projectDir}/backend/.venv/bin/python".into(),
                "-m".into(),
                "miloco.main".into(),
            ],
            cwd: "{projectDir}/backend".into(),
            env: HashMap::new(),
            require_path: Some("{projectDir}/backend/.venv/bin/python".into()),
        },
        pre_start: None,
        pid_file: "{projectDir}/data/main.pid".into(),
        log_file: "{projectDir}/data/main.log".into(),
        ports: vec![1810],
        health: HealthConfig {
            url: "http://127.0.0.1:1810/health".into(),
            contains: Some("\"ok\"".into()),
            status_ok: true,
            fallback_urls: vec![],
            accept_http_codes: vec!["200".into()],
            timeout_secs: 60,
            poll_interval_secs: 2,
        },
        open_url: "http://localhost:1810".into(),
        stop: Some(StopConfig {
            grace_secs: 15,
            cleanup_ports: vec![1810],
        }),
    }
}

fn default_config(home: &Path) -> ServicesConfig {
    let personal = home.join("workspace/personal");
    let voxlab_dir = personal.join("VoxLab");
    let repomind_dir = personal.join("RepoMind");
    let miloco_dir = home.join("workspace/github/xiaomi-miloco");
    let voxlab_python = home
        .join("miniconda3/envs/voxlab/bin/python")
        .to_string_lossy()
        .into_owned();

    ServicesConfig {
        version: 2,
        projects: vec![
            ProjectDefinition {
                id: "voxlab".into(),
                name: "VoxLab".into(),
                project_dir: voxlab_dir.to_string_lossy().into_owned(),
                services: vec![ServiceDefinition {
                    id: "main".into(),
                    name: "VoxLab".into(),
                    start: StartConfig {
                        command: vec![voxlab_python, "main.py".into()],
                        cwd: "{projectDir}".into(),
                        env: HashMap::from([("DEV_MODE".into(), "false".into())]),
                        require_path: None,
                    },
                    pre_start: Some(PreStartConfig {
                        when: "frontendStale".into(),
                        command: vec!["npm".into(), "run".into(), "build".into()],
                        cwd: "{projectDir}/dashboard".into(),
                    }),
                    pid_file: "{projectDir}/data/main.pid".into(),
                    log_file: "{projectDir}/data/main.log".into(),
                    ports: vec![8001],
                    health: HealthConfig {
                        url: "http://127.0.0.1:8001/health".into(),
                        contains: Some("\"ok\"".into()),
                        status_ok: false,
                        fallback_urls: vec![],
                        accept_http_codes: vec![],
                        timeout_secs: 120,
                        poll_interval_secs: 2,
                    },
                    open_url: "http://localhost:8001".into(),
                    stop: Some(StopConfig {
                        grace_secs: 15,
                        cleanup_ports: vec![8001],
                    }),
                }],
            },
            ProjectDefinition {
                id: "repomind".into(),
                name: "RepoMind".into(),
                project_dir: repomind_dir.to_string_lossy().into_owned(),
                services: vec![ServiceDefinition {
                    id: "main".into(),
                    name: "RepoMind".into(),
                    start: StartConfig {
                        command: vec!["npm".into(), "run".into(), "dev".into()],
                        cwd: "{projectDir}".into(),
                        env: HashMap::new(),
                        require_path: None,
                    },
                    pre_start: None,
                    pid_file: "{projectDir}/data/main.pid".into(),
                    log_file: "{projectDir}/data/main.log".into(),
                    ports: vec![3000, 3001],
                    health: HealthConfig {
                        url: "http://localhost:3001/".into(),
                        contains: None,
                        status_ok: true,
                        fallback_urls: vec![],
                        accept_http_codes: vec!["200".into(), "404".into()],
                        timeout_secs: 90,
                        poll_interval_secs: 2,
                    },
                    open_url: "http://localhost:3000".into(),
                    stop: Some(StopConfig {
                        grace_secs: 8,
                        cleanup_ports: vec![3000, 3001],
                    }),
                }],
            },
            ProjectDefinition {
                id: "miloco".into(),
                name: "Xiaomi Miloco".into(),
                project_dir: miloco_dir.to_string_lossy().into_owned(),
                services: vec![miloco_service_def()],
            },
        ],
    }
}

fn load_config(app: &AppHandle) -> Result<ServicesConfig, String> {
    let path = config_path(app);
    if !path.exists() {
        let home = dirs::home_dir().ok_or("无法解析 HOME 目录")?;
        fs::create_dir_all(agentdeck_dir()).map_err(|e| e.to_string())?;

        // One-time import from EVA if present.
        let eva_services = home
            .join("Library/Application Support/com.xiyangxie.eva/userData/services.json");
        let config = if eva_services.is_file() {
            match fs::read_to_string(&eva_services)
                .ok()
                .and_then(|raw| migrate_v1_json(&raw).ok())
            {
                Some(imported) => imported,
                None => default_config(&home),
            }
        } else {
            default_config(&home)
        };

        let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
        fs::write(&path, json).map_err(|e| e.to_string())?;
        return Ok(config);
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut config = parse_config_json(&raw)?;
    if migrate_config(&mut config) {
        if let Ok(json) = serde_json::to_string_pretty(&config) {
            let _ = fs::write(config_path(app), json);
        }
    }
    Ok(config)
}

/// Patch older v2 configs in-memory.
fn migrate_config(config: &mut ServicesConfig) -> bool {
    let mut changed = false;
    for project in &mut config.projects {
        if project.id == "repomind" {
            for svc in &mut project.services {
                if svc.health.url.contains(":3000") {
                    svc.health.url = "http://localhost:3001/".into();
                    changed = true;
                }
                if svc.health.url.contains("127.0.0.1:3001") {
                    svc.health.url = "http://localhost:3001/".into();
                    changed = true;
                }
                if !svc.health.fallback_urls.is_empty() {
                    svc.health.fallback_urls.clear();
                    changed = true;
                }
                if svc.health.accept_http_codes.is_empty() {
                    svc.health.accept_http_codes = vec!["200".into(), "404".into()];
                    changed = true;
                }
                if let Some(stop) = svc.stop.as_mut() {
                    if stop.grace_secs > 8 {
                        stop.grace_secs = 8;
                        changed = true;
                    }
                    if !stop.cleanup_ports.contains(&3000) || !stop.cleanup_ports.contains(&3001) {
                        stop.cleanup_ports = vec![3000, 3001];
                        changed = true;
                    }
                }
            }
        }
    }
    if !config.projects.iter().any(|p| p.id == "miloco") {
        if let Some(home) = dirs::home_dir() {
            let miloco_dir = home.join("workspace/github/xiaomi-miloco");
            config.projects.push(ProjectDefinition {
                id: "miloco".into(),
                name: "Xiaomi Miloco".into(),
                project_dir: miloco_dir.to_string_lossy().into_owned(),
                services: vec![miloco_service_def()],
            });
            changed = true;
        }
    }
    changed
}

// ── Process / port probes ────────────────────────────────────────────────────

fn pid_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[derive(Debug, Clone)]
struct ProcessRuntime {
    started_at: String,
    uptime_secs: u64,
}

/// Parse ps `etime` ([[dd-]hh:]mm:ss) into total seconds.
fn parse_etime(raw: &str) -> Option<u64> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    let (days, rest) = if let Some((d, r)) = s.split_once('-') {
        (d.parse::<u64>().ok()?, r)
    } else {
        (0, s)
    };
    let parts: Vec<u64> = rest.split(':').filter_map(|p| p.parse().ok()).collect();
    let secs = match parts.as_slice() {
        [ss] => *ss,
        [mm, ss] => mm * 60 + ss,
        [hh, mm, ss] => hh * 3600 + mm * 60 + ss,
        _ => return None,
    };
    Some(days * 86400 + secs)
}

/// Read process start time and elapsed duration via ps (macOS/BSD).
fn process_runtime(pid: u32) -> Option<ProcessRuntime> {
    if !pid_alive(pid) {
        return None;
    }
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "lstart=,etime="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 6 {
        return None;
    }
    let etime_raw = parts.last()?;
    let uptime_secs = parse_etime(etime_raw)?;
    let lstart = parts[..parts.len() - 1].join(" ");
    Some(ProcessRuntime {
        started_at: format_lstart(&lstart).unwrap_or(lstart),
        uptime_secs,
    })
}

/// Convert `ps -o lstart=` (`Tue Aug 18 11:27:48 2026`) to `YYYY-MM-DD HH:mm:ss`.
fn format_lstart(lstart: &str) -> Option<String> {
    let parts: Vec<&str> = lstart.split_whitespace().collect();
    if parts.len() != 5 {
        return None;
    }
    let month = month_from_abbr(parts[1])?;
    let day: u32 = parts[2].parse().ok()?;
    let time = parts[3];
    let year: u32 = parts[4].parse().ok()?;
    if time.split(':').count() != 3 {
        return None;
    }
    Some(format!("{:04}-{:02}-{:02} {}", year, month, day, time))
}

fn month_from_abbr(abbr: &str) -> Option<u32> {
    match abbr {
        "Jan" => Some(1),
        "Feb" => Some(2),
        "Mar" => Some(3),
        "Apr" => Some(4),
        "May" => Some(5),
        "Jun" => Some(6),
        "Jul" => Some(7),
        "Aug" => Some(8),
        "Sep" => Some(9),
        "Oct" => Some(10),
        "Nov" => Some(11),
        "Dec" => Some(12),
        _ => None,
    }
}

fn read_pid(pid_file: &Path) -> Option<u32> {
    let raw = fs::read_to_string(pid_file).ok()?;
    raw.trim().parse().ok()
}

fn port_listener_pids(port: u32) -> Vec<u32> {
    let output = Command::new("lsof")
        .args([
            "-ti",
            &format!(":{}", port),
            "-sTCP:LISTEN",
        ])
        .output();
    let Ok(output) = output else {
        return vec![];
    };
    if !output.status.success() {
        return vec![];
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect()
}

fn collect_listening_ports() -> HashSet<u32> {
    let output = Command::new("lsof")
        .args(["-n", "-P", "-iTCP", "-sTCP:LISTEN"])
        .output();
    let Ok(output) = output else {
        return HashSet::new();
    };
    let mut ports = HashSet::new();
    for line in String::from_utf8_lossy(&output.stdout).lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 9 {
            continue;
        }
        let name_field = parts[8];
        if let Some(colon) = name_field.rfind(':') {
            if let Ok(port) = name_field[colon + 1..].parse::<u32>() {
                ports.insert(port);
            }
        }
    }
    ports
}

fn port_listening(port: u32, ctx: &ProbeContext) -> bool {
    ctx.listening_ports.contains(&port)
}

fn fetch_http_code(url: &str, timeout_secs: &str) -> Option<String> {
    let output = Command::new("curl")
        .args([
            "-s",
            "--max-time",
            timeout_secs,
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            url,
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let code = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if code.is_empty() || code == "000" {
        return None;
    }
    Some(code)
}

fn url_localhost_variants(url: &str) -> Vec<String> {
    let mut urls = vec![url.to_string()];
    if url.contains("127.0.0.1") {
        urls.push(url.replace("127.0.0.1", "localhost"));
    }
    urls.sort();
    urls.dedup();
    urls
}

fn http_code_healthy(code: &str, health: &HealthConfig) -> bool {
    if !health.accept_http_codes.is_empty() {
        return health.accept_http_codes.iter().any(|c| c == code);
    }
    if health.status_ok {
        return code.starts_with('2');
    }
    code.starts_with('2')
}

fn check_health_url(url: &str, health: &HealthConfig, timeout_secs: &str) -> bool {
    if let Some(needle) = &health.contains {
        let body_output = Command::new("curl")
            .args(["-s", "--max-time", timeout_secs, url])
            .output();
        let Ok(body_output) = body_output else {
            return false;
        };
        if !body_output.status.success() {
            return false;
        }
        let body = String::from_utf8_lossy(&body_output.stdout);
        return body.contains(needle.as_str());
    }
    fetch_http_code(url, timeout_secs)
        .map(|code| http_code_healthy(&code, health))
        .unwrap_or(false)
}

/// Startup wait: longer timeout, try all fallback URLs.
fn check_health(health: &HealthConfig) -> bool {
    let timeout = health.timeout_secs.min(3).to_string();
    for u in std::iter::once(health.url.as_str()).chain(health.fallback_urls.iter().map(String::as_str))
    {
        for url in url_localhost_variants(u) {
            if check_health_url(&url, health, &timeout) {
                return true;
            }
        }
    }
    false
}

/// Status page: short timeout, primary + one fallback only.
fn check_health_probe(health: &HealthConfig, timeout_secs: &str) -> bool {
    for url in url_localhost_variants(&health.url) {
        if check_health_url(&url, health, timeout_secs) {
            return true;
        }
    }
    if let Some(fb) = health.fallback_urls.first() {
        for url in url_localhost_variants(fb) {
            if check_health_url(&url, health, timeout_secs) {
                return true;
            }
        }
    }
    false
}

fn is_frontend_stale(project_dir: &Path) -> bool {
    let out_index = project_dir.join("dashboard/out/index.html");
    if !out_index.exists() {
        return true;
    }
    let Ok(out_mtime) = fs::metadata(&out_index).and_then(|m| m.modified()) else {
        return true;
    };
    let pkg = project_dir.join("dashboard/package.json");
    if pkg.exists() {
        if let Ok(m) = fs::metadata(&pkg).and_then(|x| x.modified()) {
            if m > out_mtime {
                return true;
            }
        }
    }
    // Full src scan is only needed before start (preStart); status page uses mtime hint above.
    false
}

fn frontend_build_hint(project_dir: &Path) -> String {
    if !project_dir.join("dashboard/out/index.html").exists() {
        return "未构建".into();
    }
    if is_frontend_stale(project_dir) {
        "构建已过期".into()
    } else {
        "构建是最新的".into()
    }
}

fn is_frontend_stale_deep(project_dir: &Path) -> bool {
    if is_frontend_stale(project_dir) {
        return true;
    }
    let out_index = project_dir.join("dashboard/out/index.html");
    let out_str = out_index.to_string_lossy().into_owned();
    let src = project_dir.join("dashboard/src");
    let src_str = src.to_string_lossy().into_owned();
    if !src.exists() {
        return false;
    }
    Command::new("find")
        .args([&src_str, "-newer", &out_str, "-type", "f"])
        .output()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false)
}

fn run_pre_start(rt: &ServiceRuntime) -> Result<(), String> {
    let Some(pre) = &rt.service.pre_start else {
        return Ok(());
    };
    let project_dir = rt.project_dir_path();
    if pre.when == "frontendStale" && !is_frontend_stale_deep(&project_dir) {
        return Ok(());
    }
    let cwd = expand_template(&pre.cwd, &project_dir);
    let path_env = get_augmented_path();
    let raw_program = expand_template(&pre.command[0], &project_dir);
    let cmd_program = resolve_executable(&raw_program, &path_env);
    let cmd_args: Vec<String> = pre.command[1..]
        .iter()
        .map(|arg| expand_template(arg, &project_dir))
        .collect();
    let mut cmd = Command::new(&cmd_program);
    cmd.args(&cmd_args)
        .current_dir(&cwd)
        .env("PATH", &path_env);
    let output = cmd
        .output()
        .map_err(|e| format!("preStart 执行失败: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "preStart 失败 (exit {}): {}",
            output.status,
            stderr.trim()
        ));
    }
    Ok(())
}

fn probe_service(
    rt: &ServiceRuntime,
    ctx: &ProbeContext,
    port_owners: &HashMap<u32, String>,
) -> ServiceStatus {
    let def = &rt.service;
    let project_dir = rt.project_dir_path();
    let pid_file = PathBuf::from(expand_template(&def.pid_file, &project_dir));
    let log_file = expand_template(&def.log_file, &project_dir);
    let mut extras = HashMap::new();

    if rt.project_id == "voxlab" {
        extras.insert("frontend".into(), frontend_build_hint(&project_dir));
    }

    let pid_from_file = pid_file.exists().then(|| read_pid(&pid_file)).flatten();
    let all_ports_up = def.ports.iter().all(|p| port_listening(*p, ctx));
    let any_port_up = def.ports.iter().any(|p| port_listening(*p, ctx));
    let foreign_owner = def.ports.iter().find_map(|p| {
        port_owners
            .get(p)
            .filter(|owner| owner.as_str() != rt.service_key.as_str())
            .cloned()
    });
    let mut state = "stopped".to_string();
    let mut pid: Option<u32> = None;
    let mut managed_externally = false;

    if let Some(p) = pid_from_file {
        if pid_alive(p) {
            pid = Some(p);
            // Multi-port services: PID alone is not "fully running" if API ports are down.
            if all_ports_up {
                state = "running".to_string();
            } else if any_port_up {
                state = "partial".to_string();
            } else {
                state = "unhealthy".to_string();
            }
        } else if let Some(owner) = foreign_owner {
            // Our pid is dead; ports belong to another registered service.
            state = "port_conflict".to_string();
            extras.insert("portOwner".into(), owner);
        } else if all_ports_up {
            state = "running".to_string();
            managed_externally = true;
            pid = def
                .ports
                .first()
                .and_then(|port| port_listener_pids(*port).first().copied());
        } else if any_port_up {
            // Partial foreign listeners — do not claim healthy ownership.
            state = "stale_pid".to_string();
        } else {
            state = "stale_pid".to_string();
        }
    } else if let Some(owner) = foreign_owner.filter(|_| any_port_up) {
        state = "port_conflict".to_string();
        extras.insert("portOwner".into(), owner);
    } else if all_ports_up {
        state = "running".to_string();
        managed_externally = true;
        pid = def
            .ports
            .first()
            .and_then(|port| port_listener_pids(*port).first().copied());
    } else if any_port_up {
        state = "partial".to_string();
        pid = def
            .ports
            .iter()
            .find_map(|port| port_listener_pids(*port).first().copied());
    }

    if managed_externally {
        extras.insert("managedBy".into(), "external".into());
    }

    let ports: Vec<PortStatus> = def
        .ports
        .iter()
        .map(|p| PortStatus {
            port: *p,
            listening: port_listening(*p, ctx),
        })
        .collect();

    // Only report health when we believe this service owns the process.
    let health = if matches!(state.as_str(), "running" | "partial" | "unhealthy") {
        if all_ports_up {
            if check_health_probe(&def.health, PROBE_HTTP_MAX_SECS) {
                "ok".into()
            } else {
                "ports_ok".into()
            }
        } else if check_health_probe(&def.health, PROBE_HTTP_MAX_SECS) {
            "ok".into()
        } else {
            "no_response".into()
        }
    } else {
        "unknown".into()
    };

    if (state == "running" || state == "partial") && health == "no_response" {
        state = "unhealthy".to_string();
    }

    if let Some(op) = pending_op(&rt.service_key) {
        match op.as_str() {
            "starting" | "restarting" => {
                if state == "stopped" || state == "stale_pid" || state == "unhealthy" || state == "partial" || state == "port_conflict" {
                    state = "starting".to_string();
                }
            }
            "stopping" => {
                if state != "stopped" {
                    state = "stopping".to_string();
                }
            }
            _ => {}
        }
    }

    let runtime = pid.and_then(process_runtime);

    ServiceStatus {
        id: rt.service_key.clone(),
        project_id: rt.project_id.clone(),
        service_id: rt.service_id.clone(),
        project_name: rt.project_name.clone(),
        name: def.name.clone(),
        state,
        pid,
        ports,
        health,
        open_url: def.open_url.clone(),
        log_file,
        project_dir: rt.project_dir.clone(),
        started_at: runtime.as_ref().map(|r| r.started_at.clone()),
        uptime_secs: runtime.map(|r| r.uptime_secs),
        last_error: None,
        extras: if extras.is_empty() { None } else { Some(extras) },
    }
}

fn format_port_conflicts(ports: &[u32], port_owners: &HashMap<u32, String>, self_id: &str) -> Option<String> {
    let mut parts = Vec::new();
    for port in ports {
        if let Some(owner) = port_owners.get(port) {
            if owner != self_id {
                parts.push(format!("{} 已被服务「{}」占用", port, owner));
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("；"))
    }
}

fn listening_conflict_ports(rt: &ServiceRuntime) -> Vec<u32> {
    let ctx = ProbeContext {
        listening_ports: collect_listening_ports(),
    };
    rt.service
        .ports
        .iter()
        .copied()
        .filter(|p| port_listening(*p, &ctx))
        .collect()
}

// ── Lifecycle ─────────────────────────────────────────────────────────────────

fn local_timestamp() -> String {
    Command::new("date")
        .arg("+%Y-%m-%d %H:%M:%S")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown-time".into())
}

fn append_log_banner(log_file: &Path, message: &str) {
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_file) {
        let _ = writeln!(file, "\n===== [{}] {} =====", local_timestamp(), message);
        let _ = file.flush();
    }
}

fn pump_stream_to_log<R: Read + Send + 'static>(reader: R, log_path: PathBuf) {
    thread::spawn(move || {
        let mut file = match OpenOptions::new().create(true).append(true).open(&log_path) {
            Ok(f) => f,
            Err(_) => return,
        };
        let buffered = BufReader::new(reader);
        for line in buffered.lines() {
            let Ok(line) = line else { break };
            let _ = writeln!(file, "[{}] {}", local_timestamp(), line);
            let _ = file.flush();
        }
    });
}

fn ensure_data_dir(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn process_group_id(pid: u32) -> Option<u32> {
    let output = Command::new("ps")
        .args(["-o", "pgid=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

fn kill_pid(pid: u32, force: bool) {
    let sig = if force { "-9" } else { "-TERM" };
    // If this pid is its own process-group leader (spawned with process_group(0)),
    // kill the whole tree (npm → concurrently → vite/server).
    if let Some(pgid) = process_group_id(pid) {
        if pgid == pid {
            let _ = Command::new("kill")
                .args([sig, &format!("-{pgid}")])
                .output();
            return;
        }
    }
    let _ = Command::new("kill")
        .args([sig, &pid.to_string()])
        .output();
}

fn kill_port_listeners(ports: &[u32], force: bool) {
    for port in ports {
        for pid in port_listener_pids(*port) {
            kill_pid(pid, force);
            if !force {
                thread::sleep(Duration::from_millis(400));
                if pid_alive(pid) {
                    kill_pid(pid, true);
                }
            }
        }
    }
}

fn spawn_service(rt: &ServiceRuntime) -> Result<u32, String> {
    let def = &rt.service;
    let project_dir = rt.project_dir_path();
    if !project_dir.exists() {
        return Err(format!("项目目录不存在: {}", project_dir.display()));
    }

    if let Some(req) = &def.start.require_path {
        let req_path = PathBuf::from(expand_template(req, &project_dir));
        if !req_path.exists() {
            return Err(format!("依赖路径不存在: {}", req_path.display()));
        }
    }

    run_pre_start(rt)?;

    let pid_file = PathBuf::from(expand_template(&def.pid_file, &project_dir));
    let log_file = PathBuf::from(expand_template(&def.log_file, &project_dir));
    ensure_data_dir(&pid_file)?;
    ensure_data_dir(&log_file)?;

    // Idempotent: already running *and* ready
    if let Some(existing) = read_pid(&pid_file) {
        if pid_alive(existing) {
            let ctx = ProbeContext {
                listening_ports: collect_listening_ports(),
            };
            let ports_ready = def.ports.iter().all(|p| port_listening(*p, &ctx));
            if ports_ready && check_health(&def.health) {
                return Ok(existing);
            }
            // Half-dead tree: tear down before respawn (owned ports only for this service).
            let _ = stop_service_impl(rt, true, PortSweep::AllListeners);
        } else {
            let _ = fs::remove_file(&pid_file);
        }
    }

    // Port conflict guard — every declared port, not only the first.
    let busy = listening_conflict_ports(rt);
    if !busy.is_empty() {
        return Err(format!(
            "端口 {} 已被占用，请先停止冲突进程（试跑不会强杀他人端口）",
            busy
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    let cwd = expand_template(&def.start.cwd, &project_dir);
    append_log_banner(
        &log_file,
        &format!(
            "{} starting via {}",
            rt.display_name,
            expand_template(&def.start.command[0], &project_dir)
        ),
    );

    let path_env = get_augmented_path();
    let raw_program = expand_template(&def.start.command[0], &project_dir);
    let cmd_program = resolve_executable(&raw_program, &path_env);
    let cmd_args: Vec<String> = def.start.command[1..]
        .iter()
        .map(|arg| expand_template(arg, &project_dir))
        .collect();
    let mut cmd = Command::new(&cmd_program);
    cmd.args(&cmd_args)
        .current_dir(&cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("PATH", &path_env)
        .envs(&def.start.env);

    // Put service in its own process group so stop can kill npm + children cleanly,
    // without sharing EVA's process group.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    let mut child = cmd.spawn().map_err(|e| format!("启动失败: {}", e))?;
    if let Some(stdout) = child.stdout.take() {
        pump_stream_to_log(stdout, log_file.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        pump_stream_to_log(stderr, log_file.clone());
    }

    let pid = child.id();
    // Reap in background to avoid zombies; do not kill on drop.
    thread::spawn(move || {
        let _ = child.wait();
    });
    fs::write(&pid_file, pid.to_string()).map_err(|e| e.to_string())?;

    Ok(pid)
}

fn wait_until_ready(rt: &ServiceRuntime, pid: u32) -> Result<(), String> {
    let def = &rt.service;
    let project_dir = rt.project_dir_path();
    let log_file = PathBuf::from(expand_template(&def.log_file, &project_dir));
    let timeout = def.health.timeout_secs;
    let interval = def.health.poll_interval_secs;
    let max_attempts = timeout.div_ceil(interval.max(1));

    for _ in 0..max_attempts {
        if !pid_alive(pid) {
            let tail = tail_log_file(&log_file, 20);
            return Err(format!("进程已退出。最近日志:\n{}", tail));
        }

        let ctx = ProbeContext {
            listening_ports: collect_listening_ports(),
        };
        let ports_ready = def.ports.iter().all(|p| port_listening(*p, &ctx));
        let health_ok = check_health(&def.health);

        // Multi-port apps need every declared port + health (e.g. Vite + API).
        if def.ports.len() > 1 {
            if ports_ready && health_ok {
                return Ok(());
            }
        } else if health_ok {
            return Ok(());
        }

        thread::sleep(Duration::from_secs(interval));
    }

    Err(format!(
        "超时未就绪（{}s），请查看日志: {}",
        timeout,
        log_file.display()
    ))
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' || c == '\u{009b}' {
            while let Some(&next) = chars.peek() {
                if next.is_ascii_alphabetic() {
                    chars.next();
                    break;
                }
                chars.next();
            }
            continue;
        }
        out.push(c);
    }
    strip_orphan_sgr(&out)
}

fn strip_orphan_sgr(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut result = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            let start = i;
            i += 1;
            let mut has_digit = false;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b';') {
                has_digit = true;
                i += 1;
            }
            if has_digit && i < bytes.len() && bytes[i] == b'm' {
                i += 1;
                continue;
            }
            result.push('[');
            i = start + 1;
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    result
}

fn normalize_carriage_returns(text: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch == '\r' {
            current.clear();
        } else if ch == '\n' {
            lines.push(std::mem::take(&mut current));
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines.join("\n")
}

fn normalize_log_text(raw: &str) -> String {
    normalize_carriage_returns(&strip_ansi(raw))
}

fn read_tail_bytes(path: &Path, max_bytes: usize) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let len = file.metadata().map_err(|e| e.to_string())?.len() as usize;
    let start = len.saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(start as u64))
        .map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

fn tail_log_file(path: &Path, lines: usize) -> String {
    if !path.exists() {
        return "(日志文件不存在)".into();
    }
    let raw = match read_tail_bytes(path, 128 * 1024) {
        Ok(s) => s,
        Err(_) => {
            return fs::read_to_string(path).unwrap_or_else(|_| "(无法读取日志)".into());
        }
    };
    let normalized = normalize_log_text(&raw);
    normalized
        .lines()
        .rev()
        .take(lines)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PortSweep {
    /// Normal stop: also kill leftover listeners on cleanup_ports.
    AllListeners,
    /// Probe / safe stop: only kill our pid file / known process group.
    OwnedOnly,
}

fn stop_service_impl(rt: &ServiceRuntime, force: bool, sweep: PortSweep) -> ServiceActionResult {
    let def = &rt.service;
    let project_dir = rt.project_dir_path();
    let pid_file = PathBuf::from(expand_template(&def.pid_file, &project_dir));
    let stop_cfg = def.stop.clone().unwrap_or(StopConfig {
        grace_secs: 8,
        cleanup_ports: def.ports.clone(),
    });

    let mut pids: Vec<u32> = Vec::new();
    if let Some(p) = read_pid(&pid_file) {
        if pid_alive(p) {
            pids.push(p);
        }
    }

    // Never harvest foreign port listeners during OwnedOnly (probe teardown).
    if sweep == PortSweep::AllListeners {
        for port in &stop_cfg.cleanup_ports {
            for p in port_listener_pids(*port) {
                if !pids.contains(&p) {
                    pids.push(p);
                }
            }
        }
    }

    if pids.is_empty() {
        let _ = fs::remove_file(&pid_file);
        if sweep == PortSweep::AllListeners {
            kill_port_listeners(&stop_cfg.cleanup_ports, true);
        }
        return ServiceActionResult {
            success: true,
            message: format!("{} 未在运行", rt.display_name),
        };
    }

    for pid in &pids {
        kill_pid(*pid, force);
    }

    if !force {
        for _ in 0..stop_cfg.grace_secs {
            if pids.iter().all(|p| !pid_alive(*p)) {
                break;
            }
            thread::sleep(Duration::from_secs(1));
        }
        for pid in &pids {
            if pid_alive(*pid) {
                kill_pid(*pid, true);
            }
        }
    }

    let _ = fs::remove_file(&pid_file);

    if sweep == PortSweep::AllListeners {
        // Catch orphaned vite/server children that survived npm exit.
        kill_port_listeners(&stop_cfg.cleanup_ports, true);
    }
    append_log_banner(
        &PathBuf::from(expand_template(&def.log_file, &project_dir)),
        &format!("{} stopped", rt.display_name),
    );

    ServiceActionResult {
        success: true,
        message: format!("{} 已停止", rt.display_name),
    }
}

/// Stop only the process we started for a probe (never kill strangers on the same port).
fn stop_probe_process(rt: &ServiceRuntime, spawned_pid: Option<u32>) {
    let project_dir = rt.project_dir_path();
    let pid_file = PathBuf::from(expand_template(&rt.service.pid_file, &project_dir));

    let mut roots: Vec<u32> = Vec::new();
    if let Some(p) = spawned_pid {
        if pid_alive(p) {
            roots.push(p);
        }
    }
    if let Some(p) = read_pid(&pid_file) {
        if pid_alive(p) && !roots.contains(&p) {
            roots.push(p);
        }
    }

    for pid in &roots {
        kill_pid(*pid, true);
        // Brief wait then escalate if needed (kill_pid already uses -9 when force).
        thread::sleep(Duration::from_millis(200));
        if pid_alive(*pid) {
            kill_pid(*pid, true);
        }
    }
    let _ = fs::remove_file(&pid_file);
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceActionResult {
    pub success: bool,
    pub message: String,
}

// ── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn service_list(app: AppHandle) -> Result<Vec<ServiceMeta>, String> {
    let config = load_config(&app)?;
    Ok(iter_runtimes(&config)
        .map(|rt| ServiceMeta {
            id: rt.service_key.clone(),
            project_id: rt.project_id.clone(),
            service_id: rt.service_id.clone(),
            project_name: rt.project_name.clone(),
            name: rt.service.name.clone(),
            ports: rt.service.ports.clone(),
            open_url: rt.service.open_url.clone(),
            project_dir: rt.project_dir.clone(),
        })
        .collect())
}

#[tauri::command]
pub async fn service_status(
    app: AppHandle,
    id: Option<String>,
) -> Result<Vec<ServiceStatus>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let config = load_config(&app)?;
        let ctx = ProbeContext {
            listening_ports: collect_listening_ports(),
        };
        let port_owners = build_port_owners(&config);
        if let Some(sid) = id {
            let rt = find_runtime(&config, &sid)?;
            return Ok(vec![probe_service(&rt, &ctx, &port_owners)]);
        }
        Ok(iter_runtimes(&config)
            .map(|rt| probe_service(&rt, &ctx, &port_owners))
            .collect())
    })
    .await
    .map_err(|e| format!("状态查询中断: {}", e))?
}

#[tauri::command]
pub fn service_start(app: AppHandle, id: String) -> Result<ServiceActionResult, String> {
    let config = load_config(&app)?;
    let rt = find_runtime(&config, &id)?;
    if let Some(op) = pending_op(&rt.service_key) {
        return Ok(ServiceActionResult {
            success: false,
            message: format!("{} 正在{}，请稍候", rt.display_name, pending_op_label(&op)),
        });
    }
    let name = rt.display_name.clone();
    set_pending(&rt.service_key, "starting");
    run_start_and_wait(app, rt, "starting".into());
    Ok(ServiceActionResult {
        success: true,
        message: format!("{} 正在启动，请稍候…", name),
    })
}

#[tauri::command]
pub fn service_stop(app: AppHandle, id: String, force: Option<bool>) -> Result<ServiceActionResult, String> {
    let config = load_config(&app)?;
    let rt = find_runtime(&config, &id)?;
    if let Some(op) = pending_op(&rt.service_key) {
        return Ok(ServiceActionResult {
            success: false,
            message: format!("{} 正在{}，请稍候", rt.display_name, pending_op_label(&op)),
        });
    }
    if force.unwrap_or(false) {
        let result = stop_service_impl(&rt, true, PortSweep::AllListeners);
        return Ok(result);
    }
    let name = rt.display_name.clone();
    set_pending(&rt.service_key, "stopping");
    run_stop_async(app, rt);
    Ok(ServiceActionResult {
        success: true,
        message: format!("{} 正在停止…", name),
    })
}

#[tauri::command]
pub fn service_restart(app: AppHandle, id: String) -> Result<ServiceActionResult, String> {
    let config = load_config(&app)?;
    let rt = find_runtime(&config, &id)?;
    if let Some(op) = pending_op(&rt.service_key) {
        return Ok(ServiceActionResult {
            success: false,
            message: format!("{} 正在{}，请稍候", rt.display_name, pending_op_label(&op)),
        });
    }
    let name = rt.display_name.clone();
    set_pending(&rt.service_key, "restarting");
    run_restart_async(app, rt);
    Ok(ServiceActionResult {
        success: true,
        message: format!("{} 正在重启，请稍候…", name),
    })
}

fn pending_op_label(op: &str) -> &'static str {
    match op {
        "starting" => "启动",
        "stopping" => "停止",
        "restarting" => "重启",
        _ => "处理",
    }
}

#[tauri::command]
pub fn service_open(app: AppHandle, id: String) -> Result<(), String> {
    let config = load_config(&app)?;
    let rt = find_runtime(&config, &id)?;
    let ctx = ProbeContext {
        listening_ports: collect_listening_ports(),
    };
    let port_owners = build_port_owners(&config);
    let status = probe_service(&rt, &ctx, &port_owners);
    let can_open = matches!(status.state.as_str(), "running" | "partial")
        && matches!(status.health.as_str(), "ok" | "ports_ok")
        || (status.ports.iter().any(|p| p.listening) && status.state != "port_conflict");
    if !can_open {
        return Err(format!("{} 未运行，请先启动", rt.display_name));
    }
    open::that(&rt.service.open_url).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledIdes {
    pub cursor: bool,
    pub antigravity: bool,
}

fn path_exists(p: &str) -> bool {
    Path::new(p).exists()
}

fn spawn_detached(bin: &str, args: &[&str]) -> Result<(), String> {
    Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("启动失败: {}", e))
}

fn cursor_bins() -> &'static [&'static str] {
    &[
        "/usr/local/bin/cursor",
        "/opt/homebrew/bin/cursor",
        "/Applications/Cursor.app/Contents/Resources/app/bin/cursor",
        "/Applications/Cursor.app/Contents/Resources/app/bin/code",
    ]
}

fn antigravity_bins() -> &'static [&'static str] {
    &[
        "/Applications/Antigravity IDE.app/Contents/Resources/app/bin/antigravity-ide",
        "/usr/local/bin/antigravity-ide",
        "/opt/homebrew/bin/antigravity-ide",
        "/usr/local/bin/antigravity",
        "/opt/homebrew/bin/antigravity",
    ]
}

fn cursor_installed() -> bool {
    path_exists("/Applications/Cursor.app") || cursor_bins().iter().any(|p| path_exists(p))
}

fn antigravity_installed() -> bool {
    path_exists("/Applications/Antigravity IDE.app")
        || path_exists("/Applications/Antigravity.app")
        || antigravity_bins().iter().any(|p| path_exists(p))
}

fn open_with_bins(bins: &[&str], path: &str, app_names: &[&str]) -> Result<(), String> {
    for bin in bins {
        if path_exists(bin) && spawn_detached(bin, &[path]).is_ok() {
            return Ok(());
        }
    }
    for app in app_names {
        if spawn_detached("open", &["-a", app, path]).is_ok() {
            return Ok(());
        }
    }
    Err("未找到可用的 IDE".into())
}

#[tauri::command]
pub fn service_detect_ides() -> InstalledIdes {
    InstalledIdes {
        cursor: cursor_installed(),
        antigravity: antigravity_installed(),
    }
}

#[tauri::command]
pub fn service_open_in_ide(path: String, ide: String) -> Result<(), String> {
    if path.contains("..") || !Path::new(&path).is_absolute() {
        return Err("非法项目路径".into());
    }
    if !Path::new(&path).exists() {
        return Err(format!("路径不存在: {}", path));
    }
    match ide.as_str() {
        "cursor" => open_with_bins(cursor_bins(), &path, &["Cursor"]),
        "antigravity" => open_with_bins(
            antigravity_bins(),
            &path,
            &["Antigravity IDE", "Antigravity"],
        ),
        _ => Err(format!("暂不支持的 IDE: {}", ide)),
    }
}

#[tauri::command]
pub fn service_tail_log(app: AppHandle, id: String, lines: Option<u32>) -> Result<String, String> {
    let config = load_config(&app)?;
    let rt = find_runtime(&config, &id)?;
    let project_dir = rt.project_dir_path();
    let log_file = PathBuf::from(expand_template(&rt.service.log_file, &project_dir));
    let n = lines.unwrap_or(30) as usize;
    Ok(tail_log_file(&log_file, n))
}

fn save_config(app: &AppHandle, config: &ServicesConfig) -> Result<(), String> {
    fs::create_dir_all(agentdeck_dir()).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(config_path(app), json).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn service_pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let folder = rfd::FileDialog::new()
            .set_title("选择项目目录")
            .pick_folder();
        let _ = tx.send(folder.map(|p| p.to_string_lossy().into_owned()));
    })
    .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn service_scan_project(
    app: AppHandle,
    project_dir: String,
) -> Result<crate::service_discovery::CandidateConfig, String> {
    let path = PathBuf::from(&project_dir);
    if !path.is_absolute() || project_dir.contains("..") {
        return Err("非法项目路径".into());
    }
    let mut candidate = crate::service_discovery::scan_project(&path)?;
    let config = load_config(&app)?;
    let owners = build_port_owners(&config);
    let candidate_key = make_service_key(&candidate.project.id, &candidate.service.id);
    if let Some(msg) = format_port_conflicts(&candidate.service.ports, &owners, &candidate_key) {
        candidate.warnings.push(format!("端口冲突风险：{}。请改端口或先停掉占用方后再试跑。", msg));
        candidate.confidence = (candidate.confidence * 0.5).min(0.4);
    }
    let rt = ServiceRuntime::from_registration(&ServiceRegistrationPayload {
        project: candidate.project.clone(),
        service: candidate.service.clone(),
    });
    let busy = listening_conflict_ports(&rt);
    if !busy.is_empty() {
        candidate.warnings.push(format!(
            "当前已有进程监听端口 {}，试跑会被拒绝（不会强杀）",
            busy
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Ok(candidate)
}

/// Try-start a candidate on a worker thread (keeps UI responsive), then stop only our process.
#[tauri::command]
pub async fn service_probe_candidate(
    app: AppHandle,
    payload: ServiceRegistrationPayload,
    timeout_secs: Option<u64>,
    attempt: Option<u32>,
) -> Result<crate::service_discovery::ProbeReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        probe_candidate_sync(app, payload, timeout_secs, attempt)
    })
    .await
    .map_err(|e| format!("试跑任务中断: {}", e))?
}

pub(crate) fn probe_candidate_sync(
    app: AppHandle,
    payload: ServiceRegistrationPayload,
    timeout_secs: Option<u64>,
    attempt: Option<u32>,
) -> Result<crate::service_discovery::ProbeReport, String> {
    use crate::service_discovery::{prepare_probe_definition, ProbePortStatus, ProbeReport};
    use std::time::Instant;

    if payload.project.id.trim().is_empty() {
        return Err("项目 id 不能为空".into());
    }
    if payload.service.id.trim().is_empty() {
        return Err("服务 id 不能为空".into());
    }
    if payload.service.start.command.is_empty() {
        return Err("启动命令不能为空".into());
    }
    if payload.project.project_dir.contains("..")
        || !Path::new(&payload.project.project_dir).is_absolute()
    {
        return Err("非法项目路径".into());
    }

    let timeout = timeout_secs.unwrap_or(35).clamp(15, 90);
    let attempt_n = attempt.unwrap_or(1);
    let original_key = make_service_key(&payload.project.id, &payload.service.id);

    let mut probe_payload = payload.clone();
    probe_payload.service = prepare_probe_definition(probe_payload.service, timeout);
    probe_payload.service.id = format!("probe-{}", payload.service.id);

    let rt = ServiceRuntime::from_registration(&probe_payload);
    let def = &rt.service;

    let started = Instant::now();
    let project_dir = rt.project_dir_path();
    let log_file = PathBuf::from(expand_template(&def.log_file, &project_dir));

    let config = load_config(&app)?;
    let owners = build_port_owners(&config);
    if let Some(msg) = format_port_conflicts(&def.ports, &owners, &original_key) {
        return Ok(ProbeReport {
            success: false,
            message: format!("端口与已注册服务冲突：{}。请修改端口后再试。", msg),
            state: "port_conflict".into(),
            health: "unknown".into(),
            ports: def
                .ports
                .iter()
                .map(|p| ProbePortStatus {
                    port: *p,
                    listening: false,
                })
                .collect(),
            log_tail: None,
            elapsed_ms: started.elapsed().as_millis() as u64,
            attempt: attempt_n,
        });
    }

    let spawn_result = spawn_service(&rt);
    let spawned_pid = spawn_result.as_ref().ok().copied();
    let report = match spawn_result {
        Ok(pid) => match wait_until_ready(&rt, pid) {
            Ok(()) => {
                let ctx = ProbeContext {
                    listening_ports: collect_listening_ports(),
                };
                let ports: Vec<ProbePortStatus> = def
                    .ports
                    .iter()
                    .map(|p| ProbePortStatus {
                        port: *p,
                        listening: port_listening(*p, &ctx),
                    })
                    .collect();
                let health = if check_health(&def.health) {
                    "ok"
                } else if ports.iter().all(|p| p.listening) {
                    "ports_ok"
                } else {
                    "no_response"
                };
                ProbeReport {
                    success: true,
                    message: "试启动成功，服务已就绪".into(),
                    state: "running".into(),
                    health: health.into(),
                    ports,
                    log_tail: Some(tail_log_file(&log_file, 40)),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    attempt: attempt_n,
                }
            }
            Err(e) => {
                let ctx = ProbeContext {
                    listening_ports: collect_listening_ports(),
                };
                let ports: Vec<ProbePortStatus> = def
                    .ports
                    .iter()
                    .map(|p| ProbePortStatus {
                        port: *p,
                        listening: port_listening(*p, &ctx),
                    })
                    .collect();
                ProbeReport {
                    success: false,
                    message: e,
                    state: "unhealthy".into(),
                    health: "no_response".into(),
                    ports,
                    log_tail: Some(tail_log_file(&log_file, 80)),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    attempt: attempt_n,
                }
            }
        },
        Err(e) => {
            let ctx = ProbeContext {
                listening_ports: collect_listening_ports(),
            };
            ProbeReport {
                success: false,
                message: e,
                state: "stopped".into(),
                health: "unknown".into(),
                ports: def
                    .ports
                    .iter()
                    .map(|p| ProbePortStatus {
                        port: *p,
                        listening: port_listening(*p, &ctx),
                    })
                    .collect(),
                log_tail: if log_file.exists() {
                    Some(tail_log_file(&log_file, 40))
                } else {
                    None
                },
                elapsed_ms: started.elapsed().as_millis() as u64,
                attempt: attempt_n,
            }
        }
    };

    stop_probe_process(&rt, spawned_pid);

    Ok(report)
}

#[tauri::command]
pub fn service_upsert(
    app: AppHandle,
    payload: ServiceRegistrationPayload,
) -> Result<ServiceActionResult, String> {
    if payload.project.id.trim().is_empty() {
        return Err("项目 id 不能为空".into());
    }
    if payload.service.id.trim().is_empty() {
        return Err("服务 id 不能为空".into());
    }
    if payload.service.id.starts_with("probe-") {
        return Err("非法服务 id".into());
    }
    if payload.service.start.command.is_empty() {
        return Err("启动命令不能为空".into());
    }
    if payload.project.project_dir.contains("..")
        || !Path::new(&payload.project.project_dir).is_absolute()
    {
        return Err("非法项目路径".into());
    }
    if !Path::new(&payload.project.project_dir).is_dir() {
        return Err(format!("项目目录不存在: {}", payload.project.project_dir));
    }

    let mut config = load_config(&app)?;
    let mut service = payload.service;
    if service.pid_file.contains("eva-probe") || service.pid_file.contains("agentdeck-probe") {
        service.pid_file = format!("{{projectDir}}/data/{}.pid", service.id);
    }
    if service.log_file.contains("eva-probe") || service.log_file.contains("agentdeck-probe") {
        service.log_file = format!("{{projectDir}}/data/{}.log", service.id);
    }
    if service.health.timeout_secs < 30 {
        service.health.timeout_secs = 90;
    }
    if let Some(stop) = service.stop.as_mut() {
        if stop.cleanup_ports.is_empty() {
            stop.cleanup_ports = service.ports.clone();
        }
    }

    let service_key = make_service_key(&payload.project.id, &service.id);
    let owners = build_port_owners(&config);
    let conflict = format_port_conflicts(&service.ports, &owners, &service_key);

    let name = service.name.clone();
    if let Some(project) = config.projects.iter_mut().find(|p| p.id == payload.project.id) {
        project.name = payload.project.name;
        project.project_dir = payload.project.project_dir;
        if let Some(existing) = project.services.iter_mut().find(|s| s.id == service.id) {
            *existing = service;
        } else {
            project.services.push(service);
        }
    } else {
        config.projects.push(ProjectDefinition {
            id: payload.project.id,
            name: payload.project.name,
            project_dir: payload.project.project_dir,
            services: vec![service],
        });
    }

    save_config(&app, &config)?;
    let mut message = format!("已保存服务 {}", name);
    if let Some(c) = conflict {
        message.push_str(&format!("（警告：{}）", c));
    }
    Ok(ServiceActionResult {
        success: true,
        message,
    })
}

#[tauri::command]
pub fn service_rename_project(
    app: AppHandle,
    project_id: String,
    name: String,
) -> Result<ServiceActionResult, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("项目名不能为空".into());
    }
    let mut config = load_config(&app)?;
    let project = config
        .projects
        .iter_mut()
        .find(|p| p.id == project_id)
        .ok_or_else(|| format!("未知项目: {}", project_id))?;
    project.name = name.clone();
    save_config(&app, &config)?;
    Ok(ServiceActionResult {
        success: true,
        message: format!("项目名已更新为 {}", name),
    })
}

#[tauri::command]
pub fn service_rename_service(
    app: AppHandle,
    id: String,
    name: String,
) -> Result<ServiceActionResult, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("服务名不能为空".into());
    }
    let mut config = load_config(&app)?;
    let (project_id, service_id) =
        parse_service_key(&id).ok_or_else(|| format!("无效服务键: {}", id))?;
    let project = config
        .projects
        .iter_mut()
        .find(|p| p.id == project_id)
        .ok_or_else(|| format!("未知服务: {}", id))?;
    let service = project
        .services
        .iter_mut()
        .find(|s| s.id == service_id)
        .ok_or_else(|| format!("未知服务: {}", id))?;
    service.name = name.clone();
    save_config(&app, &config)?;
    Ok(ServiceActionResult {
        success: true,
        message: format!("服务名已更新为 {}", name),
    })
}

/// Stop (if needed) then remove the service from services.json.
#[tauri::command]
pub async fn service_remove(app: AppHandle, id: String) -> Result<ServiceActionResult, String> {
    tauri::async_runtime::spawn_blocking(move || service_remove_sync(app, id))
        .await
        .map_err(|e| format!("删除任务中断: {}", e))?
}

fn service_remove_sync(app: AppHandle, id: String) -> Result<ServiceActionResult, String> {
    if id.trim().is_empty() || id.starts_with("probe-") {
        return Err("非法服务 id".into());
    }
    if let Some(op) = pending_op(&id) {
        return Ok(ServiceActionResult {
            success: false,
            message: format!("服务正在{}，请稍候再删除", pending_op_label(&op)),
        });
    }

    let mut config = load_config(&app)?;
    let rt = find_runtime(&config, &id)?;
    let name = rt.display_name.clone();

    let _ = stop_service_impl(&rt, true, PortSweep::AllListeners);
    clear_pending(&rt.service_key);

    let project_idx = config
        .projects
        .iter()
        .position(|p| p.id == rt.project_id)
        .ok_or_else(|| format!("未知服务: {}", id))?;
    config.projects[project_idx]
        .services
        .retain(|s| s.id != rt.service_id);
    if config.projects[project_idx].services.is_empty() {
        config.projects.remove(project_idx);
    }
    save_config(&app, &config)?;

    Ok(ServiceActionResult {
        success: true,
        message: format!("已删除服务 {}", name),
    })
}

// ── HTTP / headless API wrappers ─────────────────────────────────────────────

pub fn api_status(app: &AppHandle, id: Option<String>) -> Result<Vec<ServiceStatus>, String> {
    let config = load_config(app)?;
    let ctx = ProbeContext {
        listening_ports: collect_listening_ports(),
    };
    let port_owners = build_port_owners(&config);
    if let Some(sid) = id {
        let rt = find_runtime(&config, &sid)?;
        return Ok(vec![probe_service(&rt, &ctx, &port_owners)]);
    }
    Ok(iter_runtimes(&config)
        .map(|rt| probe_service(&rt, &ctx, &port_owners))
        .collect())
}

pub fn api_projects(app: &AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let config = load_config(app)?;
    let statuses = api_status(app, None)?;
    let mut out = Vec::new();
    for project in &config.projects {
        let services: Vec<&ServiceStatus> = statuses
            .iter()
            .filter(|s| s.project_id == project.id)
            .collect();
        let running = services
            .iter()
            .filter(|s| matches!(s.state.as_str(), "running" | "partial" | "starting" | "stopping" | "unhealthy"))
            .count();
        out.push(serde_json::json!({
            "id": project.id,
            "name": project.name,
            "projectDir": project.project_dir,
            "serviceCount": project.services.len(),
            "runningCount": running,
            "services": services.iter().map(|s| serde_json::json!({
                "id": s.id,
                "serviceId": s.service_id,
                "name": s.name,
                "state": s.state,
                "health": s.health,
                "ports": s.ports,
                "openUrl": s.open_url,
            })).collect::<Vec<_>>(),
        }));
    }
    Ok(out)
}

pub fn api_start(app: AppHandle, id: String) -> Result<ServiceActionResult, String> {
    service_start(app, id)
}

pub fn api_stop(app: AppHandle, id: String, force: bool) -> Result<ServiceActionResult, String> {
    service_stop(app, id, Some(force))
}

pub fn api_restart(app: AppHandle, id: String) -> Result<ServiceActionResult, String> {
    service_restart(app, id)
}

pub fn api_open(app: &AppHandle, id: &str) -> Result<(), String> {
    service_open(app.clone(), id.to_string())
}

pub fn api_tail_log(app: &AppHandle, id: &str, lines: Option<u32>) -> Result<String, String> {
    service_tail_log(app.clone(), id.to_string(), lines)
}

pub fn api_upsert(app: &AppHandle, payload: ServiceRegistrationPayload) -> Result<ServiceActionResult, String> {
    service_upsert(app.clone(), payload)
}

pub fn api_rename_project(
    app: &AppHandle,
    project_id: String,
    name: String,
) -> Result<ServiceActionResult, String> {
    service_rename_project(app.clone(), project_id, name)
}

pub fn api_rename_service(
    app: &AppHandle,
    id: String,
    name: String,
) -> Result<ServiceActionResult, String> {
    service_rename_service(app.clone(), id, name)
}

pub fn api_remove(app: AppHandle, id: String, _force: bool) -> Result<ServiceActionResult, String> {
    // Always force-stop on remove so agents don't leave orphans.
    service_remove_sync(app, id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_template_replaces_project_dir() {
        let project = PathBuf::from("/tmp/voxlab");
        let out = expand_template("{projectDir}/data/x.pid", &project);
        assert_eq!(out, "/tmp/voxlab/data/x.pid");
    }

    #[test]
    fn normalize_carriage_returns_collapses_tqdm() {
        let raw = "INFO start\n\rGenerating: 0%|\rGenerating: 50%|====\rGenerating: 100%|====|\nDONE\n";
        let out = normalize_log_text(raw);
        assert!(out.contains("INFO start"));
        assert!(out.contains("Generating: 100%|====|"));
        assert!(!out.contains("Generating: 0%"));
        assert!(out.contains("DONE"));
    }

    #[test]
    fn parse_ps_lstart_etime_line() {
        let line = "Tue Aug 18 11:27:48 2026 10:28";
        let parts: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(parts.len(), 6);
        assert_eq!(parse_etime(parts.last().unwrap()).unwrap(), 10 * 60 + 28);
        assert_eq!(
            format_lstart(&parts[..parts.len() - 1].join(" ")).unwrap(),
            "2026-08-18 11:27:48"
        );
    }

    #[test]
    fn format_lstart_december() {
        assert_eq!(
            format_lstart("Wed Dec  3 12:33:12 2005").unwrap(),
            "2005-12-03 12:33:12"
        );
    }

    #[test]
    fn parse_etime_formats() {
        assert_eq!(parse_etime("45").unwrap(), 45);
        assert_eq!(parse_etime("10:28").unwrap(), 628);
        assert_eq!(parse_etime("1:23:45").unwrap(), 5025);
        assert_eq!(parse_etime("2-03:04:05").unwrap(), 2 * 86400 + 3 * 3600 + 4 * 60 + 5);
    }

    #[test]
    fn strip_orphan_sgr_codes() {
        let raw = "[94mVoice:[0m hello";
        assert_eq!(strip_ansi(raw), "Voice: hello");
    }

    #[test]
    fn format_port_conflicts_skips_self() {
        let mut owners = HashMap::new();
        owners.insert(3001, "repomind/main".into());
        owners.insert(3000, "repomind/main".into());
        assert!(format_port_conflicts(&[3001], &owners, "repomind/main").is_none());
        let msg = format_port_conflicts(&[3001], &owners, "beigushi/main").unwrap();
        assert!(msg.contains("repomind/main"));
        assert!(msg.contains("3001"));
    }

    #[test]
    fn default_config_contains_miloco() {
        let home = PathBuf::from("/Users/test");
        let cfg = default_config(&home);
        assert!(cfg.projects.iter().any(|p| p.id == "miloco"));
        let miloco = cfg.projects.iter().find(|p| p.id == "miloco").unwrap();
        assert_eq!(miloco.services[0].ports, vec![1810]);
        assert_eq!(make_service_key("voxlab", "main"), "voxlab/main");
    }
}
