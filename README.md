# AgentDeck

本地 AI 编程智能体的驾驶舱：把 Cursor、Antigravity、Claude Code、Codex、Hermes、WorkBuddy 等历史会话同步进 SQLite，用桌面端浏览、检索、复盘，并提供本机 REST API 给外部 Agent 调用。

当前版本 **0.2.6**，基于 Tauri 2 + React + TypeScript，数据目录默认在 `~/.agentdeck/`。

## 功能

- **全景数据大盘**：会话数、用户提问、消息量、工具调用；按来源 / 时段 / 项目分布。
- **三栏会话浏览**：工作区导航、会话卡片、Markdown 消息流；支持星标收藏与 Spotlight（`⌘K`）。
- **提示词库**：独立收藏外部 prompt（分类、标签、来源备注），不绑定某一条会话。
- **项目分析**：按工作区生成复盘报告与功能模块。
- **在 AI IDE 中打开**：选中项目后，可从会话列表用 Cursor / Antigravity 打开目录，或在终端启动 Claude Code / Codex。
- **本机 REST API**：`127.0.0.1:8788`，免 Token，仅回环网卡。交互文档见 `/docs`。
- **备份与媒体归档**：SQLite 热快照、媒体镜像到 `~/.agentdeck/media/`。

## 技术栈

- 桌面：Tauri 2（Rust）
- 前端：React + TypeScript + Vite + Tailwind CSS
- 存储：SQLite（rusqlite）+ FTS5

## 开发

需要 Node.js、Rust 与 Tauri 的系统依赖（macOS 需 Xcode Command Line Tools）。

```bash
npm install
npm run tauri dev
```

REST API 随应用启动，默认：

- 健康检查：`http://127.0.0.1:8788/health`
- 接口文档：`http://127.0.0.1:8788/docs`

## 打包

```bash
npm run tauri build
```

产物在 `src-tauri/target/release/bundle/`。发版时同步改这三处版本号（其余从 Cargo / `package.json` 读取）：

1. `package.json`
2. `src-tauri/Cargo.toml`
3. `src-tauri/tauri.conf.json`

并更新 `CHANGELOG.md`。打包前会执行 `npm run generate:api-docs`，生成 `src-tauri/src/api_docs.generated.md`。

## 本机 API（Agent）

服务只监听 `127.0.0.1:8788`。提示词库主流程：

1. `GET /api/prompts/categories` — 分类候选（`category` 必须用返回的 `value`）
2. `GET /api/prompts` — 搜索查重（列表只有 `content_preview`）
3. `POST /api/prompts` — 插入（`title` 必填）
4. `DELETE /api/prompts/:id` — 删除

完整说明与在线调试见 `/docs`，Markdown 规范：`GET /api/docs/markdown`。

会话检索（只读）还包括 `/api/stats`、`/api/workspaces`、`/api/conversations`、`/api/user-messages`、`/api/search` 等。

## 数据

- 配置与库：`~/.agentdeck/`
- 不要把本地数据库、备份包或绝对用户路径提交进仓库
