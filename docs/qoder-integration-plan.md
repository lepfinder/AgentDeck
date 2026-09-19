# Qoder 接入 AgentDeck —— 现状核查与接入方案评估

> 评估日期：2026-09-19
> 评估对象：AgentDeck（`/Users/xiyangxie/workspace/personal/AgentDeck`）
> 核查方式：macOS 本机实盘数据直读（Qoder IDE v1.30.1 / Qoder 桌面端 v0.3.3 / qodercli v1.1.53）

---

## 一、结论速览

| 问题 | 结论 |
|---|---|
| 现在接入 Qoder 了吗？ | **没有。** 全仓对 `qoder` 大小写不敏感搜索 **0 命中** |
| 当前接入了哪些 agent？ | 共 **9 个**：`cursor`、`antigravity`、`claude`、`codex`、`hermes`、`workbuddy`、`mimo`、`windsurf`、`codebuddy` |
| Qoder 本地有会话记录吗？ | **有，且结构清晰。** 位于 `~/.qoder/projects/<目录名>/<sessionId>.jsonl`，是 **Claude Code 兼容的追加式 JSONL** |
| 能否做到与 Claude Code 对等？ | **能，部分更强**：显式 workspace 路径、thinking、逐条 tool_use、tool_result、模型名、**credits 积分**；唯一短板是 token 计数恒为 0 |
| 主要技术难点 | **几乎没有**。工作区路径在文件内显式声明，不需要 CodeBuddy 那种 md5 反查；主要工作量在 assistant 分块流的合并解析 |
| 推荐路线 | 直接做**完整 transcript importer**（`qoder.rs`）+ **IDE 启动器** |

> 说明：本机 Qoder 首次使用即 2026-09-19，历史数据只有 2 个会话（均在本仓库），
> 评估结论以**格式结构**为主、**样本数量**为辅，见第八章风险 2。

---

## 二、真正的数据源

### 2.1 三种产品形态，一个数据家目录

| 形态 | 安装位置 | 版本 | 会话数据 |
|---|---|---|---|
| Qoder IDE（VS Code 分支） | `/Applications/Qoder IDE.app`（bundle id `com.qoder.ide`） | 1.30.1 | 其旧版 chat panel 记录 `/Application Support/Qoder/User/workspaceStorage/*/chatSessions/*.json`（**旧格式，价值低**，见第九章） |
| Qoder 桌面端（Electron） | `/Applications/Qoder.app`（bundle id `com.qoder.app`） | 0.3.3 | userData `~/Library/Application Support/com.qoder.app.stable/main.sqlite`（标题、turn 级投影） |
| qodercli（CLI） | 本机**未独立安装**（不在 PATH，`~/.local/bin/qodercli`、`~/.qoder/bin/qodercli/qodercli` 均不存在）；桌面端以 `sdk`（stream-json）模式内嵌运行 | 1.1.53 | **`~/.qoder/projects/**/*.jsonl`** |

**关键结论：三者共享同一个 `~/.qoder/` 家目录，会话正文统一落在 `~/.qoder/projects/`。**
本机实测：JSONL 由桌面端内嵌的 CLI runtime 写出（日志 `~/.qoder/logs/runs/*/qodercli.log`
可见 `mode="sdk" input_format="stream-json"`），**不依赖用户是否安装独立 CLI**。

### 2.2 目录全景

```
~/.qoder/
├── entry/qoder                        # CLI 调度脚本（本机未安装 qodercli，候选路径全部落空）
├── settings.json  mcp.json            # 配置
├── projects/                          # ★ 会话正文根目录
│   └── -Users-xiyangxie-workspace-personal-AgentDeck/   # dash 编码的 cwd（不可逆，勿解析）
│       ├── <sessionId>.jsonl          # ★ 完整会话流（追加写）
│       ├── <sessionId>/compression-v2/  # 压缩状态（加密字段，死路）
│       └── memory/*.md                # Agent 记忆（非会话，勿导入）
├── tmp/<dash 编码同上>/images/<sessionId>/<uuid>.png   # ★ 用户附图本地副本
├── tasks/<sessionId>/                 # 实测空
├── logs/runs/*/qodercli.log           # 运行日志（非正文）
└── memories/  memory/  skills/  plugins/  agents/  ... # 配置与记忆

~/Library/Application Support/
├── Qoder/                             # Qoder IDE userData（chatSessions 旧格式）
└── com.qoder.app.stable/              # Qoder 桌面端 userData
    └── main.sqlite                    # 会话标题 / turn 级投影 / workspaces / 文件变更（WAL）
```

### 2.3 本机实测数据量

| 指标 | 数值（评估时快照，会话仍在写入） |
|---|---|
| `~/.qoder/projects/` 项目目录数 | **14** |
| 含 JSONL 会话的项目 | **1**（`…workspace-personal-AgentDeck`） |
| JSONL 会话文件数 | **2**（`6f470d9b…` 1.4 MB / 508 行；`fc9bca17…` 271 KB / 80 行） |
| 其余 13 个目录 | 只有 `memory/*.md`（agent 记忆，**非会话**），且 mtime 全部为当日 15:35 |
| 桌面端 `main.sqlite` | `chat_sessions` 2 行、`chat_session_messages` 6 行、`workspaces` 1 行、`turn_file_change_sets` **0** 行 |
| `turn_file_change_sets` 出现时机 | 只有产生文件改动的 turn 才会写入，当前会话未触发 |
| IDE `chatSessions/*.json` | 若干（2025-08 ~ 2025-09-10，VS Code chat-session v3 旧格式） |

会话 A（`6f470d9b…`，即本轮评估对话本身）行类型分布：
`assistant` ~194（其中 `tool_use` 块 ~97、`thinking` 块 ~70、`text` 块 ~29）、
`user` ~101（人类输入 1 / tool_result ~97 / 压缩摘要 2）、`attachment` ~22、
`active-leaf` ~186、`system` 2、`runtime-config` 2、`workspace-directories` 1。

> ⚠️ 数据量比 CodeBuddy（26 会话 / 1093 消息）小一个量级，原因是 **Qoder 在本机今天才首次使用**，
> 不是数据不落盘。格式本身是完整的。

---

## 三、Schema（全部实测）

### 3.1 JSONL 行类型总览

每行一个 JSON 对象，`type` 区分（8 种）：

| type | 作用 | 处理 |
|---|---|---|
| `workspace-directories` | `{"sessionId","directories":["/abs/cwd"]}` | ★ **工作区唯一权威来源**；重复出现时取首个非空 |
| `runtime-config` | `{"model":"auto","reasoningEffort","contextWindow","generation","timestamp"(ms)}` | 取 `model` 作兜底模型名 |
| `user` | 三种形态，见 3.2 | ★ 正文 |
| `assistant` | 单块分片流，见 3.3 | ★ 正文 |
| `attachment` | 系统注入（`skill_listing` / `hook_output` / `task_reminder` / `agent_listing_delta` / `critical_system_reminder` / `post_compact_restored_files`） | **跳过** |
| `system` | `subtype:"compact_boundary"`，压缩边界 | **跳过** |
| `active-leaf` | UI 书签（每个消息后一个） | **跳过** |
| `last-prompt` | 末条输入（仅用于标题） | 可作标题兜底 |

所有行公共字段：`uuid`、`parentUuid`、`timestamp`（ISO8601，多数）、`isSidechain`、
`cwd`、`sessionId`、`userType`、`entrypoint`（`"cli"`）、`version`（`"1.1.53"`）、`gitBranch`。

### 3.2 `user` 的三种形态

**(a) 人类输入** —— `origin.kind == "human"`，且带 `humanInput.text`：

```jsonc
{ "type": "user", "origin": {"kind":"human"}, "permissionMode": "yolo",
  "humanInput": { "text": "看看这个 git 看板的逻辑…", "mode": "prompt" },
  "message": { "role": "user", "content": [
      { "type": "image", "source": { "type": "url", "url": "https://qoder-vl-global.oss-…aliyuncs.com/…png?Expires=…&Signature=…" } },
      { "type": "text",  "text": "看看这个 git 看板的逻辑…\n\n附件引用：\n- 文件：/Users/xiyangxie/.qoder/tmp/…/images/<sessionId>/<uuid>.png" }
  ]},
  "uuid": "…", "timestamp": "2026-09-19T07:43:06.161Z", "cwd": "/Users/…/AgentDeck" }
```

> ⚠️ 图片 `url` 是**阿里云 OSS 签名链接，会过期**（本机样本 `Expires` 约 30 天）；
> **本地副本路径写在同条消息的文本里**（`附件引用：- 文件：<绝对路径>`），
> 实测该路径真实存在，应作为图片首选来源。

**(b) 工具结果** —— `origin` 为 `null`，`message.content` 仅含 `tool_result`：

```jsonc
{ "type": "user", "origin": null, "parentUuid": "…",
  "message": { "role": "user", "content": [
      { "type": "tool_result", "tool_use_id": "call_00_vWFRNCvjpMgKdAqG2jTR8790", "content": "<字符串或块数组>" }
  ]}}
```

→ 映射为 `role="tool"`、`message_type="tool_result"`，`tool_name` 通过 `tool_use_id` 关联。

**(c) 系统注入**（一律跳过）：
- `isCompactSummary: true`：压缩摘要（`"This session is being continued…"`）
- `isMeta: true`：图片尺寸占位（`"[Image: original 2760x1780, displayed at 2000x1290…]"`）

### 3.3 `assistant` 的分块流式结构（★ 与 Claude Code 最大的不同）

**每个 assistant 行只含一个内容块**（实测 508 行中无一例外），同一轮推理被拆成多行：

| 行内容 | 示例数量 | usage |
|---|---|---|
| `content:[{"type":"thinking","thinking":"…"}]` | ~70 | 无 |
| `content:[{"type":"text","text":"…"}]` | ~29 | 部分有 |
| `content:[{"type":"tool_use","id":"call_00_…","name":"Bash","input":{…}}]` | ~97 | 部分有 |

```jsonc
{ "type": "assistant", "timestamp": "2026-09-19T07:44:39.917Z",
  "message": { "id": "chatcmpl-…" | "<uuid>", "role": "assistant", "model": "auto",
    "content": [ { "type": "tool_use", "id": "call_00_…", "name": "Bash", "input": {…} } ],
    "usage": {                      // 仅最后一个分片带 usage
      "input_tokens": 0, "output_tokens": 0, "cache_read_input_tokens": 0,   // ★ 恒为 0
      "credits": 0.435701784, "original_credits": 0.435701784, "billable": true,
      "context_usage_ratio": 0.188265625, "request_id": "…"
    } } }
```

**解析要点**：
1. 连续 assistant 行需**按 chunk 合并**成一个"推理段"：`thinking` 拼接、
   `text` 拼接；每个 `tool_use` 单独产出一条 `tool_call` 消息（**不要**学 `claude.rs` 合并成 `multiple_tools`）。
2. `usage.credits` 是**每次推理的增量**（本机会话 A 共 73 处，合计 ≈ 18.08），
   不是累计值；应附着到该 step 的最后一条产出消息。
3. `input_tokens/output_tokens` **恒为 0**（`model:"auto"` 订阅制），**不要**指望 token 统计。
4. 工具名实测：`Bash` / `Read` / `Grep` / `WebFetch`。
5. `isSidechain: true`（子代理侧链）本机 0 样本；建议 v1 跳过，见风险 5。

### 3.4 桌面端 `main.sqlite`（可选增强，非正文源）

| 表 | 关键列 | 价值 |
|---|---|---|
| `chat_sessions` | `session_id`(PK，= JSONL 文件名)、`title`（**AI 生成标题**，如"git看板逻辑与UI建议"）、`cwd`、`workspace_id`、`model`、`permission_mode`、`git_branch`、`created_at/updated_at`、`extra_json`(含 `provisionalTitle`/`titleResolved`) | ★ 唯一值得 join 的字段：**标题**（JSONL 无 title）、`first_prompt` 兜底 |
| `chat_session_messages` | `session_id`、`sequence`、`payload_json`、`source` | **turn 级投影**（每轮一条 user + 一条 assistant，只有最终文本，无中间块）→ 不作正文源 |
| `workspaces` | `workspace_id`、`root_paths_json` | 与 `chat_sessions.cwd` 冗余，可不用 |
| `turn_file_change_sets` / `_files` / `_patches` | 文件变更集 | 本机 0 行；有改动时才写，可作 artifacts 增强 |
| `chat_session_search_fts` | FTS 索引 | 不需要 |

> 注意 `main.sqlite` 处于 **WAL 模式**，只读打开即可（建议 `busy_timeout`）。

---

## 四、核心难点取舍（对比 CodeBuddy）

**这次没有硬骨头** —— 与 CodeBuddy 的 `md5(cwd)` 三重反查完全不同：

| 维度 | CodeBuddy | Qoder |
|---|---|---|
| 工作区归属 | `md5(cwd)` 不可逆，需三重反查 | **文件内显式 `workspace-directories` / `cwd`**，直接可用 |
| 目录名编码 | 不可逆 | dash 编码同样**不可逆**（`-` 与 `/` 无法区分），但**无需解析**：本机存在 `-Users-…-workspace-qorder-xflow` 这类歧义目录名，恰好证明不能靠目录名反推 |
| 消息格式 | UIMessage 二次 parse + `<user_query>` 剥壳 | Claude Code 兼容 JSONL + **分块合并** |
| token/credit | 有 token + credit，但**表无列**需扩表 | token 恒 0；**credit 有现成列**（`messages.credit`，`db.rs:525` + `importers/mod.rs:455-538` 已写入） |
| 数据量 | 26 会话 / 1093 消息 | 2 会话（历史短，样本少） |

需要决策的取舍只有 5 个：

1. **无 title 字段** → 取首条 human prompt 前 60 字；可选 join 桌面端 AI 标题（更佳）。
2. **token 恒 0** → `token_count` 留空，`credit` 写 `usage.credits`（增量求和可做成本分析）。
3. **分块流** → 需要一个"推理段缓冲 + flush"的解析器（约 60 行）。
4. **`isSidechain` / 子代理** → 本机无样本，v1 默认跳过（保守），后续按需放开。
5. **会话仍在写入** → 末行可能是半个 JSON；单行 try/catch 跳过即可，下轮同步补齐。

---

## 五、字段映射（→ `importers/mod.rs` 的 Raw* 结构）

### 5.1 `RawConversation`

| 字段 | Qoder 来源 |
|---|---|
| `id` | `<sessionId>`（= JSONL 文件名 = 行内 `sessionId`） |
| `title` | 首条 human prompt 截 60 字；可选：桌面端 `chat_sessions.title` |
| `workspace_path` | `project_root_from_path(workspace-directories.directories[0])`，兜底任一行 `cwd` |
| `source_app` | `"qoder"` |
| `created_at` / `updated_at` | 首 / 末条正文行 `timestamp` |
| `parse_status` | `"ok"`；无正文行（只有 active-leaf）→ 跳过 |
| `source_types` | `["qoder"]`（与 claude/codebuddy 一致） |
| `messages` | 见 5.2 |
| `artifacts` | v1 留空；可选：桌面端 `turn_file_change_*` |

### 5.2 `RawMessage`

| 字段 | 来源 | 备注 |
|---|---|---|
| `step_index` | 产出顺序连续递增 | 一行 assistant 可产出多条 |
| `role` | human→`user`；tool_result→`tool`；assistant→`assistant` | |
| `message_type` | text / tool_call / tool_result | |
| `content` | text 块拼接；tool_use→`input` 的 pretty JSON；tool_result→结果文本（截断 4000） | 对齐 `codebuddy.rs::CONTENT_TRUNCATE` |
| `thinking` | 连续 thinking 块拼接 | 挂到该推理段首条消息 |
| `created_at` | 行 `timestamp` | |
| `model_name` | `message.model`（`"auto"`）→ 兜底 `runtime-config.model` | |
| `tool_name` / `tool_args` | tool_use 的 `name` / `input` | tool_result 用 `tool_use_id` 反查 |
| `token_count` | `None` | 实测恒 0，不写 |
| `credit` | `usage.credits` | ★ 列已存在，直接落库 |
| `images` | `[{"src","original_path","width","height"}]` JSON | 首选 `~/.qoder/tmp/.../<uuid>.png`；对齐 `cursor.rs` 的 images 结构 |

> **与 CodeBuddy 方案的关键差异**：`messages` 表已有
> `model_name / token_count / credit / images` 列（`db.rs:523-525`），
> `save_conversation_tx` 已写入并纳入变更检测（`importers/mod.rs:455-538`）。
> **Qoder 接入不需要任何扩表。**

---

## 六、推荐方案

**新建 `src-tauri/src/importers/qoder.rs`，实现完整 transcript importer。**

```
1. 枚举 ~/.qoder/projects/*/ 下的 *.jsonl（没有 .jsonl 的目录直接跳过 → 天然排除 memory-only 项目）
2. 逐行读取、按 type 分发：
   a. workspace-directories → cwd（工作区）
   b. runtime-config       → 兜底 model
   c. user/human           → flush 推理段；产出 user 消息（text + images）
   d. user/tool_result     → flush 推理段；产出 tool 消息（tool_use_id 反查工具名）
   e. user/isCompactSummary|isMeta → 跳过
   f. assistant            → 追加到"推理段缓冲"（thinking/text/tool_use 分块；记录 usage.credits）
   g. system/attachment/active-leaf/last-prompt → 跳过
3. flush 推理段：合并 thinking/text，逐个 tool_use 产出 tool_call 消息，credits 附着到段内最后一条
4. 用 needs_sync（mtime + size）+ 解析器版本号控制增量：
   const QODER_PARSER_REV: &str = "qoder-v1";
   sync_state key: "agentdeck:qoder_parser_rev"（对齐 codebuddy.rs 的 mark_*_synced 模式）
5. save_conversation_tx 落库
```

**同步做 IDE 启动器**（成本极低）：
- `ide_installed("qoder")` = `macos_app_exists("Qoder IDE") || macos_app_exists("Qoder") || command_on_path("qodercli")`
- 打开优先级：`Qoder IDE`（VS Code 分支，适合开项目）→ 兜底 `Qoder`（桌面端）
- 本机 `qodercli` 未独立安装，`command_on_path` 仅为将来兼容

**可选增强（不阻塞主接入）**：
1. 桌面端标题 join：从 `com.qoder.app.stable/main.sqlite` 读 `chat_sessions.title`（只读 + busy_timeout），
   标题质量明显优于首句截断；
2. 图片归档：用 `crate::media_archive::archive_image_file(p, "qoder", session_id)` 复制本地副本，
   与 `cursor.rs::extract_cursor_bubble_images` 同构；
3. 文件变更 artifacts：`turn_file_change_sets/_files/_patches`（需先有样本验证）。

**明确不做**：
- Qoder IDE `chatSessions/*.json`（旧格式，最新 2025-09，与 CLI 会话不重合）
- `<sessionId>/compression-v2/` 状态（加密字段）、`~/.qoder/logs/*`（日志）
- 桌面端 `chat_session_messages` 作正文源（turn 级投影，只有最终文本）
- 任何云端 API（与"数据不出本机"定位冲突）

---

## 七、改动清单

### 后端 Rust

| # | 文件 | 改动 |
|---|---|---|
| 1 | `src-tauri/src/importers/qoder.rs` | **新建**，实现 `pub fn sync(conn, incremental) -> ImporterStats` |
| 2 | `src-tauri/src/importers/mod.rs:9-17` | 加 `pub mod qoder;` |
| 3 | `src-tauri/src/importers/mod.rs:642-652` | 调度数组加 `("Qoder", qoder::sync)`，长度 `9 → 10` |
| 4 | `src-tauri/src/sync.rs:66-178` | `collect_agent_sources` 登记 `("qoder", "Qoder", [make_info(".qoder/projects", "会话历史 (JSONL)"), make_info(".qoder/tmp", "会话图片缓存")])` |
| 5 | `src-tauri/src/sync.rs:354-355` | 测试断言 `sources.len()` `9 → 10` |
| 6 | `src-tauri/src/db.rs:39` | `WorkspaceStat` 加 `pub qoder_cnt: i64` |
| 7 | `src-tauri/src/db.rs:346` | `DashboardStats` 加 `pub qoder_conversation_count: i64` |
| 8 | `src-tauri/src/db.rs:953-966` | `source_to_label_and_color` 加 `"qoder" => ("Qoder", "#2ADB5C")`（色值取自官方 App 图标实测） |
| 9 | `src-tauri/src/db.rs:1099 / 1141 / 1416 / 1973 / 2302 / 2434 / 3528` | 各 `*_cnt` / `agent_breakdown` CASE 归类加 `WHEN source_types LIKE '%qoder%' THEN 'qoder'` |
| 10 | `src-tauri/src/db.rs:1700 (+1726)`、`2522` | `SUM(CASE …)` 聚合与行映射加 `qoder_cnt` |
| 11 | `src-tauri/src/db.rs:2495 / 2576-2577 / 2787` | 字段列表 / breakdown 文案 / detail 返回补 `qoder_cnt` |
| 12 | `src-tauri/src/lib.rs:1415-1434` | `ide_installed` 加 `qoder` 分支（`Qoder IDE.app` / `Qoder.app` / `qodercli`） |
| 13 | `src-tauri/src/lib.rs:1436-1475` | `list_ide_apps_cmd` 增加一项（label `Qoder`，kind `app`） |
| 14 | `src-tauri/src/lib.rs:1507-1536` | `open_workspace_in_ide_cmd` 加 `"qoder"`：优先 `open_in_macos_app("Qoder IDE", …)`，兜底 `"Qoder"` |
| 15 | `src-tauri/src/http_server.rs:561 / 968 / 1243` | REST 侧 `qoder_cnt` 字段与 `source_types LIKE` 映射同步 |

### 前端 TS/TSX

| # | 文件 | 改动 |
|---|---|---|
| 16 | `src/types/index.ts:12`、`:310` | `WorkspaceStat` / `DashboardStats` 加 `qoder_cnt` 字段 |
| 17 | `src/api/tauriBridge.ts:584` | Web 模式 fallback 列表加 `{ id:'qoder', label:'Qoder', kind:'app', installed:false }` |
| 18 | `src/components/browse/OpenInIdeMenu.tsx:8-15`、`:17-24` | `IDE_HINT_KEYS.qoder` + `DEFAULT_IDES` 一项 |
| 19 | `src/components/browse/ideIcons.tsx` | 加 `case 'qoder'`：Simple Icons **无 qoder**；建议取官方字标 SVG（`/Applications/Qoder IDE.app/Contents/Resources/app/out/media/logo.svg` 单 path，末段即字母 Q，viewBox 约 `0 0 27.79 31.63`）或先用 lucide 占位；品牌绿 `#2ADB5C` |
| 20 | `src/i18n/zh.ts:388` + `src/i18n/en.ts:390` | 加 `ide.hint.qoder`（"用 Qoder IDE 打开此项目"） |
| 21 | `src/components/browse/BrowseView.tsx:610`、`:829-831` | source badge 颜色（建议 `#2ADB5C`）与计数渲染 |
| 22 | `src/components/browse/WorkspaceAnalysisView.tsx:182` | source badge（如需） |

### 文档

| # | 文件 | 改动 |
|---|---|---|
| 23 | `README.md:14 / 31 / 78-81`、`README_CN.md` 同 | 支持表加 Qoder 第 10 行（当前表已含 9 个 agent，无需补漏） |

### 可选增强（不阻塞主接入）

| # | 改动 |
|---|---|
| 24 | 桌面端标题 join（只读 `com.qoder.app.stable/main.sqlite`） |
| 25 | 图片归档（`media_archive::archive_image_file`）+ `turn_file_change_*` → artifacts |

---

## 八、工作量与风险

### 工作量估算

| 范围 | 估算 |
|---|---|
| 完整 transcript importer（分块合并 + tool_result 关联 + credit 落库 + 增量） | ≈ 1.5 人日 |
| IDE 启动器 | ≈ 0.25 人日 |
| 全套注册（sync / db / http_server / 前端 / i18n / README） | ≈ 0.5 人日 |
| 可选：标题 join + 图片归档 | ≈ 0.5 人日 |
| **合计** | **≈ 2 ~ 2.5 人日** |

（比 CodeBuddy 的 3 ~ 3.5 人日少，少在 md5 反查与多布局探测。）

### 风险清单

| # | 风险 | 影响 | 缓解 |
|---|---|---|---|
| 1 | **格式随版本变动**（三条产品线：IDE 1.30.1 / 桌面 0.3.3 / CLI 1.1.53） | 升级后解析失效 | 全字段容错 + `QODER_PARSER_REV` 版本号强制重扫（对齐 `codebuddy.rs`） |
| 2 | **本机历史极短**（2 会话，今日首用） | 长尾字段（子代理、文件变更、压缩后行为）缺样本 | 先接入、灰度观察；风险 3~5 与此相关 |
| 3 | **token 恒 0，credits 非 0** | 无法做 token 统计 | 只落 `credit`；UI 已支持（与 CodeBuddy 同路径） |
| 4 | **dash 目录名不可逆**（`-` 与 `/` 歧义，本机 `qorder-xflow` 即实例） | 误判工作区 | **不解析目录名**，一律用文件内 `workspace-directories`/`cwd` |
| 5 | `isSidechain` 子代理链未实测 | 可能导入重复/内部对话 | v1 默认跳过 `isSidechain:true`，观察后放开 |
| 6 | JSONL 正在写入（末行半截） | 单文件解析失败 | 逐行 try/catch，失败行跳过不中断；下轮同步补齐 |
| 7 | 签名图片 URL 过期（OSS，约 30 天） | 历史图片失效 | 用 `~/.qoder/tmp/.../images/` 本地副本；归档到 `media_archive` |
| 8 | 桌面端 sqlite WAL 被占用 | 读锁失败 | 只读 + `busy_timeout`；失败仅跳过标题 join |
| 9 | `memory/*.md` 是 agent 记忆 | 误当会话导入 | 只扫 `*.jsonl`，天然规避 |

---

## 九、附：排查踩坑记录（避免重复劳动）

| 路径 | 实际内容 |
|---|---|
| `~/.qoder/projects/<proj>/<sessionId>/compression-v2/` | 压缩状态，字段加密（c/u/n/p/t）不可读 |
| `~/.qoder/tasks/<sessionId>/` | 实测空目录 |
| `~/.qoder/logs/runs/*/qodercli.log` | CLI 运行日志（含 sessionId、请求流水），非正文 |
| `~/.qoder/memory/`、`projects/*/memory/*.md` | Agent 记忆（如 90 个 md），非会话 |
| `~/Library/Application Support/Qoder/User/workspaceStorage/*/chatSessions/*.json` | VS Code chat-session v3 旧格式（本机最新 2025-09-10），与 CLI 会话不重合，价值低 |
| IDE `state.vscdb` 的 `lingma.chat.localHistory.*` | 仅标题索引，无正文 |
| 桌面端 `chat_session_messages` | turn 级投影（每轮 user + assistant 最终文本），**不是**完整消息流 |
| 桌面端 `qoder_daemon_session_store_entries` | 本机 0 行 |
| `~/.qoder/entry/qoder` 的 CLI 候选路径 | `~/.local/bin/qodercli`、`~/.qoder/bin/qodercli/qodercli` 本机均不存在（CLI 由桌面端内嵌，`bin/` 下只有 `qoder-computer-use`） |
| 单 path 的官方字标 logo（`logo.svg`） | 是 "Qoder" 单词字形（r/e/d/o/Q 五段子路径拼接），非单字图标 |

> 与 CodeBuddy 不同，Qoder 的 `~/.qoder/projects` **不需要任何反查**：
> 本机 14 个 dash 编码目录中 13 个是 memory-only，只要"有 .jsonl 才处理"即可。

---

## 十、一句话总结

> AgentDeck 目前**未接入 Qoder**。Qoder 的完整会话流**已在本地**
> （`~/.qoder/projects/<dash(cwd)>/<sessionId>.jsonl`，Claude Code 兼容 JSONL，
> 含 thinking / tool_use / tool_result / 模型名 / **credits 积分**，工作区路径文件内显式声明），
> 接入方式与 `claude.rs` 对等、且无需 md5 反查与扩表，主要工作量在 **assistant 分块流合并**，
> 预估 **2 ~ 2.5 人日**可完成主接入 + IDE 启动器 + 全套注册；
> 当前唯一的不确定性是本机历史极短（2 会话），建议接入后灰度观察格式稳定性。
