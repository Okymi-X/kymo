import type { Category, EventInfo, EventState, PlayerChallenge, Scoring, Verdict } from '../api/platformTypes'

export const stateLabel: Record<EventState, string> = {
  draft: 'Draft',
  registration_open: 'Registration open',
  live: 'Live',
  paused: 'Paused',
  ended: 'Ended',
  archived: 'Archived',
}

/** Transitions the server allows, mirrored for the admin buttons (the server still decides). */
export const nextStates: Record<EventState, EventState[]> = {
  draft: ['registration_open', 'live'],
  registration_open: ['draft', 'live'],
  live: ['paused', 'ended'],
  paused: ['live', 'ended'],
  ended: ['archived'],
  archived: [],
}

/** "1h 05m", "4m 09s", "38s". Negative values count as zero. */
export function countdown(totalSeconds: number): string {
  const s = Math.max(0, Math.floor(totalSeconds))
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  const r = s % 60
  if (h > 0) return `${h}h ${String(m).padStart(2, '0')}m`
  if (m > 0) return `${m}m ${String(r).padStart(2, '0')}s`
  return `${r}s`
}

export interface CategoryGroup {
  category: Category
  challenges: PlayerChallenge[]
}

/** Categories in their stored order, each with its challenges ordered by points then title.
 *  Categories without a visible challenge are left out. */
export function groupByCategory(categories: Category[], challenges: PlayerChallenge[]): CategoryGroup[] {
  return [...categories]
    .sort((a, b) => a.position - b.position || a.id - b.id)
    .map((category) => ({
      category,
      challenges: challenges
        .filter((c) => c.category_id === category.id)
        .sort((a, b) => a.points - b.points || a.title.localeCompare(b.title)),
    }))
    .filter((g) => g.challenges.length > 0)
}

export function scoringLabel(s: Scoring): string {
  return s.kind === 'static' ? `${s.points} points` : `${s.initial} down to ${s.minimum}`
}

export const verdictText: Record<Verdict, string> = {
  correct: 'Correct',
  incorrect: 'Incorrect',
  bad_format: 'Wrong format',
}

/** `datetime-local` value for a unix time in the browser's zone, or an empty string. */
export function toLocalInput(unix: number | null | undefined): string {
  if (unix === null || unix === undefined) return ''
  const d = new Date(unix * 1000)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`
}

/** Unix seconds for a `datetime-local` value, or null when empty or invalid. */
export function fromLocalInput(value: string): number | null {
  if (!value) return null
  const ms = new Date(value).getTime()
  return Number.isNaN(ms) ? null : Math.floor(ms / 1000)
}

/** "4.2 MB", "310 B". */
export function fileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

/** The shape a flag should have, for example `KYMO{...}`. */
export function flagHint(prefix: string, suffix: string): string {
  return prefix || suffix ? `${prefix}...${suffix}` : ''
}

/** Seconds as a short duration: "90 s", "15 min", "2 h", "1 d". Exact multiples only. */
export function shortDuration(secs: number): string {
  if (secs > 0 && secs % 86400 === 0) return `${secs / 86400} d`
  if (secs > 0 && secs % 3600 === 0) return `${secs / 3600} h`
  if (secs > 0 && secs % 60 === 0) return `${secs / 60} min`
  return `${secs} s`
}

export function bytesLabel(n: number): string {
  return fileSize(n)
}

export type Registration = { open: true } | { open: false; reason: string }

/**
 * Whether people can register for an event right now, and if not, why in plain words. Mirrors
 * the server's rule (the server still decides): a live event only takes late registrations
 * when it has a registration close time that has not passed.
 */
export function registrationStatus(
  event: Pick<EventInfo, 'effective_state' | 'registration_opens' | 'registration_closes'>,
  nowSeconds: number,
): Registration {
  const { effective_state: state, registration_opens: opens, registration_closes: closes } = event
  const when = (t: number) => new Date(t * 1000).toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' })
  switch (state) {
    case 'draft':
      return { open: false, reason: 'Registration has not opened yet.' }
    case 'registration_open':
      if (opens !== null && nowSeconds < opens) return { open: false, reason: `Registration opens ${when(opens)}.` }
      if (closes !== null && nowSeconds >= closes) return { open: false, reason: 'Registration has closed.' }
      return { open: true }
    case 'live':
      if (closes !== null && nowSeconds < closes) return { open: true }
      return { open: false, reason: 'Registration is closed: the event has already started.' }
    case 'paused':
      return { open: false, reason: 'The event is paused, so registration is closed.' }
    default:
      return { open: false, reason: 'This event has ended.' }
  }
}

/** Placeholder far future dates (year 2096 and later) mean "no deadline", not a real date. */
export const NO_DEADLINE_AFTER = 4_000_000_000
export function isNoDeadline(t: number): boolean {
  return t >= NO_DEADLINE_AFTER
}

export type EventCountdownTarget = { kind: 'starts' | 'ends'; at: number } | { kind: 'ended'; at: number | null }

export function eventCountdownTarget(
  event: Pick<EventInfo, 'effective_state' | 'starts_at' | 'ends_at' | 'ended_at'>,
  nowSeconds: number,
): EventCountdownTarget | null {
  if (event.effective_state === 'ended' || event.effective_state === 'archived') {
    return { kind: 'ended', at: event.ended_at }
  }
  const starts = event.starts_at !== null && !isNoDeadline(event.starts_at) ? event.starts_at : null
  const ends = event.ends_at !== null && !isNoDeadline(event.ends_at) ? event.ends_at : null
  const active = event.effective_state === 'live' || event.effective_state === 'paused' || (starts !== null && nowSeconds >= starts)
  if (active) {
    if (ends === null) return null
    return ends > nowSeconds ? { kind: 'ends', at: ends } : { kind: 'ended', at: ends }
  }
  return starts !== null && starts > nowSeconds ? { kind: 'starts', at: starts } : null
}

/** Compact event countdown with seconds at every duration. */
export function eventCountdown(totalSeconds: number): string {
  const seconds = Math.max(0, Math.ceil(totalSeconds))
  const days = Math.floor(seconds / 86_400)
  const hours = Math.floor((seconds % 86_400) / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  const remainingSeconds = seconds % 60
  const parts = [`${minutes}m`, `${remainingSeconds}s`]
  if (hours > 0 || days > 0) parts.unshift(`${hours}h`)
  if (days > 0) parts.unshift(`${days}d`)
  return parts.join(' ')
}

export function eventEndedAgo(endedAt: number, nowSeconds: number): string {
  const elapsed = Math.max(0, nowSeconds - endedAt)
  if (elapsed < 86_400) {
    const hours = Math.floor(elapsed / 3600)
    const minutes = Math.floor((elapsed % 3600) / 60)
    return `${hours}h ${minutes}m ago`
  }
  const days = Math.floor(elapsed / 86_400)
  if (days < 30) return `${days}d ago`
  if (days < 365) return `${Math.floor(days / 30)}mo ago`
  const years = Math.floor(days / 365)
  return `${years} ${years === 1 ? 'year' : 'years'} ago`
}

/** A lab host as an option in a list: its gateway, switched-off state and how busy it is, so the admin can pick by what it has to spare. */
export function labLabel(l: { name: string; enabled: boolean; vpn: { only: boolean } | null; events: number; live_instances: number }): string {
  const gateway = l.vpn ? (l.vpn.only ? ' (VPN only)' : ' (VPN)') : ''
  const off = l.enabled ? '' : ' (switched off)'
  const load = `${l.events} event${l.events === 1 ? '' : 's'}, ${l.live_instances} running`
  return `${l.name}${gateway}${off} · ${load}`
}
