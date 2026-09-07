# Changelog

本文件记录 AgentDeck 的用户可见变更，按 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 组织。

## [0.3.1] - 2026-09-07

### 新增与优化
- **Antigravity 与 Antigravity IDE 双目录适配与去重**：
  - 同时扫描 `~/.gemini/antigravity/brain` 与 `~/.gemini/antigravity-ide/brain`，完整兼容桌面端与 IDE 两个不同产品形态的会话数据
  - 针对安装迁移导致的跨目录相同 UUID (`cid`) 会话，引入修改时间与文件大小比对机制，自动择优选取最新副本，并将历史落后副本标记同步状态，杜绝会话重复、数据震荡以及大盘统计翻倍
  - 兼容双目录 `conversations/<cid>.db` 附图媒体提取，并在 IDE 唤起检测中同时支持 `Antigravity.app` 与 `Antigravity IDE.app`
- **设置页面数据源与监听路径全景展示**：
  - 在应用设置「数据存储与源」页面中，全景呈现所有已接入的 Coding Agent（Cursor、Google Antigravity、Claude Code、Codex、Hermes、WorkBuddy）
  - 动态探测本机各 Agent 数据源的就绪状态，展示已归档的历史会话条数统计
  - 详细罗列 AgentDeck 在本机监听与扫描的每一个配置文件、数据库和日志目录路径（包含语义说明、路径有效性指示器及一键复制绝对路径功能）
  - 新增 Tauri 内部指令 `get_agent_sources_cmd` 与本机 REST API 端点 `GET /api/agent-sources`，供前端界面与外部脚本无缝获取
- **全景驾驶舱大盘自动刷新与紧凑化 UI**：
  - 在首页「Agent 全景数据驾驶舱」顶部控制栏新增自动刷新频率切换器，支持关闭、10s、30s、1m、5m 多档位灵活配置
  - 一体化自适应下拉触发器，直接在按钮内部集成动态脉冲绿点与实时倒数（如 `[ 🟢 57s / 1m ▾ ]`），彻底消除多余空白与控件割裂感
  - 下拉菜单靠右对齐展开（`right-0 left-auto w-44`），选项完整展示且不遮挡标题或溢出屏幕
  - 本地记忆用户的刷新频率偏好（`localStorage` 持久化），并在窗口最小化或后台非激活状态下智能挂起倒计时节约 CPU 与能耗
  - 自动刷新时全面联动 4 大核心 KPI 指标卡片、24 小时活跃柱状图、近 30 天趋势图与每日并行活动甘特图

## [0.3.0] - 2026-09-05

### 新增与优化
- **WorkBuddy 解析器全面重构**：
  - 适配 WorkBuddy 原生 `.jsonl` 的结构化数组消息格式（`input_text` 与 `output_text`），彻底修复用户提问和模型正文被误判为空而丢失的问题
  - 支持毫秒级数字时间戳提取与 RFC3339 转换
  - 自动剥离前置系统注入的 `<system-reminder>`，精准提取 `<user_query>` 用户提问
  - 智能关联并聚合轮次中的 `reasoning`（思考过程）与 `function_call`（工具调用），使前端完整展示“用户提问 → 思考折叠框 + 工具调用 + 正文回复”
- **今日工作区概括汇总接口 (Daily Summary)**：
  - 新增 `GET /api/daily-summary`（别名 `/api/daily-digest`），汇总当天各项目活跃工作区与对话
  - 默认提供 `compact` 高密度紧凑文本格式 `[HH:MM:SS] [role] content`，极大幅度节约 LLM 输入 Token 消耗，支持 `format=json`、`max_len` 截断及多维过滤
- **最近活动记录接口 (Recent Activity)**：
  - 新增 `GET /api/recent-activity`（别名 `/api/recent`、`/api/hourly-activity`），查询最近 1 小时（或自定义 `minutes`/`hours`）的活跃交互
  - 新增 `format=timeline`（时间轴流）模式，直接按时间输出动态事件流，便于外部自动化看板与通知直连
- **开发端口调整**：
  - 默认开发端口调整为 `1421`，杜绝与同机其他 Tauri 应用端口冲突

## [0.2.9] - 2026-09-03

### 修复与优化
- **会话消息增量同步与历史时间戳锁定**：
  - 彻底废弃删除后重建（Full DELETE）的同步方式，改为基于 `step_index` 的增量比对更新
  - 严格锁定数据库已记录的历史消息创建时间，防止后续追加新对话导致历史消息时间漂移
  - 仅在消息内容、思考过程（Thinking）或工具调用有实际变动时增量更新，大幅降低 SQLite 与全文索引（FTS）读写开销
  - 修复 Cursor 历史 bubble 缺失时间戳时错误借用会话最后更新时间（`updated_at`）的问题，杜绝跨天时间错位

## [0.2.8] - 2026-08-25

### 新增
- **每日多项目并行活动甘特图（Daily Activity Gantt）**：
  - 精准展示任意指定日期内各项目、各 Agent 平台的提示词时间分布与并发峰值
  - 采用极简脉冲发光点阵（Pulse Dots）与严格防重叠聚类徽章（Clustered Badges）呈现多时段密集提问
  - 支持点击徽章弹出紧凑干练的提示词时间倒序流水弹窗，支持划词阅览与一键跳转会话上下文
  - 内置并发度趋势波形泳道，实现 24 小时时间轴与实时贯穿 `NOW` 指针绝对垂直对齐
  - 支持前一天/后一天快捷翻页、日历选择任意历史日期、一键回到今天，并与 24h 柱状图、近 30 天图表和年度热力图深度联动
- **REST API 新增**：新增 `GET /api/daily-timeline?date=YYYY-MM-DD` 接口供本地脚本与大盘调用

## [0.2.7] - 2026-08-20

### 新增
- **GitHub Actions**：`main` 分支 CI 检查；推送 `v*` 标签自动构建 macOS 安装包并发布 Release
- **README**：中英文文档、Star History、macOS 14+ 安装说明（含 `xattr` 去隔离）

### 修复
- **Release 构建**：CI 跳过不稳定的 DMG 打包，改为 zip 分发 `.app`（Apple Silicon + Intel）

## [0.2.6] - 2026-08-20

### 新增
- **提示词库 (Prompt Library)**：独立收藏外部 prompt，支持分类、标签、来源备注；App 内浏览与管理
- **提示词库 REST API**：`GET/POST/DELETE /api/prompts` 及分类候选接口，供本地 Agent 插入、查重与删除
- **在 AI IDE 中打开**：会话列表可一键用 Cursor / Antigravity 打开当前工作区，或在终端启动 Claude Code / Codex
- **Dashboard 查询修复**：用户消息统计兼容 Cursor `role` 字段格式

## [0.2.5] - 2026-08-19

### 新增
- **独立现代化 REST API 交互式文档与在线调试控制台 (`/docs`)**：
  - 采用现代双列布局（左侧固定接口目录与即时搜索筛选，右侧参数表单、丰富返回值示例与实时请求沙盒）
  - 全面支持浅色/深色主题切换并持久化保存，默认呈现清爽浅色模式
  - 全面去除 Emoji，统一对齐 Lucide 矢量图标体系
  - 支持一键导出并复制 Markdown 完整接口规范文档
  - 支持原生调起系统默认浏览器打开文档页面
- **REST API 服务隔离与端口标准化**：
  - 核心服务端口标准回归为 `8788`，严格限制在 `127.0.0.1` 本机回环网卡，免 Token 方便本地脚本与 CLI 无缝集成
  - 设置面板保持极简克制，提供状态指示与单入口快速访问

## [0.2.4] - 2026-08-19

### 新增
- **图片媒体资产本地分层持久化归档**：按 `<工具源>/<会话ID>/` 两级目录自动镜像归档 Antigravity 与 Cursor 历史图片到 `~/.agentdeck/media/`，支持完全离线查看与独立数据迁移
- **数据备份与灾备恢复引擎 (Backup & Restore)**：
  - 基于 SQLite `VACUUM INTO` 热快照与媒体压缩打包（`.tar.gz`），精确保留最近 3 份历史快照
  - 自动探测并支持一键预设 Google Drive（`~/Library/CloudStorage`）、iCloud Drive、本地 Documents 与 NAS 挂载路径
  - **macOS Cocoa 原生异步目录选择器**：集成 `rfd` 原生文件对话框，秒级响应且永不阻塞
  - **实时阶段进度条 (Real-time Progress Bar)**：热快照、媒体收集、Gzip 压缩与修剪全流程百分比与状态动态反馈
  - **`~/.agentdeck/config.json` 独立持久化配置体系**：替代易丢失的浏览器 LocalStorage，并随备份包一键跨端同步迁移
  - 支持每日后台静默自动备份、随时一键立即备份与一键快照回滚还原
- **设置页自定义极客下拉组件**：替换原生 Select 为半透明毛玻璃高定制下拉菜单（支持旋转箭头、状态高亮与主题拟态）
- **热力图与图表 Tooltip 玻璃拟态统一**：去除原生 Emoji 图标，全面对齐 Lucide 专业图标体系与微光玻璃质感

## [0.2.3] - 2026-08-19

### 新增
- 365 天研发日历热力图增加详尽单例 Tooltip（支持按用户消息、全部消息、会话数多维切换）
- 24 小时活跃时段 Punchcard 矩阵卡片增加打卡 Tooltip 与时间语境提示
- 消息详情页完整支持 Antigravity 与 Cursor 附图解析、预览与点击放大 Lightbox

### 优化与修复
- 解决同步时 UI 卡顿问题：补齐 SQLite 消息时间与角色 B-Tree 索引，打卡时段与工具统计重构为高效 SQL 聚合
- 修复 REST API 端口（调整为 8789）与 Vite Dev Server 端口冲突导致的图片 404 问题
- 前端全局增加大盘刷新并发保护锁，消除重复网络请求风暴

## [0.2.2] - 2026-08-18

### 变更

- 大盘将「按日 24 小时」与「近 30 天」活跃图提前到 KPI 下方
- 近 30 天活跃图改为柱状图叠加折线

## [0.2.1] - 2026-08-18

### 新增

- 设置页可配置后台自动同步频率（默认 60 秒）
- 大盘 24 小时活跃图默认展示「今天」
- 日/月活跃柱状图 tooltip 增加「用户消息数」

### 修复

- Cursor 消息时间戳按 bubble `createdAt` 落库，修复单小时堆量与会话统计偏差
- 自动同步仅在本次有新增或更新时弹窗通知

### 变更

- 后台监听默认间隔由 30 秒改为 60 秒

## [0.2.0] - 2026-08-18

首个带 changelog 的本机安装版本。覆盖自 `0.1.0` 初始提交以来的全部 git 记录，以及本轮同步与窗口行为加固。

### 新增

- 多源会话导入：Antigravity、Cursor、Claude Code、Codex、Hermes、WorkBuddy
- 后台实时监听与增量同步引擎（默认 30 秒探测数据源变动）
- Spotlight 全局搜索（⌘K）：空查询展示最近活跃会话，支持关键词检索与键盘导航
- 工作区研发分析：细粒度区块抽取、模块聚合、时间轴、Markdown 报告
- GitHub 风格活动热力图（月份标签与图例）
- LLM 主备链路：自动故障转移、连通性测试、推理内容与重试
- 嵌入式 REST API（`127.0.0.1:8788`），兼容 HomeCore / EVA
- 消息按 User Turn 分组折叠、图片灯箱预览
- 全局状态栏、设置弹窗、⌘R 刷新、北京时间（UTC+8）展示
- 品牌图标与 macOS 窗口样式

### 修复

- 同步卡死：单事务写入、WAL `synchronous=NORMAL`、避免每条消息 fsync
- Codex / WorkBuddy 改为按文件增量，失败不再误标已同步
- Cursor 增量少扫：主库未变整源跳过，JSON1 先筛再拉变更会话
- 工作区路径归一：`/workspace/...` 与本机 `~/workspace/...` 合并
- 会话内容指纹（`content_hash`），避免仅靠时间戳漏更新
- 同步冲突改为排队执行，不再静默丢弃
- 新库 schema 与真实 messages 列对齐，可独立启动
- 点击关闭按钮改为隐藏到 Dock，⌘Q 才退出；Dock 点击可恢复窗口
- 时间字段统一北京时间；设置弹窗高度跳动；窗口拖拽

### 变更

- 主库路径固定为 `~/.agentdeck/agentdeck.db`
- 大盘 Agent 分布图默认按消息数统计
- 会话消息列表默认最新在前

## [0.1.0] - 2026-08-17

- 初始提交：AgentDeck AI Coding Cockpit（Tauri v2 + Rust + React）
