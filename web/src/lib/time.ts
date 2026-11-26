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

/** "in 5 minutes", "in 2 days", "any moment". Past times read "now": callers say what already happened. */
export function timeUntil(unixSeconds: number, nowMs: number = Date.now()): string {
  const seconds = Math.floor(unixSeconds - nowMs / 1000)
  if (seconds <= 0) return 'now'
  if (seconds < 45) return 'any moment'
  for (const [size, name] of UNITS) {
    if (seconds >= size) {
      const n = Math.floor(seconds / size)
      return `in ${n} ${name}${n === 1 ? '' : 's'}`
    }
  }
  return 'in 1 minute'
}

/** The exact moment to the second, with the time zone, e.g. "26 Nov 2026, 14:03:07 GMT+1". Used where ties are decided. */
export function exactTime(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleString(undefined, {
    day: 'numeric',
    month: 'short',
    year: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
    timeZoneName: 'short',
  })
}
