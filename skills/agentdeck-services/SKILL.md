---
name: agentdeck-services
description: >-
  Manage local dev projects/services via AgentDeck REST API (127.0.0.1:8788):
  list status, start/stop/restart, logs, register services from agent judgment.
  Use when the user or agent needs to run local apps, check health, register
  npm scripts (e.g. core + desktop), or replace ad-hoc bin start scripts.
---

# AgentDeck Local Services

> 本技能维护于 AgentDeck 源码仓库：`skills/agentdeck-services/`（与 Desktop 同源版本控制）。

通过 **AgentDeck Desktop** 的本机 REST API 管理本地开发服务（Project → 多 Service）。

**不要**再使用旧的 `local-service-manager` / 各项目 `bin/start` 调度；统一走本技能。
**注册新服务也走本技能**（读代码 → `POST /api/services` upsert），不要依赖应用内向导。

## 前置条件

1. **AgentDeck Desktop 正在运行**（API：`http://127.0.0.1:8788`）
2. 先探活：

```bash
curl -fsS http://127.0.0.1:8788/health
```

不可达时：提示用户先打开 AgentDeck，不要猜测端口去 `kill`。

可选 CLI（本技能自带）：

```bash
# 仓库内路径（推荐）；若已 symlink 到 ~/.agents/skills 亦可
SVC=skills/agentdeck-services/scripts/agentdeck-svc.sh
# 或: SVC=~/.agents/skills/agentdeck-services/scripts/agentdeck-svc.sh
chmod +x "$SVC"   # 首次
$SVC health
$SVC projects
$SVC status
$SVC start voxlab/main
```

## 数据模型

- **Project**：`id` + `name` + `projectDir`
- **Service**：项目内一条启动命令（一个 PID），如 `main` / `core` / `desktop`
- **服务键**：`{projectId}/{serviceId}`（例：`myapp/core`）

配置文件：`~/.agentdeck/services.json`（由 AgentDeck 维护，Skill 不要手改除非排障）。

## API 速查

Base：`http://127.0.0.1:8788`

| 操作 | 方法 | 路径 |
|------|------|------|
| 全部状态 | GET | `/api/services` |
| 按项目 | GET | `/api/services/projects` |
| 单服务 | GET | `/api/services/{projectId}/{serviceId}` |
| 日志 | GET | `/api/services/{projectId}/{serviceId}/logs?lines=80` |
| 启动 | POST | `/api/services/{projectId}/{serviceId}/start` → **202** |
| 停止 | POST | `/api/services/{projectId}/{serviceId}/stop` |
| 重启 | POST | `/api/services/{projectId}/{serviceId}/restart` |
| 打开 URL | POST | `/api/services/{projectId}/{serviceId}/open` |
| 注册/更新 | POST | `/api/services` body = registration payload |
| 重命名项目 | PATCH | `/api/services/projects/{projectId}` body `{"name":"..."}` |
| 重命名服务 | PATCH | `/api/services/{projectId}/{serviceId}` body `{"name":"..."}` |
| 删除 | DELETE | `/api/services/{projectId}/{serviceId}` |

`scan` / `probe` **不**通过 HTTP 暴露（仅 AgentDeck 桌面向导走 Tauri）。
`id` 不变；PATCH 只改展示名 `name`。

统一成功：`{ "ok": true, "data": ... }`  
失败：`{ "ok": false, "error": { "code", "message" } }`

### 启停约定（重要）

- `start` / `stop` / `restart` 是 **异步受理**（202 + message）
- Agent 应 **轮询** `GET .../{projectId}/{serviceId}`，直到：
  - 成功：`state` 为 `running`（或业务接受的 `partial`）
  - 失败：`unhealthy` / `stale_pid` / `port_conflict` / `stopped`（启动失败）
- 间隔建议 2s，总超时参考服务的 `health.timeoutSecs`（常 60–120s）
- 失败时拉日志：`GET .../logs?lines=120`

## 工作流

### 1. 查看与操作已有服务

```bash
curl -s http://127.0.0.1:8788/api/services/projects | jq .
curl -s -X POST http://127.0.0.1:8788/api/services/voxlab/main/start
# 轮询
curl -s http://127.0.0.1:8788/api/services/voxlab/main | jq '.data.state,.data.health'
```

### 2. 创建 / 注册项目与服务（Agent 决策 → upsert）

Agent 通常已在项目目录、能读代码。推荐：

1. 读 `package.json` / README / 端口配置 / 现有脚本，自行判断：
   - `project.id` / `name` / `projectDir`（绝对路径）
   - 每个进程一条 `service`：`id`、`start.command`、`ports`、`health.url`、`pidFile`、`logFile`
2. 需要时先问用户确认关键歧义（多入口、端口、多进程拆分）
3. 直接 `POST /api/services` upsert
4. 再 `start` + 轮询状态；失败拉 logs 修正配置后再次 upsert

同一项目多个进程（如 `npm run core` + `npm run desktop`）：

- 两次 upsert，**相同** `project.id` / `projectDir`
- **不同** `service.id` 与命令/端口/pid/log

### 3. Registration payload 形状

```json
{
  "project": {
    "id": "myapp",
    "name": "MyApp",
    "projectDir": "/absolute/path/to/MyApp"
  },
  "service": {
    "id": "core",
    "name": "Core",
    "start": { "command": ["npm", "run", "core"], "cwd": "{projectDir}", "env": {} },
    "pidFile": "{projectDir}/data/core.pid",
    "logFile": "{projectDir}/data/core.log",
    "ports": [3001],
    "health": {
      "url": "http://localhost:3001/",
      "statusOk": true,
      "fallbackUrls": [],
      "acceptHttpCodes": ["200", "404"],
      "timeoutSecs": 90,
      "pollIntervalSecs": 2
    },
    "openUrl": "http://localhost:3001",
    "stop": { "graceSecs": 8, "cleanupPorts": [3001] }
  }
}
```

`cwd` / `pidFile` / `logFile` 支持 `{projectDir}` 模板。

## Agent 行为准则

1. 先 `health`，再操作  
2. 用服务键 `projectId/serviceId`，不要只靠端口猜测杀进程  
3. 端口冲突时读 `error` / 状态 `port_conflict`，改端口或先 stop 冲突服务  
4. **注册以读代码决策 + upsert 为主**；不要找 HTTP `scan`/`probe`（已下线）  
5. 写坏配置后用 upsert 覆盖；失败靠 logs 迭代  
6. 详细 HTTP 文档：浏览器打开 `http://127.0.0.1:8788/docs` 或 `GET /api/docs/markdown`

## 常见问题

- **API 连不上**：AgentDeck 未启动，或 8788 被占用  
- **start 返回成功但仍 stopped**：看 logs；可能依赖缺失或端口被占  
- **stale_pid**：PID 文件残留但进程已死；再 start 或 stop 会清理  
- **同目录第二个服务**：upsert 时复用同一 `project.id`，换 `service.id`
