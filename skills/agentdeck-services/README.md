# AgentDeck Services Skill

一次安装，之后在任意项目里直接用中文让 Agent 管理本机开发服务（注册 / 启停 / 日志 / 健康检查）。

适用于 Cursor、Claude Code、Codex、Gemini CLI 等支持 Agent Skills 的工具。

> 前提：本机 **AgentDeck Desktop** 正在运行（API `http://127.0.0.1:8788`）。

---

## 安装方式（给 Agent）

请将下列文件安装到用户级 Skill 目录（优先全局，便于跨项目使用）：

| 文件 | URL |
|------|-----|
| `SKILL.md` | http://127.0.0.1:8788/skills/agentdeck-services/SKILL.md |
| `scripts/agentdeck-svc.sh` | http://127.0.0.1:8788/skills/agentdeck-services/scripts/agentdeck-svc.sh |

目标目录（按你当前 Agent 选择其一或同时写入）：

- `~/.cursor/skills/agentdeck-services/`
- `~/.agents/skills/agentdeck-services/`

步骤建议：

1. 创建目录 `agentdeck-services/` 与子目录 `scripts/`
2. 下载上表两个文件到对应路径
3. `chmod +x scripts/agentdeck-svc.sh`
4. 告诉用户：多数 Agent 仅在**新会话**开始时扫描 Skill，如需请开启新会话后再用

若本机已有 AgentDeck 源码仓库，也可直接 symlink：

```bash
ln -sfn /absolute/path/to/AgentDeck/skills/agentdeck-services ~/.cursor/skills/agentdeck-services
ln -sfn /absolute/path/to/AgentDeck/skills/agentdeck-services ~/.agents/skills/agentdeck-services
```

---

## 装好后可以这样说

- 用 agentdeck-services 把当前项目的本地服务注册到 AgentDeck
- 查看 AgentDeck 里有哪些本地服务、哪些在跑
- 启动 / 停止 / 重启 `voxlab/main`
- 拉一下 `myapp/core` 最近 120 行日志

---

## 验证安装是否成功

对新会话说：

> 请用 agentdeck-services 探活 AgentDeck，并列出当前已注册的项目与服务。

成功时你应看到：先 `curl`/`health` 探活，再返回项目列表或明确的「暂无服务」说明（而不是找不到技能）。

---

## 完整能力说明

见同目录 [`SKILL.md`](./SKILL.md)（或 http://127.0.0.1:8788/skills/agentdeck-services/SKILL.md）。
