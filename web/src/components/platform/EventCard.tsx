import { ArrowUpRight, Swords, Trophy } from 'lucide-react'
import type { ReactNode } from 'react'
import { Link } from 'react-router'
import type { EventInfo } from '../../api/platformTypes'
import { StatePill } from './bits'
import { EventCountdown } from './EventCountdown'

export function eventLink(e: Pick<EventInfo, 'kind' | 'slug'>): string {
  return e.kind === 'jeopardy' ? `/e/${e.slug}` : `/t/${e.slug}`
}

type CardEvent = Pick<EventInfo, 'name' | 'kind' | 'effective_state'> & Partial<Pick<EventInfo, 'mode' | 'starts_at' | 'ends_at' | 'ended_at'>>

/** Every event looks the same wherever it is listed: name and state, what it is, and one line at the
 *  bottom (when it runs by default, or whatever the list needs). Nothing else. */
export function EventCard({ event, to, footer }: { event: CardEvent; to: string; footer?: ReactNode }) {
  const Icon = event.kind === 'jeopardy' ? Swords : Trophy
  const scheduled = event.starts_at !== undefined || event.ends_at !== undefined || event.ended_at !== undefined
  const bottom =
    footer ??
    (scheduled ? <EventCountdown event={{ effective_state: event.effective_state, starts_at: event.starts_at ?? null, ends_at: event.ends_at ?? null, ended_at: event.ended_at ?? null }} inline /> : null)
  return (
    <Link
      to={to}
      className="group relative flex h-full min-h-32 flex-col gap-3 rounded-card border border-line bg-surface p-5 shadow-[var(--shadow-card)] transition-[transform,box-shadow,border-color] hover:border-field-line hover:shadow-[var(--shadow-lift)] focus-visible:outline-2 focus-visible:outline-accent motion-safe:hover:-translate-y-0.5 motion-safe:active:translate-y-0"
    >
      <div className="flex items-start justify-between gap-3">
        <h2 className="line-clamp-2 text-base font-semibold leading-snug tracking-tight">{event.name}</h2>
        <StatePill state={event.effective_state} />
      </div>
      <p className="flex items-center gap-1.5 text-sm text-muted">
        <Icon size={14} aria-hidden="true" />
        {event.kind === 'jeopardy' ? 'Jeopardy' : 'Tournament'}
        {event.mode && ` · ${event.mode === 'team' ? 'Teams' : 'Solo'}`}
      </p>
      <div className="mt-auto flex items-end justify-between gap-3 pt-1">
        {bottom}
        <ArrowUpRight size={16} aria-hidden="true" className="ml-auto shrink-0 text-muted opacity-0 transition-opacity group-hover:opacity-100 group-focus-visible:opacity-100" />
      </div>
    </Link>
  )
}
