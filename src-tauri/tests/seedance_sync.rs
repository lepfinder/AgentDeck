//! Seedance 目录同步端到端验证（独立临时库 + 真实 jsDelivr 数据源）。
//! 运行：cargo test --test seedance_sync -- --nocapture

use agentdeck_lib::{db, seedance_catalog};

#[test]
fn sync_seedance_catalog_into_temp_db() {
    let dir = std::env::temp_dir().join(format!("agentdeck-seedance-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let conn = rusqlite::Connection::open(&dir.join("test.db")).unwrap();
    db::apply_write_pragmas(&conn);
    db::init_schema(&conn).unwrap();

    let result = seedance_catalog::sync_seedance_catalog(&conn).unwrap();
    println!("{}", result.message);
    assert!(result.fetched > 400, "fetched={}", result.fetched);
    assert_eq!(result.inserted, result.fetched);

    // 二次同步应全部 unchanged（content_hash 生效）
    let again = seedance_catalog::sync_seedance_catalog(&conn).unwrap();
    assert_eq!(again.inserted, 0);
    assert_eq!(again.updated, 0);
    assert_eq!(again.unchanged, again.fetched);

    // 用户字段（星标/笔记）不被同步覆盖
    let first = db::list_video_prompts_ex(&conn, None, false, false)
        .unwrap()
        .into_iter()
        .next()
        .expect("at least one row");
    let input = db::PromptInput {
        title: first.base.title.clone(),
        content: first.base.content.clone(),
        category: "video".into(),
        tags: first.base.tags.clone(),
        source_url: None,
        source_note: None,
        notes: Some("my note".into()),
        preview_url: None,
        preview_local: None,
        is_starred: true,
    };
    db::update_video_prompt(&conn, first.base.id, &input).unwrap();
    seedance_catalog::sync_seedance_catalog(&conn).unwrap();
    let after = db::get_video_prompt(&conn, first.base.id).unwrap();
    assert!(after.base.is_starred);
    assert_eq!(after.base.notes.as_deref(), Some("my note"));

    // 生视频表不应污染 prompts 表
    let prompts = db::list_prompts(&conn, None, None, false).unwrap();
    assert!(prompts.iter().all(|p| p.category != "video"));

    let _ = std::fs::remove_dir_all(&dir);
}
