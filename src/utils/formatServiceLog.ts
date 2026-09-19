/** Strip ANSI SGR sequences (with or without ESC prefix). */
export function stripAnsi(text: string): string {
  let out = ''
  const chars = [...text]
  for (let i = 0; i < chars.length; i++) {
    const ch = chars[i]
    if (ch === '\u001b' || ch === '\u009b') {
      i += 1
      while (i < chars.length && !/[A-Za-z]/.test(chars[i])) {
        i += 1
      }
      continue
    }
    out += ch
  }
  // Orphan codes like [94m when ESC was lost in redirect
  return out.replace(/\[(?:\d{1,3};)*\d{1,3}m/g, '')
}

/** Simulate terminal CR so tqdm progress lines collapse to their final state. */
export function normalizeCarriageReturns(text: string): string {
  const lines: string[] = []
  let current = ''
  for (let i = 0; i < text.length; i++) {
    const ch = text[i]
    if (ch === '\r') {
      current = ''
    } else if (ch === '\n') {
      lines.push(current)
      current = ''
    } else {
      current += ch
    }
  }
  if (current.length > 0) {
    lines.push(current)
  }
  return lines.join('\n')
}

const PROGRESS_LINE =
  /^(Generating:\s*\d+%|\d+\/\d+\s*\[|[\s\-|]*\d+%[\s\-|]*\||.*\|\s*\d+\/\d+\s*\[)/

export function isProgressNoiseLine(line: string): boolean {
  const trimmed = line.trim()
  if (!trimmed) return true
  return PROGRESS_LINE.test(trimmed) || /^[\s|.\-]+$/.test(trimmed)
}

export function formatServiceLog(
  raw: string,
  options?: { hideProgress?: boolean; maxLines?: number }
): string[] {
  let text = stripAnsi(raw)
  text = normalizeCarriageReturns(text)

  let lines = text.split('\n').map((l) => l.trimEnd())

  if (options?.hideProgress) {
    lines = lines.filter((l) => !isProgressNoiseLine(l))
  }

  lines = lines.filter((l, idx, arr) => {
    // Collapse multiple blank lines
    if (l.trim() === '' && arr[idx - 1]?.trim() === '') return false
    return true
  })

  const max = options?.maxLines ?? lines.length
  if (lines.length > max) {
    lines = lines.slice(-max)
  }

  return lines
}

/** Heuristic: line looks like an error / fatal / exception starter. */
const ERROR_LINE =
  /\b(?:ERROR|FATAL|CRITICAL|PANIC)\b|(?:^|\s)(?:Error|Exception|TypeError|ReferenceError|SyntaxError|RangeError|URIError):\s|Traceback \(most recent call last\)|npm ERR!|ELIFECYCLE|\bECONNREFUSED\b|\bEADDRINUSE\b|\bEACCES\b|\[(?:error|err|fatal)\]/i

/** Avoid false positives like "0 errors", "error rate", "no error". */
const ERROR_FALSE_POSITIVE =
  /\b0\s+errors?\b|\bno\s+errors?\b|\berror[s]?\s*[:=]\s*0\b|\berror[-_\s]?rate\b|\berror[-_\s]?bound\b/i

export function isErrorLogLine(line: string): boolean {
  const trimmed = line.trim()
  if (!trimmed || trimmed.length < 5) return false
  if (ERROR_FALSE_POSITIVE.test(trimmed)) return false
  return ERROR_LINE.test(trimmed)
}

/** Indices of error lines within an already-formatted log line list. */
export function findErrorLineIndices(lines: string[]): number[] {
  const out: number[] = []
  for (let i = 0; i < lines.length; i++) {
    if (isErrorLogLine(lines[i])) out.push(i)
  }
  return out
}

/** Parse first `[YYYY-MM-DD HH:mm:ss(.ms)?]` as local time. */
export function parseLogLineTimestamp(line: string): number | null {
  const m = line.match(/\[(\d{4})-(\d{2})-(\d{2})[ T](\d{2}):(\d{2}):(\d{2})(?:\.\d+)?\]/)
  if (!m) return null
  const y = Number(m[1])
  const mo = Number(m[2])
  const d = Number(m[3])
  const h = Number(m[4])
  const mi = Number(m[5])
  const s = Number(m[6])
  const ms = new Date(y, mo - 1, d, h, mi, s).getTime()
  return Number.isFinite(ms) ? ms : null
}

/**
 * Signature used for ignore matching. Strips leading timestamps so recurring
 * errors (each logged with a fresh `[YYYY-MM-DD HH:mm:ss]` prefix) collapse
 * into one signature and stay ignored across polls.
 */
export function errorSignature(line: string): string {
  return line.trim().replace(/^\[?\d{4}-\d{2}-\d{2}[ T]\d{2}:\d{2}:\d{2}(?:\.\d+)?\]?\s*/, '')
}

/** Compact Chinese relative time, e.g. `刚刚` / `3 分钟前`. */
export function formatErrorAgo(atMs: number | null | undefined, nowMs = Date.now()): string {
  if (atMs == null || !Number.isFinite(atMs)) return '未知时间'
  const secs = Math.max(0, Math.floor((nowMs - atMs) / 1000))
  if (secs < 15) return '刚刚'
  if (secs < 60) return `${secs} 秒前`
  if (secs < 3600) return `${Math.floor(secs / 60)} 分钟前`
  if (secs < 86400) return `${Math.floor(secs / 3600)} 小时前`
  return `${Math.floor(secs / 86400)} 天前`
}

export function analyzeServiceLogErrors(
  raw: string,
  options?: { hideProgress?: boolean; maxLines?: number; ignoredSignatures?: Set<string> }
): {
  lines: string[]
  errorIndices: number[]
  latestAtMs: number | null
  latestLine: string | null
} {
  const lines = formatServiceLog(raw, {
    hideProgress: options?.hideProgress ?? true,
    maxLines: options?.maxLines ?? 300,
  })
  const ignored = options?.ignoredSignatures
  const errorIndices = findErrorLineIndices(lines).filter((idx) => {
    if (!ignored || ignored.size === 0) return true
    return !ignored.has(errorSignature(lines[idx] ?? ''))
  })

  let latestAtMs: number | null = null
  let latestLine: string | null = null
  for (const idx of errorIndices) {
    const line = lines[idx]
    const at = parseLogLineTimestamp(line)
    if (at != null && (latestAtMs == null || at >= latestAtMs)) {
      latestAtMs = at
      latestLine = line
    } else if (latestLine == null) {
      latestLine = line
    }
  }

  return { lines, errorIndices, latestAtMs, latestLine }
}

/** Slice surrounding context for copy/debug (inclusive). */
export function sliceLogContext(
  lines: string[],
  centerIdx: number,
  radius = 5
): string {
  const start = Math.max(0, centerIdx - radius)
  const end = Math.min(lines.length - 1, centerIdx + radius)
  return lines.slice(start, end + 1).join('\n')
}
