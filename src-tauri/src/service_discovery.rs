//! Deterministic project scan → CandidateConfig for local service registration.

use crate::service_config::{
    slugify, HealthConfig, ProjectUpsertMeta, ServiceDefinition, StartConfig, StopConfig,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateConfig {
    pub project: ProjectUpsertMeta,
    pub service: ServiceDefinition,
    pub confidence: f32,
    pub evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub recipe: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeReport {
    pub success: bool,
    pub message: String,
    pub state: String,
    pub health: String,
    pub ports: Vec<ProbePortStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_tail: Option<String>,
    pub elapsed_ms: u64,
    pub attempt: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbePortStatus {
    pub port: u32,
    pub listening: bool,
}

fn service_id_from_script(script_name: &str) -> String {
    if script_name == "dev" || script_name == "start" {
        "main".into()
    } else {
        slugify(script_name)
    }
}

fn runtime_files(service_id: &str) -> (String, String) {
    (
        format!("{{projectDir}}/data/{}.pid", service_id),
        format!("{{projectDir}}/data/{}.log", service_id),
    )
}

/// Scan a project directory and propose a project + service definition.
pub fn scan_project(project_dir: &Path) -> Result<CandidateConfig, String> {
    if !project_dir.is_dir() {
        return Err(format!("不是有效目录: {}", project_dir.display()));
    }

    let abs = fs::canonicalize(project_dir).unwrap_or_else(|_| project_dir.to_path_buf());
    let abs_str = abs.to_string_lossy().into_owned();
    let folder_name = abs
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "service".into());
    let project_id = slugify(&folder_name);
    let project = ProjectUpsertMeta {
        id: project_id.clone(),
        name: folder_name.clone(),
        project_dir: abs_str.clone(),
    };

    let mut evidence = Vec::new();
    let mut warnings = Vec::new();

    if let Some(candidate) =
        try_scan_node(&project, &folder_name, &abs_str, &mut evidence, &mut warnings)
    {
        return Ok(candidate);
    }
    if let Some(candidate) =
        try_scan_python(&project, &folder_name, &abs_str, &mut evidence, &mut warnings)
    {
        return Ok(candidate);
    }
    if let Some(candidate) =
        try_scan_compose(&project, &folder_name, &abs_str, &mut evidence, &mut warnings)
    {
        return Ok(candidate);
    }

    evidence.push("未匹配已知模板，生成占位配置".into());
    warnings.push("请手动确认启动命令与端口".into());
    let service_id = "main".to_string();
    let (pid_file, log_file) = runtime_files(&service_id);
    Ok(build_candidate(
        project,
        ServiceDefinition {
            id: service_id,
            name: folder_name,
            start: StartConfig {
                command: vec!["npm".into(), "run".into(), "dev".into()],
                cwd: "{projectDir}".into(),
                env: HashMap::new(),
                require_path: None,
            },
            pre_start: None,
            pid_file,
            log_file,
            ports: vec![3000],
            health: HealthConfig {
                url: "http://localhost:3000/".into(),
                contains: None,
                status_ok: true,
                fallback_urls: vec![],
                accept_http_codes: vec!["200".into(), "404".into()],
                timeout_secs: 60,
                poll_interval_secs: 2,
            },
            open_url: "http://localhost:3000".into(),
            stop: Some(StopConfig {
                grace_secs: 8,
                cleanup_ports: vec![3000],
            }),
        },
        0.25,
        evidence,
        warnings,
        "generic",
    ))
}

fn try_scan_node(
    project: &ProjectUpsertMeta,
    name: &str,
    abs_str: &str,
    evidence: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> Option<CandidateConfig> {
    let root = Path::new(abs_str);
    let pkg_path = root.join("package.json");
    if !pkg_path.is_file() {
        return None;
    }
    let raw = fs::read_to_string(&pkg_path).ok()?;
    let pkg: Value = serde_json::from_str(&raw).ok()?;
    evidence.push("package.json".into());

    let scripts = pkg.get("scripts")?.as_object()?;
    let script_name = ["dev", "start", "serve", "preview"]
        .iter()
        .find(|s| scripts.contains_key(**s))
        .copied()
        .unwrap_or("dev");
    if scripts.contains_key(script_name) {
        evidence.push(format!("scripts.{}", script_name));
    } else {
        warnings.push(format!(
            "package.json 无 {} script，仍尝试 npm run {}",
            script_name, script_name
        ));
    }

    let script_body = scripts
        .get(script_name)
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let mut ports = extract_ports_from_text(script_body);
    for rel in ["vite.config.ts", "vite.config.js", "vite.config.mjs", ".env", ".env.local"] {
        if let Ok(txt) = fs::read_to_string(root.join(rel)) {
            let found = extract_ports_from_text(&txt);
            if !found.is_empty() {
                evidence.push(rel.into());
                for p in found {
                    if !ports.contains(&p) {
                        ports.push(p);
                    }
                }
            }
        }
    }

    let looks_dual = script_body.contains("concurrently")
        || (script_body.contains("vite")
            && (script_body.contains("server") || script_body.contains("3001")));
    if looks_dual && ports.len() < 2 {
        if !ports.contains(&3000) {
            ports.push(3000);
        }
        if !ports.contains(&3001) {
            ports.push(3001);
        }
        evidence.push("双进程脚本启发式（Vite + API）".into());
    }

    if ports.is_empty() {
        ports.push(3000);
        warnings.push("未检测到端口，默认 3000".into());
    }
    ports.sort_unstable();
    ports.dedup();

    let health_port = if ports.len() > 1 {
        *ports
            .iter()
            .find(|&&p| p != 3000 && p != 5173)
            .unwrap_or(&ports[ports.len() - 1])
    } else {
        ports[0]
    };
    let open_port = ports
        .iter()
        .copied()
        .find(|p| *p == 3000 || *p == 5173 || *p == 8080)
        .unwrap_or(ports[0]);

    let accept = vec!["200".into(), "404".into()];
    if ports.len() > 1 {
        warnings.push("多端口服务：健康检查指向疑似 API 端口".into());
    }

    let confidence = if scripts.contains_key(script_name) {
        if ports.len() > 1 {
            0.75
        } else {
            0.85
        }
    } else {
        0.4
    };

    let service_id = service_id_from_script(script_name);
    let (pid_file, log_file) = runtime_files(&service_id);

    Some(build_candidate(
        project.clone(),
        ServiceDefinition {
            id: service_id,
            name: display_name_from_pkg(&pkg, name),
            start: StartConfig {
                command: vec!["npm".into(), "run".into(), script_name.into()],
                cwd: "{projectDir}".into(),
                env: HashMap::new(),
                require_path: None,
            },
            pre_start: None,
            pid_file,
            log_file,
            ports: ports.clone(),
            health: HealthConfig {
                url: format!("http://localhost:{}/", health_port),
                contains: None,
                status_ok: true,
                fallback_urls: vec![],
                accept_http_codes: accept,
                timeout_secs: 90,
                poll_interval_secs: 2,
            },
            open_url: format!("http://localhost:{}", open_port),
            stop: Some(StopConfig {
                grace_secs: 8,
                cleanup_ports: ports,
            }),
        },
        confidence,
        evidence.clone(),
        warnings.clone(),
        "node",
    ))
}

fn try_scan_python(
    project: &ProjectUpsertMeta,
    name: &str,
    abs_str: &str,
    evidence: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> Option<CandidateConfig> {
    let root = Path::new(abs_str);
    let has_pyproject = root.join("pyproject.toml").is_file();
    let has_requirements = root.join("requirements.txt").is_file();
    let main_py = ["main.py", "app.py", "server.py", "run.py"]
        .iter()
        .map(|f| root.join(f))
        .find(|p| p.is_file());

    if !has_pyproject && !has_requirements && main_py.is_none() {
        return None;
    }

    if has_pyproject {
        evidence.push("pyproject.toml".into());
    }
    if has_requirements {
        evidence.push("requirements.txt".into());
    }

    let venv_python = ["backend/.venv/bin/python", ".venv/bin/python", "venv/bin/python"]
        .iter()
        .map(|rel| root.join(rel))
        .find(|p| p.is_file());

    let (python, require_path, cwd) = if let Some(ref venv) = venv_python {
        evidence.push(format!(
            "venv: {}",
            venv.strip_prefix(root).unwrap_or(venv).display()
        ));
        let cwd = if venv.to_string_lossy().contains("backend/") {
            "{projectDir}/backend".into()
        } else {
            "{projectDir}".into()
        };
        (
            venv.to_string_lossy().into_owned(),
            Some(
                venv.to_string_lossy()
                    .into_owned()
                    .replace(abs_str, "{projectDir}"),
            ),
            cwd,
        )
    } else {
        warnings.push("未找到 .venv，使用系统 python3".into());
        ("python3".into(), None, "{projectDir}".into())
    };

    let mut command = vec![python];
    if let Some(main) = main_py {
        evidence.push(
            main.file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "main.py".into()),
        );
        command.push(
            main.file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "main.py".into()),
        );
    } else {
        warnings.push("未找到 main.py/app.py，请手动指定启动参数".into());
        command.push("main.py".into());
    }

    let mut ports = Vec::new();
    for rel in ["main.py", "app.py", ".env", "README.md"] {
        if let Ok(txt) = fs::read_to_string(root.join(rel)) {
            for p in extract_ports_from_text(&txt) {
                if !ports.contains(&p) {
                    ports.push(p);
                }
            }
        }
    }
    if ports.is_empty() {
        ports.push(8000);
        warnings.push("未检测到端口，默认 8000".into());
    }

    let port = ports[0];
    let service_id = "main".to_string();
    let (pid_file, log_file) = runtime_files(&service_id);

    Some(build_candidate(
        project.clone(),
        ServiceDefinition {
            id: service_id,
            name: name.to_string(),
            start: StartConfig {
                command,
                cwd,
                env: HashMap::new(),
                require_path,
            },
            pre_start: None,
            pid_file,
            log_file,
            ports: vec![port],
            health: HealthConfig {
                url: format!("http://127.0.0.1:{}/health", port),
                contains: None,
                status_ok: true,
                fallback_urls: vec![format!("http://127.0.0.1:{}/", port)],
                accept_http_codes: vec!["200".into()],
                timeout_secs: 90,
                poll_interval_secs: 2,
            },
            open_url: format!("http://localhost:{}", port),
            stop: Some(StopConfig {
                grace_secs: 10,
                cleanup_ports: vec![port],
            }),
        },
        0.65,
        evidence.clone(),
        warnings.clone(),
        "python",
    ))
}

fn try_scan_compose(
    project: &ProjectUpsertMeta,
    name: &str,
    abs_str: &str,
    evidence: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> Option<CandidateConfig> {
    let root = Path::new(abs_str);
    let compose = ["docker-compose.yml", "docker-compose.yaml", "compose.yml"]
        .iter()
        .map(|f| root.join(f))
        .find(|p| p.is_file())?;

    evidence.push(
        compose
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "docker-compose.yml".into()),
    );
    warnings.push("Docker Compose 服务需本机已安装 docker".into());

    let txt = fs::read_to_string(&compose).ok()?;
    let mut ports = extract_ports_from_text(&txt);
    if ports.is_empty() {
        ports.push(8080);
    }
    let port = ports[0];
    let service_id = "main".to_string();
    let (pid_file, log_file) = runtime_files(&service_id);

    Some(build_candidate(
        project.clone(),
        ServiceDefinition {
            id: service_id,
            name: name.to_string(),
            start: StartConfig {
                command: vec!["docker".into(), "compose".into(), "up".into()],
                cwd: "{projectDir}".into(),
                env: HashMap::new(),
                require_path: None,
            },
            pre_start: None,
            pid_file,
            log_file,
            ports: vec![port],
            health: HealthConfig {
                url: format!("http://localhost:{}/", port),
                contains: None,
                status_ok: true,
                fallback_urls: vec![],
                accept_http_codes: vec!["200".into(), "404".into()],
                timeout_secs: 120,
                poll_interval_secs: 3,
            },
            open_url: format!("http://localhost:{}", port),
            stop: Some(StopConfig {
                grace_secs: 15,
                cleanup_ports: vec![port],
            }),
        },
        0.55,
        evidence.clone(),
        warnings.clone(),
        "compose",
    ))
}

fn build_candidate(
    project: ProjectUpsertMeta,
    service: ServiceDefinition,
    confidence: f32,
    evidence: Vec<String>,
    warnings: Vec<String>,
    recipe: &str,
) -> CandidateConfig {
    CandidateConfig {
        project,
        service,
        confidence,
        evidence,
        warnings,
        recipe: recipe.to_string(),
    }
}

fn display_name_from_pkg(pkg: &Value, fallback: &str) -> String {
    pkg.get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn extract_ports_from_text(text: &str) -> Vec<u32> {
    let mut ports = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b':' || bytes[i] == b'=' {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 1 {
                if let Ok(n) = std::str::from_utf8(&bytes[i + 1..j])
                    .unwrap_or("")
                    .parse::<u32>()
                {
                    if is_plausible_dev_port(n) && !ports.contains(&n) {
                        ports.push(n);
                    }
                }
            }
            i = j;
            continue;
        }
        i += 1;
    }

    let lower = text.to_ascii_lowercase();
    for (idx, _) in lower.match_indices("port") {
        let after = lower[idx + 4..]
            .trim_start_matches(|c: char| c == '-' || c == '_' || c.is_whitespace() || c == '=');
        let num: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(n) = num.parse::<u32>() {
            if is_plausible_dev_port(n) && !ports.contains(&n) {
                ports.push(n);
            }
        }
    }

    ports.truncate(6);
    ports
}

fn is_plausible_dev_port(n: u32) -> bool {
    matches!(
        n,
        3000..=3999 | 4000..=4999 | 5000..=5999 | 7000..=7999 | 8000..=8999 | 9000..=9999
    ) || matches!(n, 5173 | 4173 | 1810 | 1420 | 8080 | 8888)
}

/// Prepare a definition for probing: unique pid/log under data/, shorter timeout.
pub fn prepare_probe_definition(mut def: ServiceDefinition, timeout_secs: u64) -> ServiceDefinition {
    def.pid_file = "{projectDir}/data/agentdeck-probe.pid".into();
    def.log_file = "{projectDir}/data/agentdeck-probe.log".into();
    def.health.timeout_secs = timeout_secs.max(15);
    def.health.poll_interval_secs = def.health.poll_interval_secs.max(1).min(3);
    if let Some(stop) = def.stop.as_mut() {
        stop.grace_secs = stop.grace_secs.min(8);
        if stop.cleanup_ports.is_empty() {
            stop.cleanup_ports = def.ports.clone();
        }
    } else {
        def.stop = Some(StopConfig {
            grace_secs: 5,
            cleanup_ports: def.ports.clone(),
        });
    }
    def
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service_config::slugify;

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("Repo Mind"), "repo-mind");
        assert_eq!(slugify("My_App!!"), "my-app");
    }

    #[test]
    fn extract_common_ports() {
        let ports = extract_ports_from_text("vite --port 5173 && api :3001");
        assert!(ports.contains(&5173));
        assert!(ports.contains(&3001));
    }

    #[test]
    fn service_id_from_script_main_for_dev() {
        assert_eq!(service_id_from_script("dev"), "main");
        assert_eq!(service_id_from_script("preview"), "preview");
    }
}
