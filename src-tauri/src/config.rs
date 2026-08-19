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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub backup: BackupConfig,
    pub auto_sync_interval_mins: Option<u64>,
    pub theme: Option<String>,
    pub ai_config: Option<serde_json::Value>,
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
