use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// 获取统一媒体资产存储根目录 ~/.agentdeck/media
pub fn get_media_root() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".agentdeck").join("media"))
}

/// 清洗会话 ID 与文件名中的非法路径字符，防止路径穿越
pub fn sanitize_segment(s: &str) -> String {
    let sanitized: String = s
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() {
        "item".to_string()
    } else {
        sanitized
    }
}

/// 将源文件归档到 ~/.agentdeck/media/<source_app>/<conversation_id>/<filename>
/// 返回相对 Web 路由路径，如 `/media/antigravity/conv-123/a1b2c3d4_screenshot.png`
pub fn archive_image_file(source_path: &Path, source_app: &str, conv_id: &str) -> Option<String> {
    if !source_path.is_file() {
        return None;
    }

    let media_root = get_media_root()?;
    let clean_app = sanitize_segment(source_app);
    let clean_conv_id = sanitize_segment(conv_id);

    let target_dir = media_root.join(&clean_app).join(&clean_conv_id);
    if let Err(e) = std::fs::create_dir_all(&target_dir) {
        eprintln!("[MediaArchive] 创建目录失败 {:?}: {}", target_dir, e);
        return None;
    }

    // 计算源文件哈希指纹（基于路径、修改时间与文件大小）
    let meta = source_path.metadata().ok();
    let file_size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
    let file_mtime = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut hasher = DefaultHasher::new();
    source_path.to_string_lossy().hash(&mut hasher);
    file_size.hash(&mut hasher);
    file_mtime.hash(&mut hasher);
    let hash_prefix = format!("{:016x}", hasher.finish())[..8].to_string();

    let ext = source_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("png")
        .to_lowercase();

    let raw_stem = source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("image");
    let safe_stem = sanitize_segment(raw_stem);
    let truncated_stem = if safe_stem.len() > 32 {
        &safe_stem[..32]
    } else {
        &safe_stem
    };

    let filename = format!("{}_{}.{}", hash_prefix, truncated_stem, ext);
    let target_file = target_dir.join(&filename);

    // 幂等复制：若目标已存在且文件大小一致，直接复用
    if target_file.is_file() {
        if let Ok(target_meta) = target_file.metadata() {
            if target_meta.len() == file_size && file_size > 0 {
                return Some(format!("/media/{}/{}/{}", clean_app, clean_conv_id, filename));
            }
        }
    }

    if let Err(e) = std::fs::copy(source_path, &target_file) {
        eprintln!(
            "[MediaArchive] 复制图片失败 {:?} -> {:?}: {}",
            source_path, target_file, e
        );
        return None;
    }

    Some(format!("/media/{}/{}/{}", clean_app, clean_conv_id, filename))
}

/// 将内存字节归档到 ~/.agentdeck/media/<source_app>/<conversation_id>/<filename>
pub fn archive_image_bytes(
    data: &[u8],
    ext: &str,
    source_app: &str,
    conv_id: &str,
    name_hint: Option<&str>,
) -> Option<String> {
    if data.is_empty() {
        return None;
    }

    let media_root = get_media_root()?;
    let clean_app = sanitize_segment(source_app);
    let clean_conv_id = sanitize_segment(conv_id);

    let target_dir = media_root.join(&clean_app).join(&clean_conv_id);
    if let Err(e) = std::fs::create_dir_all(&target_dir) {
        eprintln!("[MediaArchive] 创建目录失败 {:?}: {}", target_dir, e);
        return None;
    }

    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    let hash_prefix = format!("{:016x}", hasher.finish())[..8].to_string();

    let clean_ext = ext.trim_start_matches('.').to_lowercase();
    let safe_ext = if clean_ext.is_empty() {
        "png".to_string()
    } else {
        clean_ext
    };

    let safe_stem = name_hint.map(sanitize_segment).unwrap_or_else(|| "asset".to_string());
    let truncated_stem = if safe_stem.len() > 32 {
        &safe_stem[..32]
    } else {
        &safe_stem
    };

    let filename = format!("{}_{}.{}", hash_prefix, truncated_stem, safe_ext);
    let target_file = target_dir.join(&filename);

    if target_file.is_file() {
        if let Ok(m) = target_file.metadata() {
            if m.len() == data.len() as u64 {
                return Some(format!("/media/{}/{}/{}", clean_app, clean_conv_id, filename));
            }
        }
    }

    if let Err(e) = std::fs::write(&target_file, data) {
        eprintln!("[MediaArchive] 写入图片失败 {:?}: {}", target_file, e);
        return None;
    }

    Some(format!("/media/{}/{}/{}", clean_app, clean_conv_id, filename))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_segment() {
        assert_eq!(sanitize_segment("normal-id_123"), "normal-id_123");
        assert_eq!(sanitize_segment("evil/../path"), "evil____path");
        assert_eq!(sanitize_segment("hello world!@#"), "hello_world___");
    }

    #[test]
    fn test_archive_image_bytes() {
        let dummy_data = b"dummy image content bytes 12345";
        let uri = archive_image_bytes(dummy_data, "png", "testapp", "testconv123", Some("mock"));
        assert!(uri.is_some());
        let uri_str = uri.unwrap();
        assert!(uri_str.starts_with("/media/testapp/testconv123/"));
        assert!(uri_str.ends_with(".png"));

        // 清理测试文件
        if let Some(root) = get_media_root() {
            let _ = std::fs::remove_dir_all(root.join("testapp"));
        }
    }
}
