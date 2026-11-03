import type { Category, EventState, PlayerChallenge, Scoring, Verdict } from '../api/platformTypes'

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
