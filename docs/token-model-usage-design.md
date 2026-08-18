# AgentDeck Token + Model Usage 统一设计

> 目标：在不大改现有「会话驾驶舱」定位的前提下，引入可演进、可降级的 usage 数据层，
> 支撑 **Token 统计** 与 **模型使用统计**，并为后续 Cost / Cache 分析预留空间。
>
> 参考：`agentsview` 的 `messages.token_usage`、`usage_events`、`cursor_usage_events`、
> `internal/db/usage.go` 聚合与 `docs/token-usage.md` 产品口径。

---

## 1. 现状与问题

### 1.1 已有能力

| 层 | 现状 |
| --- | --- |
| Importer `RawMessage` | 已有 `model_name`、`token_count` 字段 |
| DB `messages` | 有 `source` 列，**未**持久化 model/token；`fetch_conversation_messages` 把 `source` 当作 `model_name` |
| Dashboard | 消息数 / 会话数 / 用户消息数 / 按小时/按日活跃；**无** token、**无** 模型分布 |
| 各 importer | 多数 `model_name` 填的是平台名（`Cursor`、`Claude`），`token_count` 几乎全是 `None` |

### 1.2 核心问题

1. **解析与存储断层**：Importer 结构里有字段，入库时被丢弃。
2. **口径不统一**：`model_name` 有时是平台、有时是模型，无法做模型排行。
3. **无 usage 原始形态**：只有单一 `token_count`，无法扩展 input/output/cache/reasoning。
4. **无「无数据」语义**：若直接 `SUM(token)`，会把缺失当成 0，图表会误导。

### 1.3 设计原则（对齐 agentsview）

1. **原始 usage 尽量保真**（JSON blob + 结构化列）。
2. **多粒度兼容**：message-level 优先；session-level 用 `usage_events` 补位。
3. **解析与定价/聚合分离**：Importer 只负责抽取；统计在 `db` 层统一。
4. **显式 coverage**：区分「0 token」与「无 usage 数据」。
5. **不伪造**：拿不到就 `NULL`，不强行分摊到 message。

---

## 2. 目标与非目标

### 2.1 第一期目标（MVP）

- 持久化 **模型名** + **total tokens**（及可得的 input/output）。
- Dashboard 展示：
  - 总 Token（带 coverage 说明）
  - 近 30 天 Token 趋势（可复用现有柱状图 tab）
  - Top Models（消息数、会话数、Token）
  - 按 Agent 平台的模型/token 分布
  - Tooltip：消息 / 用户 / 会话 / **Token**
- 会话详情：展示真实 `model_name` 与 message token（若有）。

### 2.2 第二期（进阶）

- Cache read/write、reasoning tokens
- Cost（`model_pricing` + `cost_micros`）
- Cache hit rate、Top sessions by token/cost
- Pairwise 模型对比
- Cursor Admin billing events（若需要与本地 transcript 对齐）

### 2.3 非目标（第一期不做）

- 全平台 100% token coverage
- 实时 pricing 同步
- 独立 `/usage` 全页（可先嵌在大盘 + 工作区分析）

---

## 3. 数据模型

### 3.1 统一 Rust 结构（Importer 输出）

```rust
/// 单条 usage 记录；Importer 填充，允许部分字段缺失。
pub struct RawUsage {
    pub model_name: Option<String>,      // 真实模型 ID，如 claude-sonnet-4、gpt-4o
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_creation_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    pub total_tokens: Option<i64>,       // 上游有 total 时填；否则由 normalize 推导
    pub cost_micros: Option<i64>,        // 上游直接给 cost 时填（Copilot shutdown 等）
    pub raw_json: Option<String>,        // 原始 usage JSON，保真
    pub granularity: UsageGranularity,   // message | conversation | billing_event
    pub source: UsageSource,             // transcript | session_aggregate | admin_api | estimated
}

pub enum UsageGranularity {
    Message,
    Conversation,
    BillingEvent,
}

pub enum UsageSource {
    Transcript,
    SessionAggregate,
    AdminApi,
    Estimated,
}
```

`RawMessage` 扩展：

```rust
pub struct RawMessage {
    // ... 现有字段 ...
    pub usage: Option<RawUsage>,  // 替代单独的 model_name + token_count
}
```

**兼容策略**：过渡期保留 `model_name` / `token_count`，由 `normalize_usage()` 统一写入 `usage`。

### 3.2 `normalize_usage()` 规则

1. `total_tokens` = 显式 total，或 `input + output + cache_creation + cache_read`（仅当各分项均非 NULL）。
2. `model_name` 归一化：
   - trim、lower 比较用；展示保留原始大小写。
   - 空字符串 → `NULL`。
   - 平台占位名（`Cursor`、`Claude`）若会话内存在更具体模型，会话级统计以具体模型为准。
3. **不**把 session aggregate 摊分到每条 message（第一期）。
4. `has_usage` = 任一 token 字段非 NULL 或 `raw_json` 非空。

### 3.3 数据库 Schema（SQLite 迁移）

在现有 `messages` 上 **增量加列**（不删表、不 truncate）：

```sql
-- messages 扩展
ALTER TABLE messages ADD COLUMN model_name TEXT NOT NULL DEFAULT '';
ALTER TABLE messages ADD COLUMN input_tokens INTEGER;          -- NULL = 未知
ALTER TABLE messages ADD COLUMN output_tokens INTEGER;
ALTER TABLE messages ADD COLUMN cache_creation_tokens INTEGER;
ALTER TABLE messages ADD COLUMN cache_read_tokens INTEGER;
ALTER TABLE messages ADD COLUMN reasoning_tokens INTEGER;
ALTER TABLE messages ADD COLUMN total_tokens INTEGER;
ALTER TABLE messages ADD COLUMN usage_json TEXT NOT NULL DEFAULT '';
ALTER TABLE messages ADD COLUMN usage_source TEXT NOT NULL DEFAULT '';
ALTER TABLE messages ADD COLUMN has_usage INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_messages_created_usage
  ON messages(created_at) WHERE has_usage = 1;
CREATE INDEX IF NOT EXISTS idx_messages_model
  ON messages(model_name) WHERE model_name != '';
```

`source` 列：**保留**，语义改为「消息来源/子类型」（与 agentsview `source_type` 类似），**不再**冒充 `model_name`。

**会话级汇总**（agentsview `usage_events` 思路）：

```sql
CREATE TABLE IF NOT EXISTS usage_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    message_step_index INTEGER,           -- 可空：挂到某条 message
    source_app TEXT NOT NULL DEFAULT '',
    model_name TEXT NOT NULL DEFAULT '',
    input_tokens INTEGER,
    output_tokens INTEGER,
    cache_creation_tokens INTEGER,
    cache_read_tokens INTEGER,
    reasoning_tokens INTEGER,
    total_tokens INTEGER,
    cost_micros INTEGER,
    usage_json TEXT NOT NULL DEFAULT '',
    usage_source TEXT NOT NULL DEFAULT '',
    occurred_at TEXT,                       -- 用于按日聚合
    dedup_key TEXT,                         -- 防重
    UNIQUE(conversation_id, dedup_key)
);
```

**可选（第二期）**：`cursor_usage_events` — 仅当接入 Cursor Admin API 时再建。

**会话表轻量汇总**（加速 Top sessions / 列表）：

```sql
ALTER TABLE conversations ADD COLUMN primary_model TEXT NOT NULL DEFAULT '';
ALTER TABLE conversations ADD COLUMN models_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE conversations ADD COLUMN total_tokens INTEGER;       -- NULL = 未知
ALTER TABLE conversations ADD COLUMN has_usage INTEGER NOT NULL DEFAULT 0;
```

`primary_model`：该会话 token 最多的模型；`models_json`：`[{ "model": "...", "messages": N, "tokens": T }]`.

### 3.4 Coverage 元数据

`fetch_dashboard_stats` 返回 usage 覆盖率，供 UI 诚实展示：

```rust
pub struct UsageCoverage {
    pub messages_with_usage: i64,
    pub messages_total: i64,
    pub conversations_with_usage: i64,
    pub conversations_total: i64,
    pub coverage_percent: f64,           // messages_with_usage / assistant_messages 或全量
    pub by_source_app: Vec<SourceAppCoverage>,
}
```

---

## 4. Importer 改造

### 4.1 统一接口

```rust
pub trait UsageExtractor {
    /// 从单条上游记录解析 usage；返回 None 表示无数据（非错误）。
    fn extract_message_usage(&self, raw: &UpstreamMessage) -> Option<RawUsage>;
    /// 会话级 aggregate（可选）
    fn extract_session_usage(&self, raw_session: &UpstreamSession) -> Vec<RawUsage>;
}
```

各 importer 实现 `extract_message_usage`，`save_conversation_tx` 统一落库。

### 4.2 入库变更（`save_conversation_tx`）

INSERT `messages` 增加列；若 `usage_events` 有 session-level 行，在消息写入后单独 INSERT。

会话保存后调用 `recompute_conversation_usage_summary(conv_id)` 更新 `conversations.primary_model` 等。

### 4.3 各平台预期能力（第一期优先级）

| Importer | 模型名 | Message token | Session token | 备注 |
| --- | --- | --- | --- | --- |
| Claude | 高 | 高（JSONL usage） | 部分 | **P0** |
| Codex | 中 | 高 | 部分 | **P0** |
| Antigravity / Gemini | 中 | 中 | 低 | transcript 中 model + usage |
| Hermes | 低 | 待查 state.db | 可能 | **P1** |
| WorkBuddy | 低 | 待查 | 可能 | **P1** |
| Cursor | 中 | 低~中（bubble 元数据） | 无 | 常只有平台名；Admin API 第二期 |

**Parser revision**：与 Cursor 时间戳修复类似，用 `sync_state` 记 `agentdeck:usage_parser_rev`，版本 bump 时强制重导。

### 4.4 模型名归一化表（可选第二期）

`model_aliases`：把 `claude-3-5-sonnet-20241022` 与展示名映射；第一期可直接用原始 model id。

---

## 5. 聚合层（`db::usage` 模块）

新建 `src-tauri/src/usage.rs`（或 `db/usage.rs`），**所有 token/model 统计只从这里出**。

### 5.1 查询口径

- **时间**：与现有大盘一致，`datetime(created_at, '+8 hours')` 切北京自然日/小时。
- **Token 总量**（第一期）：
  - `SUM(COALESCE(m.total_tokens, m.input_tokens + m.output_tokens))` 仅 `has_usage = 1`。
  - Session-level：合并 `usage_events` 中 **未** 挂到 message 的行（避免双计）。
- **模型统计**：
  - 维度：`model_name` + `source_app` + `workspace_path`（通过 `conversations` JOIN）。
  - 指标：`message_count`、`conversation_count`（有该模型 assistant 消息的 DISTINCT conv）、`total_tokens`。
- **无数据**：`NoTokenData` 过滤器 — 选中仅无 usage 的 agent 时 UI 显示说明，不画空图。

### 5.2 API / Tauri 命令

第一期扩展 `DashboardStats`：

```rust
pub struct DashboardStats {
    // ... 现有字段 ...
    pub usage_coverage: UsageCoverage,
    pub total_tokens: Option<i64>,              // None 若 coverage=0
    pub last30_daily_tokens: Vec<DailyBarSlot>,
    pub last30_hourly_tokens: Vec<DayHourlyBars>, // 与 msgs 同结构
    pub model_breakdown: Vec<ModelUsageRow>,
    pub top_models: Vec<ModelUsageRow>,         // Top N
    pub agent_model_matrix: Vec<AgentModelRow>, // source_app × model
}

pub struct ModelUsageRow {
    pub model_name: String,
    pub source_app: String,
    pub message_count: i64,
    pub conversation_count: i64,
    pub total_tokens: Option<i64>,
    pub percent_messages: f64,
    pub percent_tokens: Option<f64>,
}
```

后续独立命令（第二期）：

- `get_usage_summary(filter)` — 对齐 agentsview `/api/v1/usage/summary`
- `get_session_usage(conversation_id)`

---

## 6. 前端（Dashboard）

### 6.1 第一期 UI 布局

1. **KPI 行** 增加「总 Token」卡片  
   - 副标题：`基于 xx% 消息有 usage 数据`

2. **按日 24h / 近 30 天** 柱状图  
   - Tab 增加：`按消息数 | 按会话数 | 按 Token`  
   - Tooltip 已有：消息 / 用户 / 会话 → 加 **Token**

3. **新增卡片「模型使用 Top」**（可与 Agent 分布并排）  
   - 列：模型、Agent、消息、会话、Token  
   - 支持按 Token / 消息数排序

4. **Agent 分布区** 增加「按模型」二级 breakdown（点击 Agent 看模型列表）

5. **工作区分析**（第二期）  
   - 该项目 Top Models + Token

### 6.2 组件复用

- `ActivityBarChart`：已支持多 metric tooltip，扩展 `tokenCount`。
- 模型分布：可先复用 Agent 饼图组件逻辑，或轻量横向 bar list。

---

## 7. 实施顺序

### Phase 0 — 地基（1~2 天）

1. Schema 迁移 + `ensure_messages_schema` 扩展
2. `RawUsage` + `normalize_usage()` + `save_conversation_tx` 落库
3. `fetch_conversation_messages` 读真实 `model_name` / tokens
4. `usage_coverage` 查询（哪怕全 NULL 也要正确）

### Phase 1 — Importer P0（2~4 天）

1. Claude + Codex 抽取 message usage
2. Antigravity 补 model + usage
3. `usage_parser_rev` 强制重导
4. `recompute_conversation_usage_summary`

### Phase 2 — Dashboard MVP（1~2 天）

1. `DashboardStats` 扩展字段
2. 24h / 30d Token tab + tooltip
3. Top Models 卡片
4. KPI 总 Token + coverage

### Phase 3 — 扩展（按需）

1. Hermes / WorkBuddy / Cursor bubble usage
2. `usage_events` session-level
3. Cost / cache / 独立 Usage 页

---

## 8. 风险与决策

| 风险 | 缓解 |
| --- | --- |
| 各源 usage 格式差异大 | `raw_json` + 分 importer 抽取；统一 normalize |
| Cursor 无精细 usage | coverage 标注；第二期 Admin API |
| 重导耗时长 | `usage_parser_rev` + 增量；与现有 content_hash 并存 |
| 双计 session + message usage | 聚合 SQL 明确：message 优先，session event 仅补 orphan |
| 模型名混乱 | 第一期原始 id；第二期 alias 表 |

**已拍板建议**：

- Message-level 优先，不摊分 session usage。
- 第一期不做 cost，但 schema 预留 `cost_micros`。
- 统计全部走 `usage` 模块，禁止 Dashboard 手写 SQL。

---

## 9. 验收标准（第一期）

1. Claude/Codex 会话详情可见非空 `model_name` 与 token（有上游数据时）。
2. 大盘「总 Token」与 coverage 正确；无数据时显示「暂无 token 数据」而非 0。
3. 近 30 天 / 24h 可按 Token 切换，tooltip 含 Token。
4. Top Models 列表与按 Agent 汇总一致可核对（SQL 可复算）。
5. 旧库迁移后原有消息/会话统计不变。

---

## 10. 关键文件（实施时）

| 文件 | 变更 |
| --- | --- |
| `src-tauri/src/importers/mod.rs` | `RawUsage`、`save_conversation_tx` |
| `src-tauri/src/importers/claude.rs` | usage 抽取 P0 |
| `src-tauri/src/importers/codex.rs` | usage 抽取 P0 |
| `src-tauri/src/db.rs` | schema、migration、`fetch_dashboard_stats` |
| `src-tauri/src/usage.rs` | 新建聚合 |
| `src/types/index.ts` | `DashboardStats` 扩展 |
| `src/components/dashboard/DashboardView.tsx` | Token tab、Top Models |
| `src/components/dashboard/ActivityBarChart.tsx` | tooltip token |

---

*文档版本：v0.1 — 2026-08-18*
