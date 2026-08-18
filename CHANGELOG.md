# Changelog

本文件记录 AgentDeck 的用户可见变更，按 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 组织。

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
