use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    #[serde(default)]
    pub target_path: String,
    #[serde(default = "default_true")]
    pub auto_backup_enabled: bool,
    #[serde(default = "default_max_snapshots")]
    pub max_snapshots: usize,
}

fn default_true() -> bool {
    true
}

fn default_max_snapshots() -> usize {
    3
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            target_path: String::new(),
            auto_backup_enabled: true,
            max_snapshots: 3,
        }
    }
}

/// 其他机器的 home 根：可直接写挂载路径，也可带 remote_home 声明该机器真实 home
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AgentHomeConfig {
    Plain(String),
    Mapped {
        home: String,
        #[serde(default)]
        remote_home: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct AgentHomeRoot {
    pub home: PathBuf,
    /// 会话正文里记录的路径前缀（远端机器真实 home），用于把图片路径映射到挂载根
    pub remote_prefix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub backup: BackupConfig,
    pub theme: Option<String>,
    pub ai_config: Option<serde_json::Value>,
    /// 其他机器的 home 根（如 SMB 挂载盘），importer 会按各源相对路径一并扫描
    #[serde(default)]
    pub extra_agent_homes: Vec<AgentHomeConfig>,
}

pub fn get_config_path() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        let app_dir = home.join(".agentdeck");
        let _ = std::fs::create_dir_all(&app_dir);
        app_dir.join("config.json")
    } else {
        PathBuf::from("config.json")
    }
}

/// 本机 home + 配置的额外 home 根，只保留实际存在的目录
pub fn agent_home_roots() -> Vec<AgentHomeRoot> {
    let mut roots = Vec::new();
    if let Some(home) = dirs::home_dir() {
        roots.push(AgentHomeRoot {
            home,
            remote_prefix: None,
        });
    }
    for extra in load_config().extra_agent_homes {
        let (path, remote_prefix) = match extra {
            AgentHomeConfig::Plain(s) => (PathBuf::from(&s), None),
            AgentHomeConfig::Mapped { home, remote_home } => {
                (PathBuf::from(&home), remote_home.filter(|p| !p.is_empty()))
            }
        };
        if path.is_dir() && !roots.iter().any(|r| r.home == path) {
            roots.push(AgentHomeRoot {
                home: path,
                remote_prefix,
            });
        }
    }
    roots
}

pub fn load_config() -> AppConfig {
    let path = get_config_path();
    if !path.exists() {
        return AppConfig::default();
    }

    match File::open(&path) {
        Ok(mut file) => {
            let mut content = String::new();
            if file.read_to_string(&mut content).is_ok() {
                serde_json::from_str(&content).unwrap_or_default()
            } else {
                AppConfig::default()
            }
        }
        Err(_) => AppConfig::default(),
    }
}

pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = get_config_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let json_str = serde_json::to_string_pretty(config)
        .map_err(|e| format!("序列化配置失败: {}", e))?;

    let mut file = File::create(&path)
        .map_err(|e| format!("无法写入配置文件 {:?}: {}", path, e))?;

    file.write_all(json_str.as_bytes())
        .map_err(|e| format!("写入配置文件失败: {}", e))?;

    Ok(())
}
