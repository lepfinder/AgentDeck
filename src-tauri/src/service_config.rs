//! v2 config: Project → multiple Services. Global key = `{projectId}/{serviceId}`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicesConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    pub projects: Vec<ProjectDefinition>,
}

fn default_version() -> u32 {
    2
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDefinition {
    pub id: String,
    pub name: String,
    #[serde(rename = "projectDir")]
    pub project_dir: String,
    pub services: Vec<ServiceDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDefinition {
    pub id: String,
    pub name: String,
    pub start: StartConfig,
    #[serde(rename = "preStart")]
    pub pre_start: Option<PreStartConfig>,
    #[serde(rename = "pidFile")]
    pub pid_file: String,
    #[serde(rename = "logFile")]
    pub log_file: String,
    pub ports: Vec<u32>,
    pub health: HealthConfig,
    #[serde(rename = "openUrl")]
    pub open_url: String,
    pub stop: Option<StopConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartConfig {
    pub command: Vec<String>,
    pub cwd: String,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(rename = "requirePath")]
    pub require_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreStartConfig {
    pub when: String,
    pub command: Vec<String>,
    pub cwd: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    pub url: String,
    #[serde(default)]
    pub contains: Option<String>,
    #[serde(rename = "statusOk", default)]
    pub status_ok: bool,
    #[serde(rename = "fallbackUrls", default)]
    pub fallback_urls: Vec<String>,
    #[serde(rename = "acceptHttpCodes", default)]
    pub accept_http_codes: Vec<String>,
    #[serde(rename = "timeoutSecs", default = "default_health_timeout")]
    pub timeout_secs: u64,
    #[serde(rename = "pollIntervalSecs", default = "default_poll_interval")]
    pub poll_interval_secs: u64,
}

fn default_health_timeout() -> u64 {
    120
}

fn default_poll_interval() -> u64 {
    2
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopConfig {
    #[serde(rename = "graceSecs", default = "default_grace_secs")]
    pub grace_secs: u64,
    #[serde(rename = "cleanupPorts", default)]
    pub cleanup_ports: Vec<u32>,
}

fn default_grace_secs() -> u64 {
    15
}

/// Payload for upsert / probe from the registration wizard.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceRegistrationPayload {
    pub project: ProjectUpsertMeta,
    pub service: ServiceDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUpsertMeta {
    pub id: String,
    pub name: String,
    #[serde(rename = "projectDir")]
    pub project_dir: String,
}

#[derive(Debug, Clone)]
pub struct ServiceRuntime {
    pub service_key: String,
    pub project_id: String,
    pub service_id: String,
    pub project_name: String,
    pub project_dir: String,
    pub display_name: String,
    pub service: ServiceDefinition,
}

impl ServiceRuntime {
    pub fn from_parts(project: &ProjectDefinition, service: &ServiceDefinition) -> Self {
        let service_key = make_service_key(&project.id, &service.id);
        // display_name is for action messages; status API exposes raw service.name separately.
        let display_name = if project.services.len() > 1 && project.name != service.name {
            format!("{} · {}", project.name, service.name)
        } else {
            service.name.clone()
        };
        Self {
            service_key,
            project_id: project.id.clone(),
            service_id: service.id.clone(),
            project_name: project.name.clone(),
            project_dir: project.project_dir.clone(),
            display_name,
            service: service.clone(),
        }
    }

    pub fn project_dir_path(&self) -> PathBuf {
        PathBuf::from(&self.project_dir)
    }

    pub fn from_registration(payload: &ServiceRegistrationPayload) -> Self {
        let project = ProjectDefinition {
            id: payload.project.id.clone(),
            name: payload.project.name.clone(),
            project_dir: payload.project.project_dir.clone(),
            services: vec![payload.service.clone()],
        };
        Self::from_parts(&project, &payload.service)
    }
}

pub fn make_service_key(project_id: &str, service_id: &str) -> String {
    format!("{}/{}", project_id, service_id)
}

pub fn parse_service_key(key: &str) -> Option<(String, String)> {
    key.split_once('/').map(|(p, s)| (p.to_string(), s.to_string()))
}

pub fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

// ── v1 legacy ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ServicesConfigV1 {
    services: Vec<LegacyFlatService>,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyFlatService {
    id: String,
    name: String,
    #[serde(rename = "projectDir")]
    project_dir: String,
    start: StartConfig,
    #[serde(rename = "preStart")]
    pre_start: Option<PreStartConfig>,
    #[serde(rename = "pidFile")]
    pid_file: String,
    #[serde(rename = "logFile")]
    log_file: String,
    ports: Vec<u32>,
    health: HealthConfig,
    #[serde(rename = "openUrl")]
    open_url: String,
    stop: Option<StopConfig>,
}

impl From<LegacyFlatService> for ServiceDefinition {
    fn from(v: LegacyFlatService) -> Self {
        ServiceDefinition {
            id: v.id,
            name: v.name,
            start: v.start,
            pre_start: v.pre_start,
            pid_file: v.pid_file,
            log_file: v.log_file,
            ports: v.ports,
            health: v.health,
            open_url: v.open_url,
            stop: v.stop,
        }
    }
}

pub fn migrate_v1_json(raw: &str) -> Result<ServicesConfig, String> {
    let v1: ServicesConfigV1 =
        serde_json::from_str(raw).map_err(|e| format!("services.json v1 解析失败: {}", e))?;
    Ok(migrate_v1_services(v1.services))
}

pub fn migrate_v1_services(flat: Vec<LegacyFlatService>) -> ServicesConfig {
    let mut groups: HashMap<String, Vec<LegacyFlatService>> = HashMap::new();
    for svc in flat {
        groups.entry(svc.project_dir.clone()).or_default().push(svc);
    }

    let mut projects = Vec::new();
    for (project_dir, mut group) in groups {
        group.sort_by(|a, b| a.id.cmp(&b.id));
        let folder_name = Path::new(&project_dir)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "project".into());

        let (project_id, project_name) = if group.len() == 1 {
            let only = &group[0];
            (only.id.clone(), only.name.clone())
        } else {
            (slugify(&folder_name), folder_name.clone())
        };

        let services: Vec<ServiceDefinition> = if group.len() == 1 {
            let legacy = group.into_iter().next().expect("non-empty group");
            let mut svc = ServiceDefinition::from(legacy);
            svc.id = "main".into();
            vec![svc]
        } else {
            group
                .into_iter()
                .map(|legacy| {
                    let legacy_id = legacy.id.clone();
                    let mut svc = ServiceDefinition::from(legacy);
                    svc.id = derive_service_id(&project_id, &legacy_id);
                    svc
                })
                .collect()
        };

        projects.push(ProjectDefinition {
            id: project_id,
            name: project_name,
            project_dir,
            services,
        });
    }

    projects.sort_by(|a, b| a.name.cmp(&b.name));
    ServicesConfig {
        version: 2,
        projects,
    }
}

fn derive_service_id(project_id: &str, legacy_id: &str) -> String {
    if legacy_id == project_id {
        return "main".into();
    }
    if let Some(rest) = legacy_id.strip_prefix(&format!("{}-", project_id)) {
        if !rest.is_empty() {
            return slugify(rest);
        }
    }
    if let Some(rest) = legacy_id.strip_prefix(&format!("{}_", project_id)) {
        if !rest.is_empty() {
            return slugify(rest);
        }
    }
    if legacy_id.contains('-') {
        return slugify(legacy_id.rsplit('-').next().unwrap_or(legacy_id));
    }
    slugify(legacy_id)
}

pub fn parse_config_json(raw: &str) -> Result<ServicesConfig, String> {
    if let Ok(cfg) = serde_json::from_str::<ServicesConfig>(raw) {
        if cfg.version >= 2 && !cfg.projects.is_empty() {
            return Ok(cfg);
        }
        if cfg.version >= 2 && cfg.projects.is_empty() {
            return Ok(cfg);
        }
    }
    migrate_v1_json(raw)
}

pub fn find_runtime<'a>(config: &'a ServicesConfig, key: &str) -> Result<ServiceRuntime, String> {
    if let Some((pid, sid)) = parse_service_key(key) {
        if let Some(rt) = find_in_project(config, &pid, &sid) {
            return Ok(rt);
        }
    }

    // Legacy: bare project id with single service → project/main or sole service
    if let Some(project) = config.projects.iter().find(|p| p.id == key) {
        if let Some(service) = project.services.iter().find(|s| s.id == "main") {
            return Ok(ServiceRuntime::from_parts(project, service));
        }
        if project.services.len() == 1 {
            return Ok(ServiceRuntime::from_parts(project, &project.services[0]));
        }
    }

    // Legacy: bare service id unique within config
    let mut matches: Vec<ServiceRuntime> = Vec::new();
    for project in &config.projects {
        for service in &project.services {
            if service.id == key {
                matches.push(ServiceRuntime::from_parts(project, service));
            }
        }
    }
    if matches.len() == 1 {
        return Ok(matches.remove(0));
    }

    Err(format!("未知服务: {}", key))
}

fn find_in_project<'a>(
    config: &'a ServicesConfig,
    project_id: &str,
    service_id: &str,
) -> Option<ServiceRuntime> {
    let project = config.projects.iter().find(|p| p.id == project_id)?;
    let service = project.services.iter().find(|s| s.id == service_id)?;
    Some(ServiceRuntime::from_parts(project, service))
}

pub fn iter_runtimes(config: &ServicesConfig) -> impl Iterator<Item = ServiceRuntime> + '_ {
    config.projects.iter().flat_map(|project| {
        project.services.iter().map(move |service| {
            ServiceRuntime::from_parts(project, service)
        })
    })
}

pub fn build_port_owners(config: &ServicesConfig) -> HashMap<u32, String> {
    let mut map = HashMap::new();
    for rt in iter_runtimes(config) {
        for port in &rt.service.ports {
            map.entry(*port).or_insert_with(|| rt.service_key.clone());
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_single_v1_service() {
        let raw = r#"{"services":[{"id":"voxlab","name":"VoxLab","projectDir":"/tmp/VoxLab","start":{"command":["npm","run","dev"],"cwd":"{projectDir}","env":{}},"pidFile":"{projectDir}/data/x.pid","logFile":"{projectDir}/data/x.log","ports":[8001],"health":{"url":"http://localhost:8001/","statusOk":true,"fallbackUrls":[],"acceptHttpCodes":[],"timeoutSecs":90,"pollIntervalSecs":2},"openUrl":"http://localhost:8001"}]}"#;
        let cfg = migrate_v1_json(raw).unwrap();
        assert_eq!(cfg.projects.len(), 1);
        assert_eq!(cfg.projects[0].id, "voxlab");
        assert_eq!(cfg.projects[0].services[0].id, "main");
        assert_eq!(
            make_service_key("voxlab", "main"),
            find_runtime(&cfg, "voxlab/main").unwrap().service_key
        );
    }
}
