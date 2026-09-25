//! Seedance 远程视频提示词目录增量同步（awesome-seedance / goodcase.ai）。

use crate::prompt_catalog::{
    emit_progress, CatalogSyncProgress, CatalogSyncResult, SyncProgressCallback,
};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Result as SqlResult};
use serde::Deserialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub const ORIGIN_USER: &str = "user";
pub const ORIGIN_SEEDANCE: &str = "catalog:seedance";

const CASES_JSON_URL: &str =
    "https://cdn.jsdelivr.net/gh/LearnPrompt/awesome-seedance@main/data/cases.json";

#[derive(Debug, Deserialize)]
struct RemoteCasesFile {
    cases: Vec<RemoteCase>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteCase {
    slug: String,
    title: String,
    #[serde(default)]
    title_en: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    prompt_full: String,
    #[serde(default)]
    models: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    creator: Option<String>,
    #[serde(default)]
    source_url: Option<String>,
    #[serde(default)]
    source_published_at: Option<String>,
    #[serde(default)]
    heat_score: Option<i64>,
    #[serde(default)]
    media_url: Option<String>,
    #[serde(default)]
    poster_url: Option<String>,
    #[serde(default)]
    goodcase_url: Option<String>,
    #[serde(default)]
    evidence_level: Option<String>,
    #[serde(default)]
    stability_score: Option<i64>,
}

impl RemoteCase {
    fn external_id(&self) -> String {
        format!("seedance:{}", self.slug)
    }
}

fn content_hash(case: &RemoteCase) -> String {
    let mut hasher = DefaultHasher::new();
    case.title.hash(&mut hasher);
    case.title_en.clone().unwrap_or_default().hash(&mut hasher);
    case.summary.clone().unwrap_or_default().hash(&mut hasher);
    case.prompt_full.hash(&mut hasher);
    case.models.join("\0").hash(&mut hasher);
    case.tags.join("\0").hash(&mut hasher);
    case.creator.clone().unwrap_or_default().hash(&mut hasher);
    case.source_url.clone().unwrap_or_default().hash(&mut hasher);
    case.media_url.clone().unwrap_or_default().hash(&mut hasher);
    case.poster_url.clone().unwrap_or_default().hash(&mut hasher);
    case.goodcase_url.clone().unwrap_or_default().hash(&mut hasher);
    case.evidence_level.clone().unwrap_or_default().hash(&mut hasher);
    case.heat_score.hash(&mut hasher);
    case.stability_score.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

fn fetch_remote_cases() -> Result<Vec<RemoteCase>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
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
        "SELECT id, content_hash FROM video_prompts WHERE external_id = ?1",
        params![external_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()
}

fn upsert_case(conn: &Connection, case: &RemoteCase) -> SqlResult<&'static str> {
    let external_id = case.external_id();
    let hash = content_hash(case);
    let now = Utc::now().to_rfc3339();
    let models_json = serde_json::to_string(&case.models).unwrap_or_else(|_| "[]".into());
    let tags_json = serde_json::to_string(&case.tags).unwrap_or_else(|_| "[]".into());
    let genre = case.models.first().cloned().unwrap_or_default();

    match existing_catalog_row(conn, &external_id)? {
        Some((_id, Some(old_hash))) if old_hash == hash => Ok("unchanged"),
        Some((id, _)) => {
            conn.execute(
                r#"
                UPDATE video_prompts SET
                    title = ?1,
                    content = ?2,
                    tags_json = ?3,
                    source_url = ?4,
                    source_note = ?5,
                    preview_url = ?6,
                    origin = ?7,
                    genre = ?8,
                    scenes_json = ?9,
                    prompt_preview = ?10,
                    content_hash = ?11,
                    slug = ?12,
                    title_en = ?13,
                    summary = ?14,
                    models_json = ?9,
                    media_url = ?15,
                    goodcase_url = ?16,
                    heat_score = ?17,
                    stability_score = ?18,
                    evidence_level = ?19,
                    source_published_at = ?20,
                    preview_width = CASE WHEN preview_url IS NOT ?6 THEN NULL ELSE preview_width END,
                    preview_height = CASE WHEN preview_url IS NOT ?6 THEN NULL ELSE preview_height END,
                    updated_at = ?21
                WHERE id = ?22
                "#,
                params![
                    case.title,
                    case.prompt_full,
                    tags_json,
                    case.source_url,
                    case.creator,
                    case.poster_url,
                    ORIGIN_SEEDANCE,
                    genre,
                    models_json,
                    case.summary,
                    hash,
                    case.slug,
                    case.title_en,
                    case.summary,
                    case.media_url,
                    case.goodcase_url,
                    case.heat_score,
                    case.stability_score,
                    case.evidence_level,
                    case.source_published_at,
                    now,
                    id,
                ],
            )?;
            Ok("updated")
        }
        None => {
            conn.execute(
                r#"
                INSERT INTO video_prompts (
                    title, content, tags_json, source_url, source_note, notes,
                    preview_url, preview_local, origin, external_id, genre, styles_json, scenes_json,
                    featured, github_url, prompt_preview, content_hash,
                    is_starred, use_count, created_at, updated_at,
                    slug, title_en, summary, models_json, media_url, goodcase_url,
                    heat_score, stability_score, evidence_level, source_published_at
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, NULL,
                    ?6, NULL, ?7, ?8, ?9, '[]', ?9,
                    0, NULL, ?10, ?11,
                    0, 0, ?20, ?20,
                    ?12, ?13, ?10, ?9, ?14, ?15,
                    ?16, ?17, ?18, ?19
                )
                "#,
                params![
                    case.title,
                    case.prompt_full,
                    tags_json,
                    case.source_url,
                    case.creator,
                    case.poster_url,
                    ORIGIN_SEEDANCE,
                    external_id,
                    models_json,
                    case.summary,
                    hash,
                    case.slug,
                    case.title_en,
                    case.media_url,
                    case.goodcase_url,
                    case.heat_score,
                    case.stability_score,
                    case.evidence_level,
                    case.source_published_at,
                    now,
                ],
            )?;
            Ok("inserted")
        }
    }
}

/// 拉取远程 cases.json，按 external_id + content_hash 增量 upsert。
/// 不覆盖 is_starred / notes / use_count / last_used_at / preview_local。
pub fn sync_seedance_catalog(conn: &Connection) -> Result<CatalogSyncResult, String> {
    sync_seedance_catalog_with_progress(conn, None)
}

pub fn sync_seedance_catalog_with_progress(
    conn: &Connection,
    progress: Option<SyncProgressCallback>,
) -> Result<CatalogSyncResult, String> {
    emit_progress(
        &progress,
        CatalogSyncProgress {
            phase: "download".into(),
            message: "正在下载 Seedance 目录…".into(),
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
            message: format!("正在写入视频目录（0 / {fetched}）…"),
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
        if current == fetched || current % 20 == 0 || current == 1 {
            let ratio = if fetched == 0 {
                1.0
            } else {
                current as f64 / fetched as f64
            };
            let percent = 8 + ((ratio * 90.0) as u32).min(90);
            emit_progress(
                &progress,
                CatalogSyncProgress {
                    phase: "upsert".into(),
                    message: format!("正在写入视频目录（{current} / {fetched}）…"),
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
            phase: "done".into(),
            message: "视频目录同步完成".into(),
            current: fetched,
            total: fetched,
            percent: 100,
            inserted,
            updated,
            unchanged,
            images_cached: 0,
        },
    );

    Ok(CatalogSyncResult {
        fetched,
        inserted,
        updated,
        unchanged,
        images_cached: 0,
        message: format!(
            "视频目录同步完成：拉取 {fetched}，新增 {inserted}，更新 {updated}，未变 {unchanged}"
        ),
    })
}
