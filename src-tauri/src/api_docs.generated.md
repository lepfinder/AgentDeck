# AgentDeck REST API 接口规范与集成文档 (v0.2.6)

> **鉴权说明 (Authentication)**: 
> 当前版本 API **无需 Token 校验 (No Token Required)**。
> 服务默认严格监听在本机回环网卡 `127.0.0.1:8788`，仅限当前 Mac 本机进程访问（外部网络无法直接连通），方便各类本地脚本、Raycast 扩展、Alfred 插件及 CLI 工具免签调用。

- **基础 URL (Base URL)**: `http://127.0.0.1:8788`
- **协议格式**: `HTTP/1.1` + `JSON (UTF-8)`

---

## 接口清单与返回值示例

### 1. 服务健康检查 (Health Check)
- **请求方法**: `GET /health`
- **功能说明**: 探测应用运行状态、版本信息及多智能体数据源可用性。
- **调用示例**:
  ```bash
  curl http://127.0.0.1:8788/health
  ```
- **返回值示例 (Response 200 OK)**:
  ```json
  {
    "ok": true,
    "status": "ok",
    "app": "AgentDeck",
    "version": "0.2.6",
    "cursor_available": true,
    "ai_available": true,
    "stats": {
      "total_conversations": 42,
      "total_user_messages": 128,
      "total_workspaces": 5
    }
  }
  ```

---

### 2. 全局大盘统计 (Dashboard Stats)
- **请求方法**: `GET /api/stats`
- **功能说明**: 获取全量会话数、用户提问数、智能体工具（Antigravity、Cursor、Claude Code、Codex、WorkBuddy）占比分布及工作区统计。
- **调用示例**:
  ```bash
  curl http://127.0.0.1:8788/api/stats
  ```
- **返回值示例 (Response 200 OK)**:
  ```json
  {
    "ok": true,
    "stats": {
      "total_conversations": 42,
      "total_user_messages": 128,
      "total_workspaces": 5,
      "source_counts": {
        "antigravity": 25,
        "cursor": 12,
        "claude": 5
      },
      "recent_active_days": 18
    },
    "cursor_available": true,
    "ai_available": true
  }
  ```

---

### 3. 项目工作区列表 (Workspaces)
- **请求方法**: `GET /api/workspaces`
- **查询参数**:
  - `q` (*string, 可选*): 按工作区名称或路径模糊过滤
- **调用示例**:
  ```bash
  curl "http://127.0.0.1:8788/api/workspaces?q=AgentDeck"
  ```
- **返回值示例 (Response 200 OK)**:
  ```json
  {
    "ok": true,
    "total": 1,
    "workspaces": [
      {
        "workspace_path": "/Users/xiyangxie/workspace/personal/AgentDeck",
        "cnt": 15,
        "total_conversations": 15,
        "total_user_messages": 48,
        "last_activity_at": "2026-08-19 11:20:00"
      }
    ]
  }
  ```

---

### 4. 工作区深度画像与热力图 (Workspace Detail)
- **请求方法**: `GET /api/workspaces/detail`
- **查询参数**:
  - `workspace` (*string, 必填*): 工作区绝对路径
- **调用示例**:
  ```bash
  curl "http://127.0.0.1:8788/api/workspaces/detail?workspace=/Users/xiyangxie/workspace/personal/AgentDeck"
  ```
- **返回值示例 (Response 200 OK)**:
  ```json
  {
    "ok": true,
    "workspace_path": "/Users/xiyangxie/workspace/personal/AgentDeck",
    "stats": {
      "total_conversations": 15,
      "total_user_messages": 48,
      "source_distribution": {
        "antigravity": 10,
        "cursor": 5
      },
      "heatmap_365": [
        { "date": "2026-08-18", "count": 8 },
        { "date": "2026-08-19", "count": 12 }
      ]
    }
  }
  ```

---

### 5. 智能体会话流列表 (Conversations)
- **请求方法**: `GET /api/conversations`
- **查询参数**:
  - `workspace` (*string, 可选*): 限定工作区路径
  - `source` (*string, 可选*): 限定工具源 (`antigravity` / `cursor` / `claude` / `codex` / `workbuddy`)
  - `limit` (*number, 可选*): 限制返回条数 (默认 50)
  - `offset` (*number, 可选*): 分页偏移量 (默认 0)
- **调用示例**:
  ```bash
  curl "http://127.0.0.1:8788/api/conversations?limit=2"
  ```
- **返回值示例 (Response 200 OK)**:
  ```json
  {
    "ok": true,
    "total": 42,
    "conversations": [
      {
        "id": "conv-1787102766025",
        "title": "重构 REST API 端口与接口文档",
        "workspace_path": "/Users/xiyangxie/workspace/personal/AgentDeck",
        "source": "antigravity",
        "model": "gemini-2.5-flash",
        "user_message_count": 8,
        "created_at": "2026-08-19 10:30:00",
        "updated_at": "2026-08-19 11:15:00"
      }
    ]
  }
  ```

---

### 6. 单个会话全量消息流 (Conversation Messages)
- **请求方法**: `GET /api/conversations/:id`
- **功能说明**: 返回指定会话的完整问答交互流，包含用户提问、Agent 思考链 (`thinking`)、工具调用 (`tool_args`) 及本地附图 (`images`)。
- **调用示例**:
  ```bash
  curl "http://127.0.0.1:8788/api/conversations/conv-1787102766025"
  ```
- **返回值示例 (Response 200 OK)**:
  ```json
  {
    "ok": true,
    "conversation_id": "conv-1787102766025",
    "messages": [
      {
        "id": "msg-1",
        "role": "user",
        "content": "现在对外的 API 有 token 校验的要求吗",
        "created_at": "2026-08-19 11:06:49",
        "source": "antigravity"
      },
      {
        "id": "msg-2",
        "role": "assistant",
        "content": "目前没有 Token 鉴权要求，服务严格监听在 127.0.0.1 回环地址。",
        "thinking": "用户询问鉴权要求，需要清晰解释本地隔离模型...",
        "tool_args": null,
        "images": [],
        "created_at": "2026-08-19 11:06:55",
        "source": "antigravity"
      }
    ]
  }
  ```

---

### 7. 全局消息全文检索 (Search)
- **请求方法**: `GET /api/search`
- **查询参数**:
  - `q` (*string, 必填*): 检索关键词
  - `limit` (*number, 可选*): 限制返回条数 (默认 50)
- **调用示例**:
  ```bash
  curl "http://127.0.0.1:8788/api/search?q=backup"
  ```
- **返回值示例 (Response 200 OK)**:
  ```json
  {
    "ok": true,
    "query": "backup",
    "total": 1,
    "results": [
      {
        "conversation_id": "conv-1787102766025",
        "message_id": "msg-1",
        "role": "user",
        "content": "检查下备份目录是否支持选择",
        "workspace_path": "/Users/xiyangxie/workspace/personal/AgentDeck",
        "created_at": "2026-08-19 10:15:00",
        "source": "antigravity"
      }
    ]
  }
  ```

---

### 8. 触发多源增量扫描与同步 (Sync)
- **请求方法**: `POST /api/sync`
- **功能说明**: 立即触发后端对全部 AI Coding 智能体目录进行增量变更扫描与 SQLite 落盘。
- **调用示例**:
  ```bash
  curl -X POST http://127.0.0.1:8788/api/sync
  ```
- **返回值示例 (Response 200 OK)**:
  ```json
  {
    "ok": true,
    "message": "Sync completed successfully",
    "synced_at": "2026-08-19 11:22:00"
  }
  ```

---

### 9. 本地持久化媒体图片资产静态分发 (Media Assets)
- **请求方法**: `GET /media/:source/:conversation_id/:filename`
- **功能说明**: 访问本地分层归档的附图媒体资产（支持图片缓存与流式二进制返回）。
- **调用示例**:
  ```bash
  curl -I "http://127.0.0.1:8788/media/antigravity/conv-123/screenshot.png"
  ```
- **响应头示例 (Response Headers)**:
  ```http
  HTTP/1.1 200 OK
  Content-Type: image/png
  Content-Length: 128492
  Cache-Control: public, max-age=86400
  Access-Control-Allow-Origin: *
  ```

---

### 10. 提示词库（Agent 维护接口）

面向外部 Agent：**先拉分类候选 → 搜索是否已有 → 插入 → 必要时删除**。
列表只返回 `content_preview`（约 120 字），全文用 `GET /api/prompts/:id`。
响应不含星标、使用次数、时间戳（这些仅在 App 内维护）。

#### 0. 获取分类候选（插入前先调用）
- **请求方法**: `GET /api/prompts/categories`
- **返回值**:
  ```json
  {
    "ok": true,
    "categories": [
      { "value": "coding", "label": "编程" },
      { "value": "image", "label": "生图" }
    ],
    "hint": "Call this before POST /api/prompts to pick a category value."
  }
  ```

#### 列表与搜索
- **请求方法**: `GET /api/prompts`
- **查询参数**:
  - `q` (*string, 可选*): 搜索标题、正文、标签
  - `category` (*string, 可选*): 必须是 `/api/prompts/categories` 返回的 `value` 之一
  - `limit` (*number, 可选*): 默认 50，最大 200
- **列表项字段**: `id`, `title`, `content_preview`, `category`, `tags`, `source_*`, `notes`（无 `content` 全文）

#### 获取单条（含全文）
- **请求方法**: `GET /api/prompts/:id`
- **返回**: `prompt.content` 为完整正文

#### 插入（主要操作）
- **请求方法**: `POST /api/prompts`
- **最简请求体**（仅 `title` 必填；`category` 默认 `coding`）:
  ```json
  { "title": "某文章里的 Review 模板", "content": "…" }
  ```
- **完整请求体**:
  ```json
  {
    "title": "深度 Code Review",
    "content": "请按 correctness / regression / style 三项 review…",
    "category": "coding",
    "tags": ["review"],
    "source_url": "https://example.com/skill",
    "source_note": "某 Skill 推荐",
    "notes": "适合大型 PR"
  }
  ```
- **错误示例（分类无效）**:
  ```json
  {
    "ok": false,
    "code": "INVALID_CATEGORY",
    "error": "invalid category: foo",
    "allowed_categories": ["coding", "research", "writing", "product", "agent", "image", "video", "persona", "meta"]
  }
  ```

#### 删除（主要操作）
- **请求方法**: `DELETE /api/prompts/:id`
- 不存在时返回 **404** + `{ "ok": false, "code": "NOT_FOUND", "error": "prompt not found" }`

#### 更新（可选，全量替换）
- **请求方法**: `PUT /api/prompts/:id`
- **请求体**: 与插入相同（非 PATCH，缺字段会按默认值/空值覆盖）

---

## 快速调用示例

### Python 脚本
```python
import requests

res = requests.get("http://127.0.0.1:8788/api/stats")
data = res.json()
print("Total Conversations:", data["stats"]["total_conversations"])
```

### Node.js
```javascript
const res = await fetch("http://127.0.0.1:8788/api/stats");
const data = await res.json();
console.log(data);
```
