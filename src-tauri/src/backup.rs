use chrono::{DateTime, Local};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::{Path, PathBuf};
use tar::{Archive, Builder};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    pub file_name: String,
    pub file_path: String,
    pub file_size_bytes: u64,
    pub file_size_formatted: String,
    pub created_at: String,
    pub conversation_count: Option<i64>,
    pub media_file_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreInfo {
    pub success: bool,
    pub message: String,
    pub conversation_count: i64,
    pub media_file_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupProgress {
    pub stage: String,
    pub percent: u8,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudPreset {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub path: String,
    pub available: bool,
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

pub fn expand_tilde(p: &str) -> PathBuf {
    if let Some(stripped) = p.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    PathBuf::from(p)
}

/// 自动探测系统中的 Google Drive / iCloud / 本地等云同步预设路径
pub fn detect_cloud_presets() -> Vec<CloudPreset> {
    let mut presets = Vec::new();
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return presets,
    };

    // 1. Google Drive (macOS CloudStorage)
    let cloud_storage = home.join("Library/CloudStorage");
    let mut gdrive_found = false;
    if cloud_storage.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&cloud_storage) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("GoogleDrive-") {
                    let gdrive_path = entry.path().join("My Drive/AgentDeck_Backups");
                    let display_path = gdrive_path.to_string_lossy().to_string();
                    presets.push(CloudPreset {
                        id: "gdrive".to_string(),
                        name: "Google Drive".to_string(),
                        icon: "Cloud".to_string(),
                        path: display_path,
                        available: true,
                    });
                    gdrive_found = true;
                    break;
                }
            }
        }
    }
    if !gdrive_found {
        presets.push(CloudPreset {
            id: "gdrive".to_string(),
            name: "Google Drive".to_string(),
            icon: "Cloud".to_string(),
            path: home
                .join("Library/CloudStorage/GoogleDrive/My Drive/AgentDeck_Backups")
                .to_string_lossy()
                .to_string(),
            available: false,
        });
    }

    // 2. iCloud Drive
    let icloud_dir = home.join("Library/Mobile Documents/com~apple~CloudDocs/AgentDeck_Backups");
    let icloud_parent = home.join("Library/Mobile Documents/com~apple~CloudDocs");
    presets.push(CloudPreset {
        id: "icloud".to_string(),
        name: "iCloud Drive".to_string(),
        icon: "CloudRain".to_string(),
        path: icloud_dir.to_string_lossy().to_string(),
        available: icloud_parent.is_dir(),
    });

    // 3. 本地文档目录 (Documents)
    let docs_dir = home.join("Documents/AgentDeck_Backups");
    presets.push(CloudPreset {
        id: "documents".to_string(),
        name: "本地文档 (Documents)".to_string(),
        icon: "Folder".to_string(),
        path: docs_dir.to_string_lossy().to_string(),
        available: true,
    });

    presets
}

/// 执行原子备份：SQLite VACUUM 快照 + media 文件夹打包压缩为单个 .tar.gz（带进度通知）
pub fn create_backup(
    conn: &Connection,
    target_dir_str: &str,
    max_snapshots: usize,
) -> Result<BackupInfo, String> {
    create_backup_with_progress(conn, target_dir_str, max_snapshots, None)
}

pub fn create_backup_with_progress(
    conn: &Connection,
    target_dir_str: &str,
    max_snapshots: usize,
    progress_callback: Option<Box<dyn Fn(BackupProgress) + Send + Sync>>,
) -> Result<BackupInfo, String> {
    let notify = |stage: &str, percent: u8, message: &str| {
        if let Some(ref cb) = progress_callback {
            cb(BackupProgress {
                stage: stage.to_string(),
                percent,
                message: message.to_string(),
            });
        }
    };

    if target_dir_str.trim().is_empty() {
        return Err("备份目标路径不能为空".to_string());
    }

    notify("init", 5, "正在初始化备份目录...");
    let target_dir = expand_tilde(target_dir_str);
    std::fs::create_dir_all(&target_dir)
        .map_err(|e| format!("无法创建备份目标目录 {:?}: {}", target_dir, e))?;

    let now: DateTime<Local> = Local::now();
    let ts_str = now.format("%Y%m%d_%H%M%S").to_string();
    let iso_created_at = now.to_rfc3339();

    // 临时目录，用于存放纯净一致的 SQLite 快照
    let temp_staging_dir = std::env::temp_dir().join(format!("agentdeck_backup_tmp_{}", ts_str));
    std::fs::create_dir_all(&temp_staging_dir)
        .map_err(|e| format!("创建临时打包目录失败: {}", e))?;

    let temp_db_path = temp_staging_dir.join("agentdeck.db");

    // 1. 使用 SQLite 原生 VACUUM INTO 生成自包含热快照（无锁一致性）
    notify("snapshot", 20, "正在生成 SQLite 一致性热快照...");
    let vacuum_sql = format!("VACUUM INTO '{}'", temp_db_path.to_string_lossy().replace('\'', "''"));
    if let Err(e) = conn.execute(&vacuum_sql, []) {
        let _ = std::fs::remove_dir_all(&temp_staging_dir);
        return Err(format!("生成 SQLite 热备份快照失败: {}", e));
    }

    // 读取快照库中的会话数
    let conv_count: Option<i64> = match Connection::open_with_flags(
        &temp_db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c.query_row("SELECT count(*) FROM conversations", [], |r| r.get(0)).ok(),
        Err(_) => None,
    };

    // 2. 打包生成 .tar.gz
    notify("collecting", 40, "正在收集数据库与媒体图片资产...");
    let tar_file_name = format!("agentdeck_backup_{}.tar.gz", ts_str);
    let target_tar_path = target_dir.join(&tar_file_name);

    let tar_file = File::create(&target_tar_path)
        .map_err(|e| format!("创建备份归档文件失败 {:?}: {}", target_tar_path, e))?;
    let enc = GzEncoder::new(tar_file, Compression::default());
    let mut tar_builder = Builder::new(enc);

    // 追加数据库
    if let Err(e) = tar_builder.append_path_with_name(&temp_db_path, "agentdeck.db") {
        let _ = std::fs::remove_dir_all(&temp_staging_dir);
        let _ = std::fs::remove_file(&target_tar_path);
        return Err(format!("追加数据库到归档包失败: {}", e));
    }

    // 追加配置文件 ~/.agentdeck/config.json
    let config_path = crate::config::get_config_path();
    if config_path.is_file() {
        let _ = tar_builder.append_path_with_name(&config_path, "config.json");
    }

    // 追加媒体目录 ~/.agentdeck/media
    notify("compressing", 65, "正在高压缩比打包归档 (.tar.gz)...");
    let mut media_file_count = 0usize;
    if let Some(media_root) = crate::media_archive::get_media_root() {
        if media_root.is_dir() {
            for entry in WalkDir::new(&media_root).into_iter().flatten() {
                let p = entry.path();
                if p.is_file() {
                    media_file_count += 1;
                    if let Ok(rel) = p.strip_prefix(&media_root) {
                        let tar_entry_path = Path::new("media").join(rel);
                        let _ = tar_builder.append_path_with_name(p, tar_entry_path);
                    }
                }
            }
        }
    }

    let enc = match tar_builder.into_inner() {
        Ok(e) => e,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&temp_staging_dir);
            let _ = std::fs::remove_file(&target_tar_path);
            return Err(format!("完成归档打包失败: {}", e));
        }
    };
    if let Err(e) = enc.finish() {
        let _ = std::fs::remove_dir_all(&temp_staging_dir);
        let _ = std::fs::remove_file(&target_tar_path);
        return Err(format!("完成 Gzip 压缩失败: {}", e));
    }

    // 清理临时快照
    let _ = std::fs::remove_dir_all(&temp_staging_dir);

    // 3. 修剪旧备份：保留最新的 max_snapshots 份（默认 3 份）
    notify("pruning", 90, "正在检查并修剪多余旧快照 (保留最新 3 份)...");
    let effective_max = if max_snapshots == 0 { 3 } else { max_snapshots };
    prune_old_backups(&target_dir, effective_max);

    let file_size_bytes = target_tar_path
        .metadata()
        .map(|m| m.len())
        .unwrap_or(0);

    notify("done", 100, "备份完成！");

    Ok(BackupInfo {
        file_name: tar_file_name,
        file_path: target_tar_path.to_string_lossy().to_string(),
        file_size_bytes,
        file_size_formatted: format_bytes(file_size_bytes),
        created_at: iso_created_at,
        conversation_count: conv_count,
        media_file_count: Some(media_file_count),
    })
}

/// 扫描目标目录下的全部备份历史
pub fn list_backups(target_dir_str: &str) -> Result<Vec<BackupInfo>, String> {
    let target_dir = expand_tilde(target_dir_str);
    if !target_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut list = Vec::new();
    let entries = std::fs::read_dir(&target_dir).map_err(|e| e.to_string())?;

    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_file() {
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            if name.starts_with("agentdeck_backup_") && name.ends_with(".tar.gz") {
                if let Ok(meta) = p.metadata() {
                    let size = meta.len();
                    let mtime = meta
                        .modified()
                        .ok()
                        .and_then(|t| {
                            let dt: DateTime<Local> = t.into();
                            Some(dt.to_rfc3339())
                        })
                        .unwrap_or_default();

                    list.push(BackupInfo {
                        file_name: name,
                        file_path: p.to_string_lossy().to_string(),
                        file_size_bytes: size,
                        file_size_formatted: format_bytes(size),
                        created_at: mtime,
                        conversation_count: None,
                        media_file_count: None,
                    });
                }
            }
        }
    }

    // 按文件名或创建时间倒序（最新的在前）
    list.sort_by(|a, b| b.file_name.cmp(&a.file_name));
    Ok(list)
}

/// 修剪多余旧备份，只保留最新 N 份
fn prune_old_backups(target_dir: &Path, max_keep: usize) {
    if let Ok(mut backups) = list_backups(&target_dir.to_string_lossy()) {
        if backups.len() > max_keep {
            for old in backups.drain(max_keep..) {
                let p = PathBuf::from(&old.file_path);
                if p.is_file() {
                    let _ = std::fs::remove_file(&p);
                }
            }
        }
    }
}

/// 从 .tar.gz 归档文件恢复数据库与媒体文件夹
pub fn restore_backup(backup_file_str: &str) -> Result<RestoreInfo, String> {
    let backup_path = expand_tilde(backup_file_str);
    if !backup_path.is_file() {
        return Err(format!("备份文件不存在: {:?}", backup_path));
    }

    let file = File::open(&backup_path)
        .map_err(|e| format!("打开备份文件失败: {}", e))?;
    let dec = GzDecoder::new(file);
    let mut archive = Archive::new(dec);

    let temp_restore_dir = std::env::temp_dir().join(format!("agentdeck_restore_tmp_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_restore_dir);
    std::fs::create_dir_all(&temp_restore_dir)
        .map_err(|e| format!("创建解压目录失败: {}", e))?;

    if let Err(e) = archive.unpack(&temp_restore_dir) {
        let _ = std::fs::remove_dir_all(&temp_restore_dir);
        return Err(format!("解压备份文件失败: {}", e));
    }

    let restored_db = temp_restore_dir.join("agentdeck.db");
    if !restored_db.is_file() {
        let _ = std::fs::remove_dir_all(&temp_restore_dir);
        return Err("归档包中未包含有效的 agentdeck.db 数据库文件".to_string());
    }

    // 校验恢复后的数据库
    let conv_count: i64 = match Connection::open_with_flags(
        &restored_db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c
            .query_row("SELECT count(*) FROM conversations", [], |r| r.get(0))
            .unwrap_or(0),
        Err(e) => {
            let _ = std::fs::remove_dir_all(&temp_restore_dir);
            return Err(format!("恢复数据库校验失败: {}", e));
        }
    };

    // 替换本地 SQLite 数据库
    let target_db = crate::db::get_database_path();
    if let Some(parent) = target_db.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // 清理可能存在的 wal 与 shm
    let wal = target_db.with_file_name("agentdeck.db-wal");
    let shm = target_db.with_file_name("agentdeck.db-shm");
    let _ = std::fs::remove_file(&wal);
    let _ = std::fs::remove_file(&shm);

    if let Err(e) = std::fs::copy(&restored_db, &target_db) {
        let _ = std::fs::remove_dir_all(&temp_restore_dir);
        return Err(format!("替换目标数据库失败: {}", e));
    }

    // 恢复媒体文件夹
    let mut media_restored_count = 0usize;
    let restored_media = temp_restore_dir.join("media");
    if restored_media.is_dir() {
        if let Some(target_media_root) = crate::media_archive::get_media_root() {
            let _ = std::fs::create_dir_all(&target_media_root);
            for entry in WalkDir::new(&restored_media).into_iter().flatten() {
                let p = entry.path();
                if p.is_file() {
                    media_restored_count += 1;
                    if let Ok(rel) = p.strip_prefix(&restored_media) {
                        let dest = target_media_root.join(rel);
                        if let Some(parent) = dest.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        let _ = std::fs::copy(p, &dest);
                    }
                }
            }
        }
    }

    // 恢复配置文件 config.json
    let restored_config = temp_restore_dir.join("config.json");
    if restored_config.is_file() {
        let target_config = crate::config::get_config_path();
        if let Some(parent) = target_config.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::copy(&restored_config, &target_config);
    }

    let _ = std::fs::remove_dir_all(&temp_restore_dir);

    Ok(RestoreInfo {
        success: true,
        message: "数据、配置与媒体资产恢复成功！".to_string(),
        conversation_count: conv_count,
        media_file_count: media_restored_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(2048), "2 KB");
        assert_eq!(format_bytes(1048576 * 5), "5.0 MB");
    }

    #[test]
    fn test_expand_tilde() {
        let p = expand_tilde("~/Documents");
        assert!(!p.to_string_lossy().starts_with('~'));
    }

    #[test]
    fn test_create_and_prune_backups() {
        let test_target = std::env::temp_dir().join(format!("agentdeck_test_bk_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&test_target);
        std::fs::create_dir_all(&test_target).unwrap();

        let temp_db_file = test_target.join("source.db");
        let conn = Connection::open(&temp_db_file).unwrap();
        conn.execute(
            "CREATE TABLE conversations (id TEXT PRIMARY KEY, title TEXT)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO conversations (id, title) VALUES ('test-1', 'Test Conversation')",
            [],
        )
        .unwrap();

        let res = create_backup(&conn, &test_target.to_string_lossy(), 3);
        assert!(res.is_ok());
        let info = res.unwrap();
        assert_eq!(info.conversation_count, Some(1));
        assert!(info.file_size_bytes > 0);

        let list = list_backups(&test_target.to_string_lossy()).unwrap();
        assert_eq!(list.len(), 1);

        let _ = std::fs::remove_dir_all(&test_target);
    }
}
