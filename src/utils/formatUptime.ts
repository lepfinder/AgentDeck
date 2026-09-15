/** Format seconds into a compact Chinese uptime string. */
export function formatUptime(totalSecs: number): string {
  if (totalSecs < 0 || !Number.isFinite(totalSecs)) return '—'
  const secs = Math.floor(totalSecs)
  if (secs < 60) return `${secs} 秒`
  if (secs < 3600) {
    const m = Math.floor(secs / 60)
    const s = secs % 60
    return s > 0 ? `${m} 分 ${s} 秒` : `${m} 分`
  }
  if (secs < 86400) {
    const h = Math.floor(secs / 3600)
    const m = Math.floor((secs % 3600) / 60)
    return m > 0 ? `${h} 小时 ${m} 分` : `${h} 小时`
  }
  const d = Math.floor(secs / 86400)
  const h = Math.floor((secs % 86400) / 3600)
  return h > 0 ? `${d} 天 ${h} 小时` : `${d} 天`
}
