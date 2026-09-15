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
