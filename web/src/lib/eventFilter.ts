import type { EventInfo } from '../api/platformTypes'

export type EventGroup = 'all' | 'live' | 'upcoming' | 'draft' | 'finished'

export const EVENT_GROUPS: { id: EventGroup; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'live', label: 'Live' },
  { id: 'upcoming', label: 'Upcoming' },
  { id: 'draft', label: 'Drafts' },
  { id: 'finished', label: 'Finished' },
]

/** Which group an event is in, by what it is doing now (its schedule counts, not only the stored state). */
export function groupOf(e: Pick<EventInfo, 'effective_state'>): Exclude<EventGroup, 'all'> {
  switch (e.effective_state) {
    case 'live':
    case 'paused':
      return 'live'
    case 'registration_open':
      return 'upcoming'
    case 'draft':
      return 'draft'
    default:
      return 'finished'
  }
}

type Line = Pick<EventInfo, 'effective_state' | 'name' | 'slug'>

/** The events of a group whose name or address contains the text, live ones first and then newest as given. */
export function filterEvents<T extends Line>(events: T[], group: EventGroup, text: string): T[] {
  const needle = text.trim().toLowerCase()
  const rank = { live: 0, upcoming: 1, draft: 2, finished: 3 } as const
  return events
    .filter((e) => (group === 'all' || groupOf(e) === group) && (needle === '' || e.name.toLowerCase().includes(needle) || e.slug.toLowerCase().includes(needle)))
    .map((e, i) => ({ e, i }))
    .sort((a, b) => rank[groupOf(a.e)] - rank[groupOf(b.e)] || a.i - b.i)
    .map(({ e }) => e)
}

export function countByGroup(events: Pick<EventInfo, 'effective_state'>[]): Record<EventGroup, number> {
  const out: Record<EventGroup, number> = { all: events.length, live: 0, upcoming: 0, draft: 0, finished: 0 }
  for (const e of events) out[groupOf(e)] += 1
  return out
}
