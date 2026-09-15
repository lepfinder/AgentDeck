#!/usr/bin/env bash
# Thin CLI for AgentDeck local services REST API (127.0.0.1:8788)
set -euo pipefail

BASE="${AGENTDECK_API:-http://127.0.0.1:8788}"

die() { echo "error: $*" >&2; exit 1; }

need_jq() {
  command -v jq >/dev/null 2>&1 || die "需要 jq（brew install jq）"
}

ensure_api() {
  if ! curl -fsS --max-time 2 "$BASE/health" >/dev/null 2>&1; then
    die "AgentDeck API 不可达（$BASE）。请先启动 AgentDeck Desktop。"
  fi
}

usage() {
  cat <<'EOF'
Usage: agentdeck-svc <command> [args]

  health                         检查 AgentDeck API
  projects                       列出项目与服务摘要
  list | status [project/service] 全部或单个服务状态
  logs <project/service> [n]     日志尾部（默认 80 行）
  start|stop|restart <key>       启停重启（异步，请再 status）
  open <project/service>         打开 openUrl
  register <payload.json>        upsert 注册/更新（JSON 文件）
  rename-project <projectId> <name>
  rename-service <project/service> <name>
  remove <project/service>       强制停止并删除配置

Environment:
  AGENTDECK_API   默认 http://127.0.0.1:8788
EOF
}

http() {
  local method="$1"; shift
  local path="$1"; shift
  curl -fsS -X "$method" "$BASE$path" \
    -H 'Content-Type: application/json' \
    "$@"
}

split_key() {
  local key="$1"
  [[ "$key" == */* ]] || die "服务键格式应为 projectId/serviceId，例如 voxlab/main"
  echo "${key%%/*}" "${key#*/}"
}

cmd="${1:-}"
[[ -n "$cmd" ]] || { usage; exit 1; }
shift || true

case "$cmd" in
  -h|--help|help) usage; exit 0 ;;
  health)
    curl -fsS "$BASE/health" | (command -v jq >/dev/null && jq . || cat)
    ;;
  projects)
    ensure_api; need_jq
    http GET /api/services/projects | jq .
    ;;
  list|status)
    ensure_api; need_jq
    if [[ $# -ge 1 ]]; then
      read -r pid sid < <(split_key "$1")
      http GET "/api/services/$pid/$sid" | jq .
    else
      http GET /api/services | jq .
    fi
    ;;
  logs)
    ensure_api; need_jq
    [[ $# -ge 1 ]] || die "usage: logs <project/service> [lines]"
    read -r pid sid < <(split_key "$1")
    lines="${2:-80}"
    http GET "/api/services/$pid/$sid/logs?lines=$lines" | jq .
    ;;
  start|stop|restart|open)
    ensure_api; need_jq
    [[ $# -ge 1 ]] || die "usage: $cmd <project/service>"
    read -r pid sid < <(split_key "$1")
    body="{}"
    if [[ "$cmd" == "stop" && "${2:-}" == "--force" ]]; then
      body='{"force":true}'
    fi
    http POST "/api/services/$pid/$sid/$cmd" -d "$body" | jq .
    ;;
  register)
    ensure_api; need_jq
    [[ $# -ge 1 ]] || die "usage: register <payload.json>"
    file="$1"
    [[ -f "$file" ]] || die "文件不存在: $file"
    http POST /api/services -d @"$file" | jq .
    ;;
  rename-project)
    ensure_api; need_jq
    [[ $# -ge 2 ]] || die "usage: rename-project <projectId> <name>"
    pid="$1"; shift
    name="$*"
    http PATCH "/api/services/projects/$pid" -d "$(jq -nc --arg n "$name" '{name:$n}')" | jq .
    ;;
  rename-service)
    ensure_api; need_jq
    [[ $# -ge 2 ]] || die "usage: rename-service <project/service> <name>"
    read -r pid sid < <(split_key "$1"); shift
    name="$*"
    http PATCH "/api/services/$pid/$sid" -d "$(jq -nc --arg n "$name" '{name:$n}')" | jq .
    ;;
  remove|rm|delete)
    ensure_api; need_jq
    [[ $# -ge 1 ]] || die "usage: remove <project/service>"
    read -r pid sid < <(split_key "$1")
    http DELETE "/api/services/$pid/$sid" -d '{}' | jq .
    ;;
  scan|probe)
    die "scan/probe 已从 HTTP API 下线；请读代码后用 register（upsert），或使用 AgentDeck 桌面向导"
    ;;
  *)
    usage
    die "未知命令: $cmd"
    ;;
esac
