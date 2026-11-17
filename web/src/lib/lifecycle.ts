// The steps an event goes through and the moves between them, in the words a button says.

import type { EventInfo, EventState } from '../api/platformTypes'
import { nextStates } from './platform'
import { timeAgo, timeUntil } from './time'

export const STEPS: { state: EventState; label: string }[] = [
  { state: 'draft', label: 'Draft' },
  { state: 'registration_open', label: 'Registration' },
  { state: 'live', label: 'Live' },
  { state: 'ended', label: 'Ended' },
  { state: 'archived', label: 'Archived' },
]

/** Where the event sits on the line. A paused event is still on the Live step. */
export function stepOf(state: EventState): number {
  return STEPS.findIndex((s) => s.state === (state === 'paused' ? 'live' : state))
}

export type MoveTone = 'primary' | 'secondary' | 'danger'

export interface Move {
  to: EventState
  label: string
  tone: MoveTone
}

const LABELS: Partial<Record<EventState, Partial<Record<EventState, [string, MoveTone]>>>> = {
  draft: { registration_open: ['Open registration', 'primary'], live: ['Go live', 'secondary'] },
  registration_open: { live: ['Go live', 'primary'], draft: ['Back to draft', 'secondary'] },
  live: { paused: ['Pause', 'secondary'], ended: ['End event', 'danger'] },
  paused: { live: ['Resume', 'primary'], ended: ['End event', 'danger'] },
  ended: { archived: ['Archive', 'primary'], live: ['Reopen', 'secondary'] },
}

/** The moves open to an event now: the usual next step first, the one that ends something last. */
export function movesFrom(state: EventState): Move[] {
  const order: Record<MoveTone, number> = { primary: 0, secondary: 1, danger: 2 }
  return nextStates[state]
    .map((to) => {
      const [label, tone] = LABELS[state]?.[to] ?? [`Move to ${to}`, 'secondary' as MoveTone]
      return { to, label, tone }
    })
    .sort((a, b) => order[a.tone] - order[b.tone])
}

/** One line on what is going on, for the top of the card. */
export function statusLine(event: Pick<EventInfo, 'state' | 'starts_at' | 'ends_at' | 'ended_at'>, nowMs: number): string {
  const until = (t: number | null) => (t === null ? null : timeUntil(t, nowMs))
  switch (event.state) {
    case 'draft':
      return 'Only organizers can see it'
    case 'registration_open': {
      const start = until(event.starts_at)
      return start && start !== 'now' ? `Starts ${start}` : 'Open for registration'
    }
    case 'live': {
      const end = until(event.ends_at)
      return end === null ? 'No end time' : end === 'now' ? 'Past its end time' : `Ends ${end}`
    }
    case 'paused':
      return 'Paused'
    case 'ended':
      return event.ended_at ? `Ended ${timeAgo(event.ended_at, nowMs)}` : 'Ended'
    case 'archived':
      return 'Hidden from players'
  }
}
