/** Format a service CPU percent (single-core basis, may exceed 100%). */
export function formatCpuPercent(value?: number): string {
  if (value === undefined || value === null || !Number.isFinite(value)) return '—'
  return `${value.toFixed(1)}%`
}

/** Format memory in MB, switching to GB past 1024. */
export function formatMemoryMb(value?: number): string {
  if (value === undefined || value === null || !Number.isFinite(value)) return '—'
  if (value >= 1024) return `${(value / 1024).toFixed(1)} GB`
  if (value >= 100) return `${Math.round(value)} MB`
  return `${value.toFixed(1)} MB`
}
