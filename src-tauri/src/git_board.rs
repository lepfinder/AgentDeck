//! Git 看板 — 汇总"最近 N 天有会话活动"的工作区当前 Git 状态。
//!
//! 只读采集：分支、ahead/behind、暂存/未暂存/未跟踪/冲突计数、最近提交。
//! 不做任何写操作（stage/commit/push 属于后续阶段，另行设计确认流程）。
//!
//! 目录不存在或非 git 仓库的工作区不展示、不统计，采集阶段直接过滤。
//!
//! 性能：git2 打开仓库 + status 通常是毫秒级，但大仓库可能上百毫秒，
//! 因此用 scoped threads 并行探测（上限 8 线程），避免几十个工作区时阻塞 UI。

use crate::db;
use chrono::FixedOffset;
use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 单个工作区的 Git 状态条目（仅有效 git 仓库，无效工作区在采集阶段被过滤）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitBoardEntry {
    pub workspace_path: String,
    /// 用户自定义显示名（可能为空，前端 fallback 到路径末段）
    pub display_name: String,
    /// 是否为有效 git 仓库
    pub is_repo: bool,
    /// 当前分支名；detached HEAD 时为 None
    pub branch: Option<String>,
    pub detached: bool,
    /// 本地领先远端提交数（无远端跟踪时 None）
    pub ahead: Option<u32>,
    /// 本地落后远端提交数
    pub behind: Option<u32>,
    /// 远端名称（优先 origin，否则第一个远端）；无远端时 None
    pub remote_name: Option<String>,
    /// 远端 URL
    pub remote_url: Option<String>,
    /// 当前分支的跟踪分支（如 origin/main）；无跟踪时 None
    pub upstream: Option<String>,
    /// 已暂存变更数
    pub staged: u32,
    /// 已跟踪文件的工作区修改数（未暂存）
    pub unstaged: u32,
    /// 未跟踪文件/目录数
    pub untracked: u32,
    /// 冲突文件数
    pub conflicts: u32,
    pub last_commit_message: Option<String>,
    /// 最近提交时间（北京时间 RFC3339）
    pub last_commit_time: Option<String>,
    /// 最近提交短哈希（7 位）
    pub last_commit_short_id: Option<String>,
    /// AgentDeck 内该工作区最近一次会话活动时间
    pub last_activity: Option<String>,
    /// 时间窗口内的会话总数
    pub conversation_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitBoardSummary {
    pub days: i64,
    /// 时间窗口内有活动的工作区总数
    pub total: usize,
    /// 其中是 git 仓库的数量
    pub repo_count: usize,
    /// 有未提交变更（staged + unstaged + untracked + conflicts > 0）的仓库数
    pub dirty_count: usize,
    /// 本地领先远端（待推送）的仓库数
    pub ahead_count: usize,
    pub generated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitBoardResponse {
    pub entries: Vec<GitBoardEntry>,
    pub summary: GitBoardSummary,
}

/// 从数据库查出"最近 N 天有会话活动"的工作区清单。
fn query_active_workspaces(conn: &Connection, days: i64) -> Result<Vec<GitBoardEntry>> {
    let modifier = format!("-{} days", days.max(1));
    let mut stmt = conn.prepare(
        r#"
        SELECT w.workspace_path,
               COALESCE(NULLIF(w.display_name, ''), '') AS display_name,
               MAX(c.updated_at) AS last_activity,
               COUNT(c.id) AS conversation_count
        FROM workspaces w
        JOIN conversations c ON c.workspace_path = w.workspace_path
        GROUP BY w.workspace_path
        HAVING MAX(c.updated_at) IS NOT NULL
           AND datetime(MAX(c.updated_at)) >= datetime('now', ?1)
        ORDER BY MAX(c.updated_at) DESC
        "#,
    )?;

    let rows = stmt.query_map(params![modifier], |row| {
        Ok(GitBoardEntry {
            workspace_path: row.get(0)?,
            display_name: row.get(1)?,
            is_repo: false,
            branch: None,
            detached: false,
            ahead: None,
            behind: None,
            remote_name: None,
            remote_url: None,
            upstream: None,
            staged: 0,
            unstaged: 0,
            untracked: 0,
            conflicts: 0,
            last_commit_message: None,
            last_commit_time: None,
            last_commit_short_id: None,
            last_activity: row.get(2)?,
            conversation_count: row.get(3)?,
        })
    })?;

    rows.collect()
}

/// 探测单个工作区的 git 状态（纯文件系统读取，线程安全）。
fn probe_repo(entry: GitBoardEntry) -> GitBoardEntry {
    let path = Path::new(&entry.workspace_path);
    if !path.exists() {
        return GitBoardEntry {
            is_repo: false,
            ..entry
        };
    }

    let repo = match git2::Repository::open(path) {
        Ok(r) => r,
        Err(_) => {
            return GitBoardEntry {
                is_repo: false,
                ..entry
            }
        }
    };

    let mut e = GitBoardEntry {
        is_repo: true,
        ..entry
    };

    // 分支 / 最近提交 / ahead-behind
    match repo.head() {
        Ok(head) => {
            if head.is_branch() {
                e.branch = head.shorthand().map(|s| s.to_string());
            } else {
                e.detached = true;
            }

            if let Ok(commit) = head.peel_to_commit() {
                e.last_commit_message = commit.summary().map(|s| s.to_string());
                let full_id = commit.id().to_string();
                e.last_commit_short_id = Some(full_id.chars().take(7).collect());
                if let Some(dt) = chrono::DateTime::from_timestamp(commit.time().seconds(), 0) {
                    let beijing = dt.with_timezone(&FixedOffset::east_opt(8 * 3600).unwrap());
                    e.last_commit_time = Some(beijing.to_rfc3339_opts(chrono::SecondsFormat::Secs, false));
                }
            }

            if let (Some(branch), Some(local_oid)) = (e.branch.as_deref(), head.target()) {
                let upstream_ref = repo
                    .branch_upstream_name(&format!("refs/heads/{}", branch))
                    .ok()
                    .and_then(|b| b.as_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| format!("refs/remotes/origin/{}", branch));
                if let Ok(remote) = repo.find_reference(&upstream_ref) {
                    if let Some(remote_oid) = remote.target() {
                        if let Ok((ahead, behind)) = repo.graph_ahead_behind(local_oid, remote_oid) {
                            e.ahead = Some(ahead as u32);
                            e.behind = Some(behind as u32);
                        }
                    }
                }
            }
        }
        Err(_) => {
            // 空仓库（尚无任何提交）或 HEAD 损坏：保留 is_repo，状态字段留空
        }
    }

    // 远端信息：优先 origin，否则第一个远端；跟踪分支从 upstream ref 取
    if let Ok(names) = repo.remotes() {
        let list: Vec<String> = names.iter().flatten().map(|s| s.to_string()).collect();
        let picked = list
            .iter()
            .find(|n| *n == "origin")
            .or_else(|| list.first())
            .cloned();
        if let Some(n) = picked {
            if let Ok(remote) = repo.find_remote(&n) {
                e.remote_url = remote.url().map(|s| s.to_string());
                e.remote_name = Some(n);
            }
        }
    }
    if let Some(branch) = e.branch.as_deref() {
        if let Ok(up) = repo.branch_upstream_name(&format!("refs/heads/{}", branch)) {
            e.upstream = up
                .as_str()
                .map(|s| s.trim_start_matches("refs/remotes/").to_string());
        }
    }

    // 工作区状态统计
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(false)
        .exclude_submodules(true);
    if let Ok(statuses) = repo.statuses(Some(&mut opts)) {
        for st in statuses.iter() {
            let s = st.status();
            if s.is_index_new()
                || s.is_index_modified()
                || s.is_index_deleted()
                || s.is_index_renamed()
                || s.is_index_typechange()
            {
                e.staged += 1;
            }
            if s.is_conflicted() {
                e.conflicts += 1;
            } else if s.is_wt_new() {
                e.untracked += 1;
            } else if s.is_wt_modified()
                || s.is_wt_deleted()
                || s.is_wt_renamed()
                || s.is_wt_typechange()
            {
                e.unstaged += 1;
            }
        }
    }

    e
}

/// 采集 Git 看板数据：先查活跃工作区，再并行探测各仓库状态。
pub fn collect_git_board(conn: &Connection, days: i64) -> Result<GitBoardResponse> {
    let items = query_active_workspaces(conn, days)?;

    let mut entries: Vec<GitBoardEntry> = Vec::with_capacity(items.len());
    if !items.is_empty() {
        let threads = items.len().min(8);
        let chunk_size = (items.len() + threads - 1) / threads;
        std::thread::scope(|s| {
            let handles: Vec<_> = items
                .chunks(chunk_size)
                .map(|chunk| s.spawn(move || chunk.iter().cloned().map(probe_repo).collect::<Vec<_>>()))
                .collect();
            for h in handles {
                if let Ok(mut part) = h.join() {
                    entries.append(&mut part);
                }
            }
        });
    }

    // 目录不存在 / 非 git 仓库的工作区：不展示、不统计
    entries.retain(|e| e.is_repo);

    // 按最后修改时间（会话活动 / 最近提交取最新）倒序排列
    let parse_time = |s: &Option<String>| -> Option<chrono::DateTime<chrono::Utc>> {
        s.as_deref()
            .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
    };
    entries.sort_by_key(|e| {
        let last = parse_time(&e.last_activity).max(parse_time(&e.last_commit_time));
        std::cmp::Reverse(last)
    });

    let repo_count = entries.len();
    let dirty_count = entries
        .iter()
        .filter(|e| (e.staged + e.unstaged + e.untracked + e.conflicts) > 0)
        .count();
    let ahead_count = entries.iter().filter(|e| e.ahead.unwrap_or(0) > 0).count();

    let summary = GitBoardSummary {
        days,
        total: entries.len(),
        repo_count,
        dirty_count,
        ahead_count,
        generated_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
    };

    Ok(GitBoardResponse { entries, summary })
}

#[tauri::command]
pub async fn get_git_board(days: Option<i64>) -> Result<GitBoardResponse, String> {
    let days = days.unwrap_or(30).clamp(1, 365);
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open_read_connection().map_err(|e| e.to_string())?;
        collect_git_board(&conn, days).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 探测单个工作区的 Git 状态（项目分析页 → 单项目变更视图）。
/// 不依赖会话活跃窗口，任意 git 目录均可。
#[tauri::command]
pub async fn get_git_workspace_entry(workspace_path: String) -> Result<GitBoardEntry, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = Path::new(&workspace_path);
        if !path.exists() {
            return Err(format!("目录不存在: {}", workspace_path));
        }
        let display_name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| workspace_path.clone());
        let skeleton = GitBoardEntry {
            workspace_path,
            display_name,
            is_repo: false,
            branch: None,
            detached: false,
            ahead: None,
            behind: None,
            remote_name: None,
            remote_url: None,
            upstream: None,
            staged: 0,
            unstaged: 0,
            untracked: 0,
            conflicts: 0,
            last_commit_message: None,
            last_commit_time: None,
            last_commit_short_id: None,
            last_activity: None,
            conversation_count: 0,
        };
        Ok(probe_repo(skeleton))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 单条提交记录（看板详情面板展示用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCommitInfo {
    /// 完整提交 ID
    pub id: String,
    /// 提交信息首行
    pub message: String,
    /// 短哈希（7 位）
    pub short_id: String,
    /// 作者名
    pub author: Option<String>,
    /// 提交时间（北京时间 RFC3339）
    pub time: String,
}

/// 获取指定仓库最近的提交记录（从 HEAD 按时间倒序，最多 limit 条）。
#[tauri::command]
pub async fn get_git_commits(
    workspace_path: String,
    limit: Option<i64>,
) -> Result<Vec<GitCommitInfo>, String> {
    let limit = limit.unwrap_or(20).clamp(1, 100);
    tauri::async_runtime::spawn_blocking(move || {
        let path = Path::new(&workspace_path);
        if !path.exists() {
            return Err("目录不存在".to_string());
        }
        let repo = git2::Repository::open(path).map_err(|e| format!("打开仓库失败: {}", e))?;

        // 空仓库（尚无任何提交）：返回空列表
        if repo.head().is_err() {
            return Ok(Vec::new());
        }

        let mut revwalk = repo.revwalk().map_err(|e| e.to_string())?;
        revwalk
            .set_sorting(git2::Sort::TIME)
            .map_err(|e| e.to_string())?;
        revwalk.push_head().map_err(|e| e.to_string())?;

        let beijing = FixedOffset::east_opt(8 * 3600).unwrap();
        let mut commits = Vec::new();
        for oid in revwalk.take(limit as usize).flatten() {
            let Ok(commit) = repo.find_commit(oid) else {
                continue;
            };
            let full_id = commit.id().to_string();
            commits.push(GitCommitInfo {
                id: full_id.clone(),
                message: commit.summary().unwrap_or("").to_string(),
                short_id: full_id.chars().take(7).collect(),
                author: commit.author().name().map(|s| s.to_string()),
                time: chrono::DateTime::from_timestamp(commit.time().seconds(), 0)
                    .map(|dt| {
                        dt.with_timezone(&beijing)
                            .to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
                    })
                    .unwrap_or_default(),
            });
        }
        Ok(commits)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 单个未提交文件的状态明细。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitStatusFile {
    /// 相对仓库根的路径（rename 时为新路径）
    pub path: String,
    /// rename 的原路径
    pub old_path: Option<String>,
    /// 暂存区状态字母（A/M/D/R/T），无暂存变更时 None
    pub staged: Option<String>,
    /// 工作区状态字母（M/D/R/T/U），未跟踪为 "?"，无变更时 None
    pub worktree: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitStatusFilesResponse {
    pub files: Vec<GitStatusFile>,
    /// 未提交文件总数（可能超过返回的 files 条数）
    pub total: usize,
}

/// 排序优先级：冲突 > 未跟踪 > 未暂存 > 仅暂存，同类按路径。
fn status_file_rank(f: &GitStatusFile) -> u8 {
    match f.worktree.as_deref() {
        Some("U") => 0,
        Some("?") => 1,
        Some(_) => 2,
        None => 3,
    }
}

/// 获取指定仓库当前未提交的文件列表（详情面板按需加载，最多 limit 条）。
#[tauri::command]
pub async fn get_git_status_files(
    workspace_path: String,
    limit: Option<usize>,
) -> Result<GitStatusFilesResponse, String> {
    let limit = limit.unwrap_or(200).clamp(1, 1000);
    tauri::async_runtime::spawn_blocking(move || {
        let path = Path::new(&workspace_path);
        if !path.exists() {
            return Err("目录不存在".to_string());
        }
        let repo = git2::Repository::open(path).map_err(|e| format!("打开仓库失败: {}", e))?;

        let mut opts = git2::StatusOptions::new();
        // 递归列出未跟踪目录内的文件，避免只报 `bin/` 这类目录项导致前端文件名为空
        opts.include_untracked(true)
            .recurse_untracked_dirs(true)
            .exclude_submodules(true);
        let statuses = repo
            .statuses(Some(&mut opts))
            .map_err(|e| format!("读取状态失败: {}", e))?;

        let mut files: Vec<GitStatusFile> = Vec::new();
        for st in statuses.iter() {
            let s = st.status();
            let staged = if s.is_index_new() {
                Some("A")
            } else if s.is_index_modified() {
                Some("M")
            } else if s.is_index_deleted() {
                Some("D")
            } else if s.is_index_renamed() {
                Some("R")
            } else if s.is_index_typechange() {
                Some("T")
            } else {
                None
            };
            let worktree = if s.is_conflicted() {
                Some("U")
            } else if s.is_wt_new() {
                Some("?")
            } else if s.is_wt_modified() {
                Some("M")
            } else if s.is_wt_deleted() {
                Some("D")
            } else if s.is_wt_renamed() {
                Some("R")
            } else if s.is_wt_typechange() {
                Some("T")
            } else {
                None
            };
            if staged.is_none() && worktree.is_none() {
                continue;
            }
            // rename 时从 delta 取原路径（未开启 rename 检测时通常为 None）
            let old_path = if s.is_index_renamed() || s.is_wt_renamed() {
                st.head_to_index()
                    .or_else(|| st.index_to_workdir())
                    .and_then(|d| d.old_file().path().map(|p| p.display().to_string()))
            } else {
                None
            };
            // 路径去掉尾部 `/`：未跟踪目录有时会以 `apps/core/bin/` 形式返回
            let raw_path = st.path().unwrap_or_default().to_string();
            let path = raw_path.trim_end_matches('/').to_string();
            files.push(GitStatusFile {
                path,
                old_path,
                staged: staged.map(str::to_string),
                worktree: worktree.map(str::to_string),
            });
        }

        let total = files.len();
        files.sort_by(|a, b| {
            status_file_rank(a)
                .cmp(&status_file_rank(b))
                .then_with(|| a.path.cmp(&b.path))
        });
        files.truncate(limit);

        Ok(GitStatusFilesResponse { files, total })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 单文件 diff 响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitFileDiffResponse {
    /// unified diff 文本（二进制文件为空）
    pub diff: String,
    /// 是否二进制文件
    pub binary: bool,
    /// diff 是否因过长被截断
    pub truncated: bool,
}

/// 获取单个文件的未提交 diff（详情面板点击查看）。
/// staged=true: HEAD vs 暂存区；
/// staged=false: 暂存区 vs 工作区（等价 `git diff`；未跟踪文件输出全内容）。
#[tauri::command]
pub async fn get_git_file_diff(
    workspace_path: String,
    path: String,
    staged: bool,
) -> Result<GitFileDiffResponse, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = Path::new(&workspace_path);
        if !root.exists() {
            return Err("目录不存在".to_string());
        }
        let repo = git2::Repository::open(root).map_err(|e| format!("打开仓库失败: {}", e))?;

        // recurse_untracked_dirs(true) 是必须的：libgit2 在 false 时 pathspec 匹配不到未跟踪文件
        let mut opts = git2::DiffOptions::new();
        opts.pathspec(&path)
            .include_untracked(true)
            .show_untracked_content(true)
            .recurse_untracked_dirs(true);

        let diff = if staged {
            // HEAD vs 暂存区（空仓库无 HEAD 时与空树比较）
            let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
            repo.diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))
                .map_err(|e| format!("生成 diff 失败: {}", e))?
        } else {
            // 暂存区 vs 工作区（git diff）
            // 注意：不能用 diff_tree_to_workdir_with_index(None)，
            // 那会把旧侧当成空树，修改过的文件会被整文件当成新增。
            repo.diff_index_to_workdir(None, Some(&mut opts))
                .map_err(|e| format!("生成 diff 失败: {}", e))?
        };

        // 收集 unified diff 文本，超过上限主动中止
        const MAX_DIFF_BYTES: usize = 256 * 1024;
        let mut text = String::new();
        let mut truncated = false;
        let print_res = diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            if text.len() >= MAX_DIFF_BYTES {
                truncated = true;
                return false;
            }
            match line.origin() {
                '+' | '-' | ' ' => text.push(line.origin()),
                _ => {}
            }
            text.push_str(&String::from_utf8_lossy(line.content()));
            true
        });
        if let Err(e) = print_res {
            if !truncated {
                return Err(format!("生成 diff 失败: {}", e));
            }
        }

        // print 过程会填充二进制检测标志
        let binary = diff
            .deltas()
            .any(|d| d.flags().contains(git2::DiffFlags::BINARY));

        Ok(GitFileDiffResponse {
            diff: text,
            binary,
            truncated,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/* ===================== 提交与推送（写操作） ===================== */

/// 加入 .gitignore 的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitAddIgnoreResponse {
    /// 实际使用的规则文本
    pub pattern: String,
    /// 本次是否新写入（规则已存在时为 false）
    pub added: bool,
    /// 路径是否已被 git 跟踪（已跟踪文件写入规则后不会立即生效）
    pub tracked: bool,
}

/// 转义 gitignore 模式中的特殊字符（fnmatch 元字符与注释符）。
fn escape_gitignore_path(p: &str) -> String {
    let mut out = String::with_capacity(p.len());
    for ch in p.chars() {
        match ch {
            '\\' | '*' | '?' | '[' | ']' | '#' | '!' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

/// 把文件或目录加入仓库根 .gitignore（追加规则，不碰索引）。
#[tauri::command]
pub async fn add_git_ignore(workspace_path: String, path: String) -> Result<GitAddIgnoreResponse, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = Path::new(&workspace_path);
        if !root.exists() {
            return Err("目录不存在".to_string());
        }
        let rel = path.trim_end_matches('/').to_string();
        if rel.is_empty()
            || rel.starts_with('/')
            || rel.split('/').any(|seg| seg == ".." || seg.is_empty())
        {
            return Err("无效路径".to_string());
        }
        let repo = git2::Repository::open(root).map_err(|e| format!("打开仓库失败: {}", e))?;
        let workdir = repo
            .workdir()
            .ok_or_else(|| "无法获取仓库工作目录".to_string())?;

        let is_dir = root.join(&rel).is_dir();
        let tracked = repo
            .index()
            .map(|idx| idx.get_path(Path::new(&rel), 0).is_some())
            .unwrap_or(false);
        let mut pattern = escape_gitignore_path(&rel);
        if is_dir {
            pattern.push('/');
        }

        let gitignore = workdir.join(".gitignore");
        let mut content = std::fs::read_to_string(&gitignore).unwrap_or_default();
        let exists = content.lines().any(|l| l.trim_end() == pattern);
        if !exists {
            if !content.is_empty() && !content.ends_with('\n') {
                content.push('\n');
            }
            content.push_str(&pattern);
            content.push('\n');
            std::fs::write(&gitignore, &content).map_err(|e| format!("写入 .gitignore 失败: {}", e))?;
        }
        Ok(GitAddIgnoreResponse {
            pattern,
            added: !exists,
            tracked,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 暂存全部变更（等价 git add -A：含删除，不含 ignored 文件）。
#[tauri::command]
pub async fn git_stage_all(workspace_path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = Path::new(&workspace_path);
        if !root.exists() {
            return Err("目录不存在".to_string());
        }
        let repo = git2::Repository::open(root).map_err(|e| format!("打开仓库失败: {}", e))?;
        let mut index = repo.index().map_err(|e| format!("读取暂存区失败: {}", e))?;
        index
            .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
            .map_err(|e| format!("暂存失败: {}", e))?;
        index.write().map_err(|e| format!("写入暂存区失败: {}", e))?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 提交当前暂存区内容，返回新提交短哈希。
/// 作者身份取仓库配置（含全局），未配置时给出明确报错。
#[tauri::command]
pub async fn git_commit(workspace_path: String, message: String) -> Result<String, String> {
    let message = message.trim().to_string();
    if message.is_empty() {
        return Err("提交信息不能为空".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let root = Path::new(&workspace_path);
        if !root.exists() {
            return Err("目录不存在".to_string());
        }
        let repo = git2::Repository::open(root).map_err(|e| format!("打开仓库失败: {}", e))?;
        let sig = repo.signature().map_err(|_| {
            "未找到 git 用户名/邮箱，请先执行 git config --global user.name / user.email 配置".to_string()
        })?;

        let mut index = repo.index().map_err(|e| format!("读取暂存区失败: {}", e))?;
        let tree_oid = index.write_tree().map_err(|e| format!("写入树失败: {}", e))?;
        let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
        if head_tree.as_ref().map(|t| t.id()) == Some(tree_oid) {
            return Err("没有可提交的变更".to_string());
        }
        let tree = repo.find_tree(tree_oid).map_err(|e| e.to_string())?;

        let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();

        let oid = repo
            .commit(Some("HEAD"), &sig, &sig, &message, &tree, &parents)
            .map_err(|e| format!("提交失败: {}", e))?;
        Ok(oid.to_string().chars().take(7).collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 推送到远端：shell out 到 git CLI，直接复用用户机器上已有的凭证配置
/// （ssh-agent / osxkeychain / credential helper），git2 需自行实现整套凭证回调。
#[tauri::command]
pub async fn git_push(workspace_path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let output = std::process::Command::new("git")
            .arg("push")
            .current_dir(&workspace_path)
            .output()
            .map_err(|e| format!("无法执行 git 命令: {}", e))?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("推送失败: {}", stderr.trim()))
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 待提交变更总览（HEAD vs 工作区含暂存区），供 AI 生成 commit 信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitPendingDiffResponse {
    /// 变更文件相对路径列表
    pub files: Vec<String>,
    /// unified diff 文本（超长截断）
    pub diff: String,
    pub truncated: bool,
}

#[tauri::command]
pub async fn get_git_pending_diff(workspace_path: String) -> Result<GitPendingDiffResponse, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = Path::new(&workspace_path);
        if !root.exists() {
            return Err("目录不存在".to_string());
        }
        let repo = git2::Repository::open(root).map_err(|e| format!("打开仓库失败: {}", e))?;

        let mut opts = git2::DiffOptions::new();
        opts.include_untracked(true)
            .show_untracked_content(true)
            .recurse_untracked_dirs(false);
        let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
        let diff = repo
            .diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut opts))
            .map_err(|e| format!("生成 diff 失败: {}", e))?;

        let files: Vec<String> = diff
            .deltas()
            .filter_map(|d| d.new_file().path().map(|p| p.display().to_string()))
            .collect();

        const MAX_DIFF_BYTES: usize = 16 * 1024;
        let mut text = String::new();
        let mut truncated = false;
        let print_res = diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            if text.len() >= MAX_DIFF_BYTES {
                truncated = true;
                return false;
            }
            match line.origin() {
                '+' | '-' | ' ' => text.push(line.origin()),
                _ => {}
            }
            text.push_str(&String::from_utf8_lossy(line.content()));
            true
        });
        if let Err(e) = print_res {
            if !truncated {
                return Err(format!("生成 diff 失败: {}", e));
            }
        }

        Ok(GitPendingDiffResponse {
            files,
            diff: text,
            truncated,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/* ===================== 历史提交查看 ===================== */

/// 提交内的单个变更文件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCommitFile {
    pub path: String,
    /// 变更类型字母：A/M/D/R/T
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCommitShowResponse {
    pub short_id: String,
    pub message: String,
    pub author: Option<String>,
    pub time: String,
    pub files: Vec<GitCommitFile>,
}

/// 提交相对其父提交的 diff 采集结果（初始提交则与空树比较）。
struct CommitDiffData {
    files: Vec<GitCommitFile>,
    diff: String,
    truncated: bool,
    binary: bool,
}

/// cap 为 diff 文本字节上限，0 表示只采集文件清单不生成文本。
fn collect_commit_diff(
    repo: &git2::Repository,
    commit: &git2::Commit,
    pathspec: Option<&str>,
    cap: usize,
) -> Result<CommitDiffData, String> {
    let new_tree = commit.tree().map_err(|e| format!("读取提交树失败: {}", e))?;
    let old_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());
    let mut opts = git2::DiffOptions::new();
    if let Some(p) = pathspec {
        opts.pathspec(p);
    }
    let diff = repo
        .diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), Some(&mut opts))
        .map_err(|e| format!("生成 diff 失败: {}", e))?;

    let files: Vec<GitCommitFile> = diff
        .deltas()
        .map(|d| {
            let status = match d.status() {
                git2::Delta::Added => "A",
                git2::Delta::Deleted => "D",
                git2::Delta::Renamed => "R",
                git2::Delta::Typechange => "T",
                _ => "M",
            };
            GitCommitFile {
                path: d.new_file().path().map(|p| p.display().to_string()).unwrap_or_default(),
                status: status.to_string(),
            }
        })
        .collect();

    let mut text = String::new();
    let mut truncated = false;
    if cap > 0 {
        let print_res = diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            if text.len() >= cap {
                truncated = true;
                return false;
            }
            match line.origin() {
                '+' | '-' | ' ' => text.push(line.origin()),
                _ => {}
            }
            text.push_str(&String::from_utf8_lossy(line.content()));
            true
        });
        if let Err(e) = print_res {
            if !truncated {
                return Err(format!("生成 diff 失败: {}", e));
            }
        }
    }

    let binary = diff
        .deltas()
        .any(|d| d.flags().contains(git2::DiffFlags::BINARY));

    Ok(CommitDiffData {
        files,
        diff: text,
        truncated,
        binary,
    })
}

/// 查看某次提交的元信息与变更文件清单。
#[tauri::command]
pub async fn get_git_commit_show(
    workspace_path: String,
    commit_id: String,
) -> Result<GitCommitShowResponse, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = Path::new(&workspace_path);
        if !root.exists() {
            return Err("目录不存在".to_string());
        }
        let repo = git2::Repository::open(root).map_err(|e| format!("打开仓库失败: {}", e))?;
        let oid = git2::Oid::from_str(&commit_id).map_err(|e| format!("无效的提交 ID: {}", e))?;
        let commit = repo.find_commit(oid).map_err(|e| format!("读取提交失败: {}", e))?;

        let data = collect_commit_diff(&repo, &commit, None, 0)?;

        // 先取成 owned 局部变量：Signature 等临时值的 Drop 不能晚于 repo/commit
        let short_id: String = commit.id().to_string().chars().take(7).collect();
        let message = commit.summary().unwrap_or("").to_string();
        let author = commit.author().name().map(|s| s.to_string());
        let beijing = FixedOffset::east_opt(8 * 3600).unwrap();
        let time = chrono::DateTime::from_timestamp(commit.time().seconds(), 0)
            .map(|dt| {
                dt.with_timezone(&beijing)
                    .to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
            })
            .unwrap_or_default();

        Ok(GitCommitShowResponse {
            short_id,
            message,
            author,
            time,
            files: data.files,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 查看某次提交中单个文件的 diff。
#[tauri::command]
pub async fn get_git_commit_file_diff(
    workspace_path: String,
    commit_id: String,
    path: String,
) -> Result<GitFileDiffResponse, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = Path::new(&workspace_path);
        if !root.exists() {
            return Err("目录不存在".to_string());
        }
        let repo = git2::Repository::open(root).map_err(|e| format!("打开仓库失败: {}", e))?;
        let oid = git2::Oid::from_str(&commit_id).map_err(|e| format!("无效的提交 ID: {}", e))?;
        let commit = repo.find_commit(oid).map_err(|e| format!("读取提交失败: {}", e))?;

        const MAX_DIFF_BYTES: usize = 256 * 1024;
        let data = collect_commit_diff(&repo, &commit, Some(&path), MAX_DIFF_BYTES)?;

        Ok(GitFileDiffResponse {
            diff: data.diff,
            binary: data.binary,
            truncated: data.truncated,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/* ===================== 全部文件树（只读浏览，懒加载） ===================== */

/// 校验前端传来的工作区相对路径：禁止绝对路径与 `..` 穿越。
fn validate_rel_path(rel: &str) -> Result<(), String> {
    if rel.starts_with('/') || rel.split('/').any(|seg| seg == "..") {
        return Err("无效路径".to_string());
    }
    Ok(())
}

/// 目录下单个条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceDirEntry {
    pub name: String,
    /// 相对工作区根的路径（'/' 分隔）
    pub path: String,
    pub is_dir: bool,
}

/// 懒加载列出目录一层内容：仅跳过 .git，目录在前、名称不区分大小写排序。
#[tauri::command]
pub async fn list_workspace_dir(
    workspace_path: String,
    rel_path: Option<String>,
) -> Result<Vec<WorkspaceDirEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = Path::new(&workspace_path);
        if !root.exists() {
            return Err("目录不存在".to_string());
        }
        let rel = rel_path.unwrap_or_default();
        validate_rel_path(&rel)?;
        let dir = if rel.is_empty() {
            root.to_path_buf()
        } else {
            root.join(&rel)
        };
        let read = std::fs::read_dir(&dir).map_err(|e| format!("读取目录失败: {}", e))?;

        // 单目录条目上限，防御异常巨型目录
        const MAX_ENTRIES: usize = 5000;
        let mut entries: Vec<WorkspaceDirEntry> = Vec::new();
        for item in read.flatten() {
            let name = item.file_name().to_string_lossy().to_string();
            if name == ".git" {
                continue;
            }
            let is_dir = item
                .file_type()
                .map(|t| t.is_dir())
                .unwrap_or(false);
            let path = if rel.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", rel, name)
            };
            entries.push(WorkspaceDirEntry { name, path, is_dir });
            if entries.len() >= MAX_ENTRIES {
                break;
            }
        }
        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(entries)
    })
    .await
    .map_err(|e| e.to_string())?
}

/* ===================== 搜索与文件级暂存 ===================== */

/// 执行 git CLI（cwd = 仓库根），失败时返回 stderr。
fn run_git(workspace_path: &str, args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(workspace_path)
        .output()
        .map_err(|e| format!("无法执行 git 命令: {}", e))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// 搜索工作区文件路径（跟踪 + 未跟踪，不含 ignored），大小写不敏感子串匹配。
#[tauri::command]
pub async fn search_workspace_files(
    workspace_path: String,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return Ok(Vec::new());
        }
        let root = Path::new(&workspace_path);
        if !root.exists() {
            return Err("目录不存在".to_string());
        }
        let repo = git2::Repository::open(root).map_err(|e| format!("打开仓库失败: {}", e))?;
        let cap = limit.unwrap_or(100).clamp(1, 500);

        let mut paths: Vec<String> = Vec::new();
        if let Ok(index) = repo.index() {
            for entry in index.iter() {
                paths.push(String::from_utf8_lossy(&entry.path).into_owned());
            }
        }
        // 未跟踪文件（递归进新目录）
        let mut status_opts = git2::StatusOptions::new();
        status_opts
            .include_untracked(true)
            .recurse_untracked_dirs(true);
        if let Ok(statuses) = repo.statuses(Some(&mut status_opts)) {
            for s in statuses.iter() {
                if s.status().contains(git2::Status::WT_NEW) {
                    if let Some(p) = s.path() {
                        paths.push(p.to_string());
                    }
                }
            }
        }

        let mut out: Vec<String> = paths
            .into_iter()
            .filter(|p| p.to_lowercase().contains(&needle))
            .collect();
        out.sort();
        out.dedup();
        out.truncate(cap);
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 暂存单个文件或目录（含删除；等价 git add -A -- <path>）。
#[tauri::command]
pub async fn git_stage_file(workspace_path: String, path: String) -> Result<(), String> {
    validate_rel_path(&path)?;
    tauri::async_runtime::spawn_blocking(move || {
        run_git(&workspace_path, &["add", "-A", "--", &path]).map(|_| ())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 取消暂存单个文件（回到 HEAD 状态；新文件变回未跟踪）。
#[tauri::command]
pub async fn git_unstage_file(workspace_path: String, path: String) -> Result<(), String> {
    validate_rel_path(&path)?;
    tauri::async_runtime::spawn_blocking(move || {
        run_git(&workspace_path, &["restore", "--staged", "--", &path]).map(|_| ())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// fetch 当前仓库远端，使 ahead/behind 反映最新状态。
#[tauri::command]
pub async fn git_fetch(workspace_path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || run_git(&workspace_path, &["fetch"]))
        .await
        .map_err(|e| e.to_string())?
}

/// 文件内容预览响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceFileContent {
    pub content: String,
    pub truncated: bool,
    pub binary: bool,
    pub size: u64,
}

/// 读取工作区内单个文件（只读预览，超大文件截断，二进制不返回内容）。
#[tauri::command]
pub async fn read_workspace_file(
    workspace_path: String,
    path: String,
) -> Result<WorkspaceFileContent, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = Path::new(&workspace_path);
        if !root.exists() {
            return Err("目录不存在".to_string());
        }
        validate_rel_path(&path)?;
        let file = root.join(&path);
        let meta = std::fs::metadata(&file).map_err(|e| format!("读取文件失败: {}", e))?;
        if !meta.is_file() {
            return Err("目标不是文件".to_string());
        }
        let size = meta.len();

        const MAX_BYTES: u64 = 512 * 1024;
        use std::io::Read;
        let mut buf: Vec<u8> = Vec::with_capacity((size.min(MAX_BYTES)) as usize + 1);
        if size > MAX_BYTES {
            let mut f = std::fs::File::open(&file).map_err(|e| e.to_string())?;
            f.by_ref()
                .take(MAX_BYTES)
                .read_to_end(&mut buf)
                .map_err(|e| e.to_string())?;
        } else {
            buf = std::fs::read(&file).map_err(|e| e.to_string())?;
        }

        let binary = buf.iter().take(8000).any(|b| *b == 0);
        Ok(WorkspaceFileContent {
            content: if binary { String::new() } else { String::from_utf8_lossy(&buf).into_owned() },
            truncated: size > MAX_BYTES,
            binary,
            size,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
