# AgentDeck Services Skill

一次安装，之后在任意项目里直接用中文让 Agent 管理本机开发服务（注册 / 启停 / 日志 / 健康检查）。

适用于 Cursor、Claude Code、Codex、Gemini CLI 等支持 Agent Skills 的工具。

> 使用本技能时，本机 **AgentDeck Desktop** 需正在运行（API `http://127.0.0.1:8788`）。  
> **安装 / 更新** 文件本身不依赖 Desktop，优先从 GitHub 拉取。

当前版本：**0.1.0**（与同目录 `SKILL.md` 的 `metadata.version` 一致）

---

## 安装提示词（给用户复制）

```
请安装 AgentDeck Services Skill：https://raw.githubusercontent.com/lepfinder/AgentDeck/main/skills/agentdeck-services/README.md
装完告诉我是否需要开启新会话。
```

---

## 安装方式（给 Agent）

请将下列文件**整包**安装到用户级 Skill 目录（含 `scripts/`，不要只装 `SKILL.md`）：

| 文件 | GitHub（推荐） | 本机兜底（Desktop 运行时） |
|------|----------------|---------------------------|
| `SKILL.md` | https://raw.githubusercontent.com/lepfinder/AgentDeck/main/skills/agentdeck-services/SKILL.md | http://127.0.0.1:8788/skills/agentdeck-services/SKILL.md |
| `scripts/agentdeck-svc.sh` | https://raw.githubusercontent.com/lepfinder/AgentDeck/main/skills/agentdeck-services/scripts/agentdeck-svc.sh | http://127.0.0.1:8788/skills/agentdeck-services/scripts/agentdeck-svc.sh |

目标目录（按当前 Agent 选其一，或两边都写）：

- `~/.cursor/skills/agentdeck-services/`
- `~/.agents/skills/agentdeck-services/`

步骤：

1. 创建目录 `agentdeck-services/` 与子目录 `scripts/`
2. 下载上表两个文件到对应路径（优先 GitHub；GitHub 不可用且 Desktop 在跑时再用本机 URL）
3. `chmod +x scripts/agentdeck-svc.sh`
4. 告诉用户：多数 Agent 仅在**新会话**开始时扫描 Skill，如需请开启新会话后再用

若本机已有 AgentDeck 源码仓库，也可直接 symlink（`git pull` 即更新）：

```bash
ln -sfn /absolute/path/to/AgentDeck/skills/agentdeck-services ~/.cursor/skills/agentdeck-services
ln -sfn /absolute/path/to/AgentDeck/skills/agentdeck-services ~/.agents/skills/agentdeck-services
```

---

## 更新方式

没有后台自动升级。要拿到新版本，任选其一：

### 1. 再跑一遍安装提示词（推荐）

对 Agent 说：

```
请按 https://raw.githubusercontent.com/lepfinder/AgentDeck/main/skills/agentdeck-services/README.md 更新 AgentDeck Services Skill，覆盖本地文件后告诉我版本号，以及是否需要开启新会话。
```

Agent 应：

1. 拉取远端 README / `SKILL.md`，读取 `metadata.version`
2. 与本地已安装文件对比；若远端更新（或用户明确要求更新），**覆盖** `SKILL.md` 与 `scripts/agentdeck-svc.sh`
3. 再次 `chmod +x scripts/agentdeck-svc.sh`
4. 回报新旧版本，并提醒是否需新会话

### 2. 检查是否有新版本

```
请检查我本机的 agentdeck-services 是否落后于 GitHub main，若落后则更新。
```

### 3. Symlink 用户

在 AgentDeck 仓库里 `git pull` 即可，无需再下载。

---

## 装好后可以这样说

- 阅读下这个项目，请使用 agentdeck-services 技能，把本地开发服务注册一下
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

见同目录 [`SKILL.md`](./SKILL.md)（或  
https://raw.githubusercontent.com/lepfinder/AgentDeck/main/skills/agentdeck-services/SKILL.md ）。
