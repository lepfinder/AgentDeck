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
  return PROGRESS_LINE.test(trimmed) || /^[-\s|.]+$/.test(trimmed)
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

export function analyzeServiceLogErrors(
  raw: string,
  options?: { hideProgress?: boolean; maxLines?: number }
): {
  errorIndices: number[]
  latestAtMs: number | null
} {
  const lines = formatServiceLog(raw, {
    hideProgress: options?.hideProgress ?? true,
    maxLines: options?.maxLines ?? 300,
  })
  const errorIndices = findErrorLineIndices(lines)

  let latestAtMs: number | null = null
  for (const idx of errorIndices) {
    const at = parseLogLineTimestamp(lines[idx])
    if (at != null && (latestAtMs == null || at >= latestAtMs)) {
      latestAtMs = at
    }
  }

  return { errorIndices, latestAtMs }
}
