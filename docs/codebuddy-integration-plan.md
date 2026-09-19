# CodeBuddy 接入 AgentDeck —— 现状核查与接入方案评估

> 评估日期：2026-09-19（**v2 修正版**）
> 评估对象：AgentDeck（`/Users/xiyangxie/workspace/personal/AgentDeck`）

---

## 〇、v1 结论修正声明

v1 版本曾判断「CodeBuddy 对话正文不落盘本地，只在云端」——**该结论错误**。

原因是排查范围只覆盖了 `~/Library/Application Support/CodeBuddy CN/` 与 `~/.codebuddy/`，
**遗漏了扩展的独立数据目录 `~/Library/Application Support/CodeBuddyExtension/`**。

实测结论：**CodeBuddy 在本地保存了完整的会话正文、工具调用、token 用量、TODO 计划与文件变更树**，
数据完整度**不亚于 Claude Code / Codex**，可以做对等接入。

---

## 一、结论速览

| 问题 | 结论 |
|---|---|
| 现在接入 CodeBuddy 了吗？ | **没有。** 全仓对 `codebuddy` 大小写不敏感搜索 **0 命中** |
| 当前接入了哪些 agent？ | 共 **8 个**：`cursor`、`antigravity`、`claude`、`codex`、`hermes`、`workbuddy`、`mimo`、`windsurf` |
| CodeBuddy 本地有会话记录吗？ | **有，且很完整。** 位于 `~/Library/Application Support/CodeBuddyExtension/Data/` |
| 能否做到与 Claude Code 对等？ | **能。** 完整消息流 + role + tool_call/tool_result + reasoning + 模型名 + token/credit |
| 主要技术难点 | **工作区路径映射**：目录名是 `md5(cwd)`，不可逆，需三重策略反查 |
| 推荐路线 | 直接做**完整 transcript importer**（不需要 v1 提的"元数据降级方案"） |

> 注意：仓库中已有一个形近的 `workbuddy`（WorkBuddy），与 CodeBuddy 无关。

---

## 二、真正的数据源

### 2.1 目录全景

```
~/Library/Application Support/CodeBuddyExtension/
├── Cache/CodeBuddyIDE/{CodeBuddy, CodeBuddy CN}/{mcp, MarketReadme}
├── Logs/CodeBuddyIDE/<YYYY-MM-DD>/
└── Data/
    ├── Public/
    │   ├── .memories/.memory-global-config.json
    │   ├── auth/workbuddy-desktop.info
    │   └── internal-rules/*.mdc
    ├── default/CodeBuddyIDE/                     ← 未登录域
    │   ├── history/<convId>/{index.json, .index_bak.json}
    │   ├── plan-task/<convId>/
    │   └── <base64(cwd)>/{history, plan-task, check-point, file-tree}/<convId>/
    ├── 081d77ce-7bb9-4439-ada5-3379048ec3be/     ← 账号域（本机主力）
    │   └── CodeBuddyIDE/
    │       ├── <userId>/                          ← 账号级（跨工作区）
    │       │   ├── history/<md5(cwd)>/<convId>/
    │       │   │   ├── index.json                 ★ 会话索引
    │       │   │   ├── .index_bak.json
    │       │   │   └── messages/<msgId>.json      ★ 逐条消息正文
    │       │   ├── plan-task/<md5(cwd)>/<convId>/meta.json     ★ TODO
    │       │   ├── file-tree/<md5(cwd)>/<convId>/file-tree.json ★ 变更文件树
    │       │   └── check-point/<convId>/
    │       ├── <base64(cwd)>/history/<convId>/    ← 工作区级（布局少一层）
    │       └── genie-cache/{Ada, xflow, dawnote, Claw, <ts>}
    └── bbf7e1ba-12e6-478a-9c8c-5031b16c7158/CodeBuddyIDE/<userId>/{history, plan-task}
```

> **关键：`history/` 有三种布局深度**，importer 必须递归探测而非写死层级：
> 1. `CodeBuddyIDE/<userId>/history/<md5(cwd)>/<convId>/`
> 2. `CodeBuddyIDE/<base64(cwd)>/history/<convId>/`
> 3. `CodeBuddyIDE/history/<convId>/`（default 域）

### 2.2 本机实测数据量

| 指标 | 数值 |
|---|---|
| 账号域数量 | 3（`default` / `081d77ce…` / `bbf7e1ba…`） |
| 会话目录数（主力域） | **26** |
| 消息文件总数 | **1093**（assistant 546 / tool 484 / user 63） |
| 有实质内容的会话 | 6 个（420 / 422 / 179 / 50 / 20 / 4 条），其余 20 个为空壳（`index.json` 仅 38 B） |
| 最大 `index.json` | 85,696 B |
| 最大单条 `messages/*.json` | 93,380 B |

---

## 三、Schema（全部实测）

### 3.1 `index.json` —— 会话索引

```jsonc
{
  "messages": [                                  // 顺序即 step_index
    { "id": "124ca3dd…", "type": "text", "role": "user",      "isComplete": true  },
    { "id": "e1958cd6…", "type": "text", "role": "assistant", "isComplete": false },
    { "id": "71f1d5c2…", "type": "text", "role": "tool",      "isComplete": true  }
  ],
  "requests": [                                  // 一次用户提问 = 一个 request
    {
      "id": "ed3216f4…",
      "type": "craft",
      "messages": ["124ca3dd…", "e1958cd6…", …], // 归属该轮的消息 id
      "state": "complete",
      "startedAt": 1784117882954,                // epoch ms
      "usage": {
        "inputTokens": 420035, "outputTokens": 3081, "totalTokens": 423116,
        "lastTokens": 55163,   "cacheTokens": 372614,
        "cachedWriteTokens": 0, "cachedMissTokens": 47421,
        "credit": 17.01                          // ★ 积分消耗
      }
    }
  ]
}
```

### 3.2 `messages/<msgId>.json` —— 单条消息

```jsonc
{
  "role": "user" | "assistant" | "tool",
  "message": "<JSON 字符串>",        // ★ 需二次 parse，内容为 Vercel AI SDK UIMessage
  "id": "<32hex>",
  "references": [ … ],               // 仅 user：附加的 rules / 上下文引用
  "extra": "<JSON 字符串>",          // ★ 需二次 parse
  "createdAt": "2026-09-19T10:41:02.901Z"
}
```

`extra` 解析后：
```json
{ "requestId": "74ab5f2a…", "modelId": "kimi-k2.7",
  "modelName": "Kimi-K2.7-Code", "isHelperMessage": false,
  "traceId": "3e235e78…", "responseId": "0006c66d…" }
```

`message` 解析后按 role 分三种形态：

| role | content 结构 |
|---|---|
| `user` | `[{ "type":"text", "text":"<additional_data>…</additional_data>\n\n<user_query>真实提问</user_query>" }]` |
| `assistant` | `[{ "type":"reasoning", "text":"…" }, { "type":"tool-call", "toolCallId":"list_dir_20", "toolName":"list_dir", "args":{…} }]` + `providerOptions.openaiCompatible.reasoning` |
| `tool` | `[{ "type":"tool-result", "toolCallId":"replace_in_file_67", "toolName":"replace_in_file", "result":{ "status":"success", "result":{ "type":"replace_in_file_result", "path":"…", "addLineCount":1, "removedLines":1, "addedChars":94, "removedChars":86 }, "errorCode":"0" }, "isError":false }]` |

> ⚠️ **解析要点**：user 正文被 `<additional_data>` / `<user_query>` 标签包裹，
> 必须抽取 `<user_query>` 内的内容，否则会话标题、消息统计、关键词检索全部失真。

### 3.3 `plan-task/<md5(cwd)>/<convId>/meta.json` —— TODO

```jsonc
{ "title": "Todo List",
  "items": [ { "id":"1", "title":"…", "description":"…",
               "status":"completed", "createdAt":"…", "updatedAt":"…" } ] }
```

### 3.4 `file-tree/<md5(cwd)>/<convId>/file-tree.json` —— 变更文件树

```jsonc
[ { "name":"AiOpenApiAgentQueryService.java",
    "filePath":"/Users/xiyangxie/workspace/securio/xflow/…/AiOpenApiAgentQueryService.java",
    "type":"file",
    "versions":[ { "diff": { "addedLines":1, "removedLines":22,
                             "addedChars":57, "removedChars":1080 } } ] } ]
```

### 3.5 辅助源（可选增强）

| 路径 | 内容 |
|---|---|
| `~/Library/Application Support/CodeBuddy CN/codebuddy-sessions.vscdb` | `session:<convId>` → `{title, cwd, status, createdAt, updatedAt}`。**仅保留最近 6 条**，用作标题/cwd 的快速 join |
| `…/tencent-cloud.coding-copilot/todos/<convId>.json` | 运行时 TODO 快照 |
| `…/tencent-cloud.coding-copilot/edit-sessions/<convId>.json` | `fileDiffs`（含 oldContent） |
| `…/tencent-cloud.coding-copilot/file-changes/<convId>/*.json` | 文件变更快照 |
| `…/tencent-cloud.coding-copilot/genie-history/<base64(cwd)>/current.json` | ★ **明文 base64 工作区路径**，是 md5 反查的关键 |

---

## 四、核心难点：工作区路径映射

`history/` 下的一级目录名是 **`md5(cwd)`**，已实测验证 **5/5 全部命中**：

| cwd | md5(cwd) = 目录名 |
|---|---|
| `/Users/xiyangxie/workspace/wiwj/Ada` | `03c19949eb503714a8348640aef64830` ✅ |
| `/Users/xiyangxie/workspace/chuhai/dawnote` | `a855f44bc4b1029323b496cae8174a07` ✅ |
| `/Users/xiyangxie/workspace/personal/AgentDeck` | `badc582712cac142fd12606bb86001d4` ✅ |
| `/Users/xiyangxie/workspace/securio/xflow` | `c9aea32e06a255cd6fd4b139a05d8ef7` ✅ |
| `/Users/xiyangxie/CodeBuddy/20260919103956` | `f054b094cbab8b29220cfbd533abbfe4` ✅ |

MD5 不可逆，需**三重反查策略**（按优先级）：

1. **base64 目录名（主路径，覆盖历史全部工作区）**
   枚举 `CodeBuddyIDE/<base64>/` 与 `genie-history/<base64>/` 的目录名 → base64 解码得明文 cwd → 计算 md5 → 建立 `md5 → cwd` 映射表。
   实测 `L1VzZXJzL3hpeWFuZ3hpZS93b3Jrc3BhY2UvcGVyc29uYWwvQWdlbnREZWNr` 解码即 `/Users/xiyangxie/workspace/personal/AgentDeck`。
2. **`codebuddy-sessions.vscdb` join（补最近会话）**
   `session:<convId>.cwd` 直接给出路径。缺点：只保留最近 6 条。
3. **消息内绝对路径兜底**
   从 `file-tree.json` 的 `filePath`、或 tool-call `args.filePath` 取最长公共前缀反推项目根，再回算 md5 校验一致性。

---

## 五、字段映射（→ `importers/mod.rs` 的 Raw* 结构）

### 5.1 `RawConversation`

| 字段 | CodeBuddy 来源 |
|---|---|
| `id` | `<convId>`（32hex） |
| `title` | vscdb `title`；缺失时取首条 user 的 `<user_query>` 截断 |
| `workspace_path` | `project_root_from_path(cwd)`，cwd 由第四章三重策略反查 |
| `source_app` | `"CodeBuddy"` |
| `created_at` / `updated_at` | 首 / 末条 message 的 `createdAt`；或 vscdb |
| `parse_status` | `"ok"`；空壳会话（`index.json` = 38 B）跳过 |
| `source_types` | 收集 assistant `tool-call` 的 `toolName` 去重 |
| `messages` | 按 `index.json.messages` 顺序逐个读 `messages/<id>.json` |
| `artifacts` | `file-tree.json` 的 `filePath` + `versions[].diff` 汇总 |

### 5.2 `RawMessage`

| 字段 | 来源 | 备注 |
|---|---|---|
| `step_index` | `index.json.messages` 数组下标 | |
| `role` | `role` | user / assistant / tool |
| `message_type` | `content[].type` 映射 | `text`→text、`tool-call`→tool_call、`tool-result`→tool_result、`reasoning`→thinking |
| `content` | `content[].text`；tool 则取 `result` 摘要 | user 需剥 `<user_query>` |
| `thinking` | `content[].type=="reasoning"` 的 `text` | |
| `created_at` | `createdAt` | |
| `tool_name` | `content[].toolName` | |
| `tool_args` | `JSON.stringify(content[].args)` | |
| `model_name` | `extra.modelName` | ⚠️ 见下方限制 |
| `token_count` | `requests[].usage` 分摊 | ⚠️ 见下方限制 |
| `images` | `content[].type=="image"`（待确认） | |

> ⚠️ **已确认的落库限制**：`messages` 表（`db.rs:506-521`）只有
> `step_index / role / message_type / content / thinking / tool_name / tool_args / created_at / source / is_truncated / images`，
> **没有 `model_name` / `token_count` / `duration_ms` 列**。
> `save_conversation_tx`（`importers/mod.rs:357-389`）也不写这三项。
> 因此 CodeBuddy 的 `modelName` 与 `usage.credit` 目前**无处落库**。
> → 若要利用（对成本分析很有价值），需扩表，属**独立的增强项**，不阻塞主接入。

---

## 六、推荐方案

**直接做完整 transcript importer，与 `claude.rs` 对等。** v1 提出的「元数据降级方案」已无必要。

`src-tauri/src/importers/codebuddy.rs` 处理流程：

```
1. 枚举 Data/ 下所有账号域（default + <userId>），跳过 Public
2. 递归探测每个域下的 history/ 目录，兼容三种布局深度
3. 建立 md5 → cwd 映射表（base64 目录名 + vscdb + 路径兜底）
4. 对每个 <convId>：
   a. 读 index.json，取 messages 顺序 + requests(usage)
   b. 空壳（messages 为空）→ 跳过
   c. 逐条读 messages/<id>.json，二次 parse message / extra
   d. 按 role + content[].type 展开为 RawMessage（一条 AI 消息可产出多条：reasoning / text / tool_call）
   e. user 消息剥 <user_query>
   f. 读 plan-task/meta.json、file-tree/file-tree.json → artifacts
   g. needs_sync 增量判断 → save_conversation_tx
```

**同步做 IDE 启动器**（成本极低，约 0.5 人日）：
`ide_installed("codebuddy")` → 检测 `/Applications/CodeBuddy CN.app` 与 `CodeBuddy.app`；
`open -a "CodeBuddy CN" <workspace>`。

**明确不做**：直连 `copilot.tencent.com` 云端 API（接口非公开、需读用户凭证、与「数据不出本机」定位冲突）。本地数据已足够完整，无此必要。

---

## 七、改动清单

### 后端 Rust

| # | 文件 | 改动 |
|---|---|---|
| 1 | `src-tauri/src/importers/codebuddy.rs` | **新建**，实现 `pub fn sync(conn, incremental) -> ImporterStats` |
| 2 | `src-tauri/src/importers/mod.rs:9-16` | 加 `pub mod codebuddy;` |
| 3 | `src-tauri/src/importers/mod.rs:619-628` | 调度数组加 `("CodeBuddy", codebuddy::sync)`，长度 `8 → 9` |
| 4 | `src-tauri/src/sync.rs:66-144` | `collect_agent_sources` 登记 `("codebuddy", "CodeBuddy", [...])`，扫描路径含 `CodeBuddyExtension/Data`、`codebuddy-sessions.vscdb`、`tencent-cloud.coding-copilot/` |
| 5 | `src-tauri/src/sync.rs:336` | 测试断言 `sources.len()` `8 → 9` |
| 6 | `src-tauri/src/db.rs:940-952` | `source_to_label_and_color` 加 `"codebuddy" => ("CodeBuddy", "#0052d9")` |
| 7 | `src-tauri/src/db.rs:1076 / 1119 / 1393 / 1660` | 各 `*_cnt` / `agent_breakdown` SQL 归类同步 |
| 8 | `src-tauri/src/http_server.rs:958-968, 1232-1242` | REST 侧 `source_types LIKE` 映射同步 |
| 9 | `src-tauri/src/lib.rs:1376-1388` | `ide_installed` 加 `codebuddy` 分支 |
| 10 | `src-tauri/src/lib.rs:1392-1424` | `list_ide_apps_cmd` 增加一项 |
| 11 | `src-tauri/src/lib.rs:1465-1480` | `open_workspace_in_ide_cmd` 加 `"codebuddy" => open_in_macos_app("CodeBuddy CN", …)` |

### 前端 TS/TSX

| # | 文件 | 改动 |
|---|---|---|
| 12 | `src/components/browse/ideIcons.tsx:38-87` | 加 `case 'codebuddy'` 图标（`IdeAppStatus` 结构通用，`types/index.ts` 无需改） |
| 13 | `src/components/browse/OpenInIdeMenu.tsx:8-14, 16-22` | 加 `IDE_HINT_KEYS.codebuddy` + `DEFAULT_IDES` 一项 |
| 14 | `src/api/tauriBridge.ts:574-585` | Web 模式 fallback 列表加一项 |
| 15 | `src/i18n/zh.ts:313-324` + `src/i18n/en.ts` | 加 `ide.hint.codebuddy` |
| 16 | `src/components/browse/WorkspaceAnalysisView.tsx:152-181` | source badge（如需） |

### 文档

| # | 文件 | 改动 |
|---|---|---|
| 17 | `README.md:69-80`、`README_CN.md:69-80` | 支持表补 CodeBuddy |

> 顺带发现：README 支持表**当前漏了 MiMo 与 Windsurf**（表格只有 6 行，实际已接入 8 个），建议一并补齐。

### 可选增强（不阻塞主接入）

| # | 改动 |
|---|---|
| 18 | `db.rs` `messages` 表加 `model_name` / `token_count` 列 + `save_conversation_tx` 写入，以承接 `extra.modelName` 与 `usage.credit` |
| 19 | `conversations` 表加 `credit` / `total_tokens` 聚合列，用于成本分析视图 |

---

## 八、工作量与风险

### 工作量估算

| 范围 | 估算 |
|---|---|
| 完整 transcript importer（含 md5 反查、三种布局、UIMessage 解析） | ≈ 2 ~ 2.5 人日 |
| IDE 启动器 | ≈ 0.5 人日 |
| 全套注册（sync / db / http_server / 前端 / i18n / README） | ≈ 0.5 人日 |
| 可选：token/credit 扩表 + 成本视图 | ≈ 1 人日 |

### 风险清单

| # | 风险 | 影响 | 缓解 |
|---|---|---|---|
| 1 | **私有格式，随版本变动**（当前 CodeBuddy CN v4.12.0） | 升级后解析失效 | 全字段容错 + 解析器版本号强制重扫（对齐 `cursor.rs::CURSOR_PARSER_REV`） |
| 2 | **`history/` 三种布局深度** | 漏扫会话 | 递归探测目录名匹配 32hex，不写死层级 |
| 3 | **`md5(cwd)` 不可逆** | 无法归属工作区 | 第四章三重反查；全部失败时 `workspace_path` 置空并标 `parse_status` |
| 4 | **20/26 会话是空壳**（`index.json` = 38 B） | 产生大量空记录 | `messages` 为空直接跳过 |
| 5 | **user 正文被 `<additional_data>`/`<user_query>` 包裹** | 标题/统计/检索失真 | 正则抽取 `<user_query>`，无标签时回退全文 |
| 6 | **一条 assistant 消息含多个 content 块**（reasoning + 多个 tool-call） | 消息数与 step_index 错位 | 按 content 块展开为多条 RawMessage，step_index 用 `i.sub` 形式或连续递增 |
| 7 | `.index_bak.json` 与 `index.json` 可能不一致 | 读到旧数据 | 只以 `index.json` 为准 |
| 8 | IDE 运行时正在写文件 | JSON 半写入解析失败 | 单文件 try/catch 跳过，不中断整轮同步 |
| 9 | 多账号域（`default` / 多个 userId） | 重复导入同一 convId | 以 `convId` 为主键去重，`ON CONFLICT` 已覆盖 |
| 10 | `file-tree` / `plan-task` 含源码路径与内容 | 隐私 | 与现有 edit 快照同类，沿用本地 `~/.agentdeck/` 策略，不上传 |

---

## 九、附：排查踩坑记录（避免重复劳动）

以下是**已确认的死路**，不含会话正文：

| 路径 | 实际内容 |
|---|---|
| `~/Library/Application Support/CodeBuddy CN/codebuddy-sessions.vscdb` | 仅最近 6 条会话**元数据**（title/cwd/时间），无正文 |
| `…/tencent-cloud.coding-copilot/message-queue/*.json` | 运行时队列，实测 `items` 恒为空 |
| `…/tencent-cloud.coding-copilot/genie-history/<b64>/conversations/<convId>/` | 实测空目录 |
| `~/.codebuddy/` | IDE user-data（`extensions/`、`argv.json`、`settings.json`、`models.json`、`skills/`、`plugins/`）+ `memery/<userId>_memery.md` 用户记忆画像，**非会话** |
| `…/CodeBuddy CN/User/History/` | VS Code 本地文件历史（编辑器 undo 快照），非会话 |
| `state.vscdb` 的 `Tencent-Cloud.coding-copilot` 条目（118 KB） | UI 状态 / 配置缓存，非正文 |
| `~/.codebuddy/logs/memwatch/*.json` | 内存监控日志 |
| `…/CodeBuddy CN/logs/**/腾讯云代码助手.log` | 扩展运行日志，仅含会话 id 与请求流水，非结构化正文 |

另：**本机不存在独立的 CodeBuddy Code CLI**（`which codebuddy` 为空、npm 全局无该包、`~/.codebuddy/bin` 只有指向 IDE 启动器的 `buddy`/`buddycn`），
故 v1 设想的「Phase 2 接 CLI 拿 transcript」已无必要 —— IDE 本地数据就是完整的。

---

## 十、一句话总结

> AgentDeck 目前**未接入 CodeBuddy**。CodeBuddy 的完整会话正文**确实存在本地**
> （`~/Library/Application Support/CodeBuddyExtension/Data/<域>/CodeBuddyIDE/**/history/<md5(cwd)>/<convId>/`，
> 本机实测 26 会话 / 1093 条消息，含 role、tool-call、reasoning、模型名、token 与 credit）。
> 接入方式与 Claude Code 对等，主要工作量在**三种目录布局兼容**与 **md5→cwd 工作区反查**，
> 预估 **3 ~ 3.5 人日**可完成主接入 + IDE 启动器 + 全套注册。
