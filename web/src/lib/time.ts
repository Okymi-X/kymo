const UNITS: [number, string][] = [
  [86400, 'day'],
  [3600, 'hour'],
  [60, 'minute'],
]

/** "just now", "5 minutes ago", "2 hours ago". Older than a week falls back to a date. */
export function timeAgo(unixSeconds: number, nowMs: number = Date.now()): string {
  const seconds = Math.max(0, Math.floor(nowMs / 1000 - unixSeconds))
  if (seconds < 45) return 'just now'
  if (seconds >= 7 * 86400) return new Date(unixSeconds * 1000).toLocaleDateString()
  for (const [size, name] of UNITS) {
    if (seconds >= size) {
      const n = Math.floor(seconds / size)
      return `${n} ${name}${n === 1 ? '' : 's'} ago`
    }
  }
  return '1 minute ago'
}
