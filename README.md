# AgentDeck 🛸

> **The Command Deck for Your AI Coding Agents.**
> 专为 AI 时代开发者打造的全平台 AI 编程智能体全景驾驶舱与资产管理桌面应用。

---

## ✨ 核心特性

- 🛸 **全景数据驾驶舱 (Dashboard)**：
  - **4 大核心 KPI**：全量会话数、用户提问 Prompt 数、交互总消息数、工具调用执行数。
  - **Agent 平台分布占比**：支持「按会话数」与「按消息数」双维秒级平滑切换（Cursor、Antigravity、Claude、Hermes、Codex、WorkBuddy）。
  - **24 小时活跃时段 (Hourly Punchcard)**：支持双维切换，5 级科技蓝热力色阶，直观洞察高频编码时段。
  - **深度会话排行榜 Top 10**：按全部消息/用户提问双模式排行，一键直达深度复盘。
  - **热门项目工作区分布 Top 8**：掌握每个工程项目的 AI 辅助投入占比。
  - **Agent 工具调用分布 (Tool Usage)**：细分文件阅读、代码编辑、终端命令、搜索检索等 7 大类别。
- 🔍 **Raycast 风格 Spotlight 搜索 (`⌘K`)**：
  - 全局极速搜索 1,000+ 会话与代码片段，支持用户提问与全部消息筛选，键盘无缝上下导航与回车直达。
- 📑 **三栏高效会话浏览器**：
  - 左栏工作区与收藏夹导航、中栏多 Agent 状态徽标与会话卡片、右栏 Markdown 会话流与代码高亮。
- ⭐ **星标收藏体系**：
  - 深度精华会话一键收藏，专属视图集中沉淀。
- ⚡️ **极致性能与质感 (Tauri 2.0)**：
  - 启动 < 100ms，常驻内存仅 ~25MB，macOS 原生窗口与现代暗色极简美学。

---

## 🛠️ 技术栈

- **桌面框架**：Tauri 2.0 (Rust)
- **前端核心**：React 18/19 + TypeScript + Vite
- **UI & 样式**：Tailwind CSS + Lucide Icons + Framer Motion
- **本地存储**：SQLite (rusqlite) + FTS5

---

## 🚀 快速启动

### 开发模式
```bash
# 安装依赖
npm install

# 启动桌面端调试
npm run tauri dev
```

### 生产打包
```bash
npm run tauri build
```
