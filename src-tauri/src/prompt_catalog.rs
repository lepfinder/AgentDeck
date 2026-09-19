//! GPT-Image2 远程目录增量同步与预览图本地缓存。

use crate::media_archive::{get_media_root, sanitize_segment};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Result as SqlResult};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

pub const ORIGIN_USER: &str = "user";
pub const ORIGIN_GPT_IMAGE_2: &str = "catalog:gpt-image-2";

const CASES_JSON_URL: &str =
    "https://cdn.jsdelivr.net/gh/freestylefly/awesome-gpt-image-2@main/data/cases.json";
const IMAGE_CDN_PREFIX: &str =
    "https://cdn.jsdelivr.net/gh/freestylefly/awesome-gpt-image-2@main/data/images";

#[derive(Debug, Clone, Serialize)]
pub struct CatalogSyncResult {
    pub fetched: usize,
    pub inserted: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub images_cached: usize,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSyncProgress {
    /// download | upsert | cache | done
    pub phase: String,
    pub message: String,
    pub current: usize,
    pub total: usize,
    pub percent: u32,
    pub inserted: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub images_cached: usize,
}

pub type SyncProgressCallback = Box<dyn Fn(CatalogSyncProgress) + Send>;

fn emit_progress(cb: &Option<SyncProgressCallback>, p: CatalogSyncProgress) {
    if let Some(f) = cb {
        f(p);
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteCasesFile {
    cases: Vec<RemoteCase>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteCase {
    id: i64,
    title: String,
    #[serde(default)]
    image: Option<String>,
    #[serde(default)]
    source_label: Option<String>,
    #[serde(default)]
    source_url: Option<String>,
    prompt: String,
    #[serde(default)]
    prompt_preview: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    styles: Vec<String>,
    #[serde(default)]
    scenes: Vec<String>,
    #[serde(default)]
    featured: bool,
    #[serde(default)]
    github_url: Option<String>,
}

impl RemoteCase {
    fn external_id(&self) -> String {
        format!("gpt-image-2:{}", self.id)
    }

    /// 按 cases.json 的 image 字段拼 CDN（可能是 .jpg / .png）
    fn preview_url(&self) -> String {
        if let Some(img) = self.image.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            let name = img.trim_start_matches('/');
            let path = if let Some(rest) = name.strip_prefix("images/") {
                format!("data/images/{rest}")
            } else if name.starts_with("data/") {
                name.to_string()
            } else {
                format!("data/images/{name}")
            };
            return format!(
                "https://cdn.jsdelivr.net/gh/freestylefly/awesome-gpt-image-2@main/{path}"
            );
        }
        format!("{}/case{}.jpg", IMAGE_CDN_PREFIX, self.id)
    }
}

fn content_hash(case: &RemoteCase, preview_url: &str) -> String {
    let mut hasher = DefaultHasher::new();
    case.title.hash(&mut hasher);
    case.prompt.hash(&mut hasher);
    preview_url.hash(&mut hasher);
    case.category.clone().unwrap_or_default().hash(&mut hasher);
    case.styles.join("\0").hash(&mut hasher);
    case.scenes.join("\0").hash(&mut hasher);
    case.featured.hash(&mut hasher);
    case.source_label
        .clone()
        .unwrap_or_default()
        .hash(&mut hasher);
    case.source_url.clone().unwrap_or_default().hash(&mut hasher);
    case.github_url.clone().unwrap_or_default().hash(&mut hasher);
    case.prompt_preview
        .clone()
        .unwrap_or_default()
        .hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

fn fetch_remote_cases() -> Result<Vec<RemoteCase>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(CASES_JSON_URL)
        .send()
        .map_err(|e| format!("下载 cases.json 失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("cases.json HTTP {}", resp.status()));
    }
    let file: RemoteCasesFile = resp
        .json()
        .map_err(|e| format!("解析 cases.json 失败: {e}"))?;
    Ok(file.cases)
}

fn existing_catalog_row(
    conn: &Connection,
    external_id: &str,
) -> SqlResult<Option<(i64, Option<String>)>> {
    conn.query_row(
        "SELECT id, content_hash FROM prompts WHERE external_id = ?1",
        params![external_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()
}

fn upsert_case(conn: &Connection, case: &RemoteCase) -> SqlResult<&'static str> {
    let external_id = case.external_id();
    let preview_url = case.preview_url();
    let hash = content_hash(case, &preview_url);
    let now = Utc::now().to_rfc3339();
    let styles_json = serde_json::to_string(&case.styles).unwrap_or_else(|_| "[]".into());
    let scenes_json = serde_json::to_string(&case.scenes).unwrap_or_else(|_| "[]".into());
    let mut tags = case.styles.clone();
    for s in &case.scenes {
        if !tags.iter().any(|t| t == s) {
            tags.push(s.clone());
        }
    }
    let tags_json = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".into());
    let genre = case.category.clone().unwrap_or_default();
    let featured = if case.featured { 1 } else { 0 };

    match existing_catalog_row(conn, &external_id)? {
        Some((_id, Some(old_hash))) if old_hash == hash => Ok("unchanged"),
        Some((id, _)) => {
            conn.execute(
                r#"
                UPDATE prompts SET
                    title = ?1,
                    content = ?2,
                    category = 'image',
                    tags_json = ?3,
                    source_url = ?4,
                    source_note = ?5,
                    preview_url = ?6,
                    origin = ?7,
                    genre = ?8,
                    styles_json = ?9,
                    scenes_json = ?10,
                    featured = ?11,
                    github_url = ?12,
                    prompt_preview = ?13,
                    content_hash = ?14,
                    preview_width = CASE WHEN preview_url IS NOT ?6 THEN NULL ELSE preview_width END,
                    preview_height = CASE WHEN preview_url IS NOT ?6 THEN NULL ELSE preview_height END,
                    updated_at = ?15
                WHERE id = ?16
                "#,
                params![
                    case.title,
                    case.prompt,
                    tags_json,
                    case.source_url,
                    case.source_label,
                    preview_url,
                    ORIGIN_GPT_IMAGE_2,
                    genre,
                    styles_json,
                    scenes_json,
                    featured,
                    case.github_url,
                    case.prompt_preview,
                    hash,
                    now,
                    id,
                ],
            )?;
            Ok("updated")
        }
        None => {
            conn.execute(
                r#"
                INSERT INTO prompts (
                    title, content, category, tags_json, source_url, source_note, notes,
                    preview_url, preview_local, origin, external_id, genre, styles_json, scenes_json,
                    featured, github_url, prompt_preview, content_hash,
                    is_starred, use_count, created_at, updated_at
                ) VALUES (
                    ?1, ?2, 'image', ?3, ?4, ?5, NULL,
                    ?6, NULL, ?7, ?8, ?9, ?10, ?11,
                    ?12, ?13, ?14, ?15,
                    0, 0, ?16, ?16
                )
                "#,
                params![
                    case.title,
                    case.prompt,
                    tags_json,
                    case.source_url,
                    case.source_label,
                    preview_url,
                    ORIGIN_GPT_IMAGE_2,
                    external_id,
                    genre,
                    styles_json,
                    scenes_json,
                    featured,
                    case.github_url,
                    case.prompt_preview,
                    hash,
                    now,
                ],
            )?;
            Ok("inserted")
        }
    }
}

/// 拉取远程 cases.json，按 external_id + content_hash 增量 upsert。
/// 不覆盖 is_starred / notes / use_count / last_used_at / preview_local。
pub fn sync_gpt_image_catalog(conn: &Connection) -> Result<CatalogSyncResult, String> {
    sync_gpt_image_catalog_with_progress(conn, None)
}

pub fn sync_gpt_image_catalog_with_progress(
    conn: &Connection,
    progress: Option<SyncProgressCallback>,
) -> Result<CatalogSyncResult, String> {
    emit_progress(
        &progress,
        CatalogSyncProgress {
            phase: "download".into(),
            message: "正在下载目录索引…".into(),
            current: 0,
            total: 0,
            percent: 2,
            inserted: 0,
            updated: 0,
            unchanged: 0,
            images_cached: 0,
        },
    );

    let cases = fetch_remote_cases()?;
    let fetched = cases.len();
    let mut inserted = 0usize;
    let mut updated = 0usize;
    let mut unchanged = 0usize;

    emit_progress(
        &progress,
        CatalogSyncProgress {
            phase: "upsert".into(),
            message: format!("正在写入目录（0 / {fetched}）…"),
            current: 0,
            total: fetched,
            percent: 8,
            inserted: 0,
            updated: 0,
            unchanged: 0,
            images_cached: 0,
        },
    );

    for (idx, case) in cases.iter().enumerate() {
        match upsert_case(conn, case).map_err(|e| e.to_string())? {
            "inserted" => inserted += 1,
            "updated" => updated += 1,
            _ => unchanged += 1,
        }

        let current = idx + 1;
        // 约每 20 条或最后一条汇报一次，避免事件风暴
        if current == fetched || current % 20 == 0 || current == 1 {
            let upsert_ratio = if fetched == 0 {
                1.0
            } else {
                current as f64 / fetched as f64
            };
            let percent = 8 + ((upsert_ratio * 72.0) as u32).min(72);
            emit_progress(
                &progress,
                CatalogSyncProgress {
                    phase: "upsert".into(),
                    message: format!("正在写入目录（{current} / {fetched}）…"),
                    current,
                    total: fetched,
                    percent,
                    inserted,
                    updated,
                    unchanged,
                    images_cached: 0,
                },
            );
        }
    }

    emit_progress(
        &progress,
        CatalogSyncProgress {
            phase: "cache".into(),
            message: "正在缓存预览图…".into(),
            current: fetched,
            total: fetched,
            percent: 88,
            inserted,
            updated,
            unchanged,
            images_cached: 0,
        },
    );

    let images_cached = cache_missing_previews(conn, 8)?;

    emit_progress(
        &progress,
        CatalogSyncProgress {
            phase: "done".into(),
            message: "同步完成".into(),
            current: fetched,
            total: fetched,
            percent: 100,
            inserted,
            updated,
            unchanged,
            images_cached,
        },
    );

    Ok(CatalogSyncResult {
        fetched,
        inserted,
        updated,
        unchanged,
        images_cached,
        message: format!(
            "同步完成：拉取 {fetched}，新增 {inserted}，更新 {updated}，未变 {unchanged}，缓存图片 {images_cached}"
        ),
    })
}

fn catalog_cache_dir() -> Option<PathBuf> {
    get_media_root().map(|r| r.join("prompt-catalog").join("gpt-image-2"))
}

fn cache_filename_for(external_id: &str, preview_url: &str) -> String {
    let case_num = external_id
        .strip_prefix("gpt-image-2:")
        .unwrap_or("unknown");
    let ext = preview_url
        .rsplit('/')
        .next()
        .and_then(|name| name.rsplit('.').next())
        .filter(|e| e.len() <= 4 && e.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or("jpg");
    format!("case{}.{}", sanitize_segment(case_num), sanitize_segment(ext))
}

fn media_route_for_file(filename: &str) -> String {
    format!("/media/prompt-catalog/gpt-image-2/{}", filename)
}

/// 尚未本地缓存预览图的目录条目数。
pub fn count_uncached_previews(conn: &Connection) -> Result<usize, String> {
    conn.query_row(
        r#"
        SELECT COUNT(*) FROM prompts
        WHERE origin = ?1
          AND preview_url IS NOT NULL AND preview_url != ''
          AND (preview_local IS NULL OR preview_local = '')
        "#,
        params![ORIGIN_GPT_IMAGE_2],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n as usize)
    .map_err(|e| e.to_string())
}

/// 后台批量缓存，直到没有缺失或达到 max_batches。
pub fn warm_preview_cache(
    conn: &Connection,
    batch_size: usize,
    max_batches: usize,
) -> Result<usize, String> {
    let mut total = 0usize;
    for _ in 0..max_batches {
        let n = cache_missing_previews(conn, batch_size)?;
        total += n;
        if n == 0 {
            break;
        }
    }
    Ok(total)
}

/// 增量缓存尚未落地的预览图。
pub fn cache_missing_previews(conn: &Connection, limit: usize) -> Result<usize, String> {
    let dir = catalog_cache_dir().ok_or_else(|| "无法解析媒体目录".to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, external_id, preview_url
            FROM prompts
            WHERE origin = ?1
              AND preview_url IS NOT NULL
              AND preview_url != ''
              AND (preview_local IS NULL OR preview_local = '')
            ORDER BY featured DESC, id DESC
            LIMIT ?2
            "#,
        )
        .map_err(|e| e.to_string())?;

    let rows: Vec<(i64, Option<String>, String)> = stmt
        .query_map(params![ORIGIN_GPT_IMAGE_2, limit as i64], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let mut cached = 0usize;
    for (id, external_id, preview_url) in rows {
        let filename = cache_filename_for(
            external_id.as_deref().unwrap_or("unknown"),
            &preview_url,
        );
        let dest = dir.join(&filename);
        let route = media_route_for_file(&filename);

        if dest.is_file() {
            let _ = conn.execute(
                "UPDATE prompts SET preview_local = ?1 WHERE id = ?2",
                params![route, id],
            );
            cached += 1;
            continue;
        }

        if let Ok(resp) = client.get(&preview_url).send() {
            if resp.status().is_success() {
                if let Ok(bytes) = resp.bytes() {
                    if !bytes.is_empty() && std::fs::write(&dest, &bytes).is_ok() {
                        let _ = conn.execute(
                            "UPDATE prompts SET preview_local = ?1 WHERE id = ?2",
                            params![route, id],
                        );
                        cached += 1;
                    }
                }
            }
        }
    }
    Ok(cached)
}
