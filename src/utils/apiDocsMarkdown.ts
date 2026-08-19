export function getApiDocsMarkdown(): string {
  return `# AgentDeck REST API 接口规范与集成文档 (v0.2.4)

> 💡 **鉴权说明 (Authentication)**: 
> 当前版本 API **无需 Token 校验 (No Token Required)**。
> 服务默认严格监听在本机回环网卡 \`127.0.0.1:8788\`，仅限当前 Mac 本机进程访问（外部网络无法直接连通），方便各类本地脚本、Raycast 扩展、Alfred 插件及 CLI 工具免签调用。

- **基础 URL (Base URL)**: \`http://127.0.0.1:8788\`
- **协议格式**: \`HTTP/1.1\` + \`JSON (UTF-8)\`

---

## 接口清单

### 1. 服务健康检查 (Health Check)
- **请求方法**: \`GET /health\`
- **功能说明**: 探测应用运行状态、版本信息及多智能体数据源可用性。
- **调用示例**:
  \`\`\`bash
  curl http://127.0.0.1:8788/health
  \`\`\`
- **响应示例**:
  \`\`\`json
  {
    "ok": true,
    "status": "ok",
    "app": "AgentDeck",
    "version": "0.2.4",
    "cursor_available": true,
    "ai_available": true,
    "stats": {
      "total_conversations": 42,
      "total_user_messages": 128
    }
  }
  \`\`\`

---

### 2. 全局大盘统计 (Dashboard Stats)
- **请求方法**: \`GET /api/stats\`
- **功能说明**: 获取全量会话数、用户提问数、智能体工具（Antigravity、Cursor、Claude Code、Codex、WorkBuddy）占比分布及工作区统计。
- **调用示例**:
  \`\`\`bash
  curl http://127.0.0.1:8788/api/stats
  \`\`\`

---

### 3. 项目工作区列表 (Workspaces)
- **请求方法**: \`GET /api/workspaces\`
- **查询参数**:
  - \`q\` (*string, 可选*): 按工作区名称或路径模糊过滤
- **调用示例**:
  \`\`\`bash
  curl "http://127.0.0.1:8788/api/workspaces?q=AgentDeck"
  \`\`\`

---

### 4. 工作区深度画像与热力图 (Workspace Detail)
- **请求方法**: \`GET /api/workspaces/detail\`
- **查询参数**:
  - \`workspace\` (*string, 必填*): 工作区绝对路径
- **调用示例**:
  \`\`\`bash
  curl "http://127.0.0.1:8788/api/workspaces/detail?workspace=/Users/xiyangxie/workspace/personal/AgentDeck"
  \`\`\`

---

### 5. 智能体会话流列表 (Conversations)
- **请求方法**: \`GET /api/conversations\`
- **查询参数**:
  - \`workspace\` (*string, 可选*): 限定工作区路径
  - \`source\` (*string, 可选*): 限定工具源 (\`antigravity\` / \`cursor\` / \`claude\` / \`codex\` / \`workbuddy\`)
  - \`limit\` (*number, 可选*): 限制返回条数 (默认 50)
  - \`offset\` (*number, 可选*): 分页偏移量 (默认 0)
- **调用示例**:
  \`\`\`bash
  curl "http://127.0.0.1:8788/api/conversations?limit=20"
  \`\`\`

---

### 6. 单个会话全量消息流 (Conversation Messages)
- **请求方法**: \`GET /api/conversations/:id\`
- **功能说明**: 返回指定会话的完整问答交互流，包含用户提问、Agent 思考链 (\`thinking\`)、工具调用 (\`tool_args\`) 及本地附图 (\`images\`)。
- **调用示例**:
  \`\`\`bash
  curl "http://127.0.0.1:8788/api/conversations/{conversation_id}"
  \`\`\`

---

### 7. 全局消息全文检索 (Search)
- **请求方法**: \`GET /api/search\`
- **查询参数**:
  - \`q\` (*string, 必填*): 检索关键词
  - \`limit\` (*number, 可选*): 限制返回条数 (默认 50)
- **调用示例**:
  \`\`\`bash
  curl "http://127.0.0.1:8788/api/search?q=backup"
  \`\`\`

---

### 8. 触发多源增量扫描与同步 (Sync)
- **请求方法**: \`POST /api/sync\`
- **功能说明**: 立即触发后端对全部 AI Coding 智能体目录进行增量变更扫描与 SQLite 落盘。
- **调用示例**:
  \`\`\`bash
  curl -X POST http://127.0.0.1:8788/api/sync
  \`\`\`

---

### 9. 本地持久化媒体图片资产静态分发 (Media Assets)
- **请求方法**: \`GET /media/:source/:conversation_id/:filename\`
- **功能说明**: 访问本地分层归档的附图媒体资产（支持图片缓存与流式二进制返回）。
- **调用示例**:
  \`\`\`bash
  curl -I "http://127.0.0.1:8788/media/antigravity/{conversation_id}/{filename.png}"
  \`\`\`

---

## 快速调用示例

### Python 脚本
\`\`\`python
import requests

res = requests.get("http://127.0.0.1:8788/api/stats")
data = res.json()
print("Total Conversations:", data["stats"]["total_conversations"])
\`\`\`

### Node.js
\`\`\`javascript
const res = await fetch("http://127.0.0.1:8788/api/stats");
const data = await res.json();
console.log(data);
\`\`\`
`;
}
