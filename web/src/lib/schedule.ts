import type { Match } from '../api/types'

const TIME = new Intl.DateTimeFormat(undefined, { hour: '2-digit', minute: '2-digit' })
const DAY = new Intl.DateTimeFormat(undefined, { weekday: 'short', day: 'numeric', month: 'short' })

const startOfDay = (ms: number) => {
  const d = new Date(ms)
  d.setHours(0, 0, 0, 0)
  return d.getTime()
}

/** "Today 14:30", "Tomorrow 09:00" or "Sat, 8 Nov 14:30". */
export function formatSlot(unixSeconds: number, nowMs: number = Date.now()): string {
  const at = unixSeconds * 1000
  const days = Math.round((startOfDay(at) - startOfDay(nowMs)) / 86_400_000)
  const day = days === 0 ? 'Today' : days === 1 ? 'Tomorrow' : DAY.format(at)
  return `${day} ${TIME.format(at)}`
}

/** "in 12 minutes", "starting now", "5 minutes ago" for a planned start. */
export function relativeStart(unixSeconds: number, nowMs: number = Date.now()): string {
  const minutes = Math.round((unixSeconds * 1000 - nowMs) / 60_000)
  if (Math.abs(minutes) < 1) return 'starting now'
  const abs = Math.abs(minutes)
  const unit = abs >= 1440 ? [Math.round(abs / 1440), 'day'] : abs >= 60 ? [Math.round(abs / 60), 'hour'] : [abs, 'minute']
  const text = `${unit[0]} ${unit[1]}${unit[0] === 1 ? '' : 's'}`
  return minutes > 0 ? `in ${text}` : `${text} ago`
}

/** Planned time and place as one short line, or null when nothing is planned. */
export function planLine(m: Pick<Match, 'scheduled_at' | 'venue'>, nowMs?: number): string | null {
  const parts = [m.scheduled_at === null ? null : formatSlot(m.scheduled_at, nowMs), m.venue].filter((p): p is string => !!p)
  return parts.length ? parts.join(', ') : null
}

/** Order by planned start, earliest first, matches without a plan last. Stable. */
export function bySchedule<T extends Pick<Match, 'scheduled_at'>>(items: T[]): T[] {
  return [...items].sort((a, b) => (a.scheduled_at ?? Infinity) - (b.scheduled_at ?? Infinity))
}

/** Value for a datetime-local input in the viewer's time zone. */
export function toLocalInput(unixSeconds: number | null): string {
  if (unixSeconds === null) return ''
  const d = new Date(unixSeconds * 1000)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`
}

export function fromLocalInput(value: string): number | null {
  if (!value) return null
  const ms = new Date(value).getTime()
  return Number.isNaN(ms) ? null : Math.floor(ms / 1000)
}

/** Minutes past its planned start that a ready, unplayed match is allowed before it counts as late. */
export const LATE_AFTER_MINUTES = 10

/** Whole minutes a ready match is overdue, or null when it is on time, has no plan or was played. */
export function lateMinutes(m: Pick<Match, 'scheduled_at' | 'score' | 'forfeit' | 'slots'>, nowMs: number = Date.now()): number | null {
  if (m.scheduled_at === null || m.score !== null || m.forfeit) return null
  if (!m.slots.every((s) => s.kind === 'team')) return null
  const minutes = Math.floor((nowMs - m.scheduled_at * 1000) / 60_000)
  return minutes >= LATE_AFTER_MINUTES ? minutes : null
}
