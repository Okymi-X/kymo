import { ArrowRight, CalendarClock, Swords, Trophy, Users } from 'lucide-react'
import { Link } from 'react-router'
import type { EventInfo } from '../../api/platformTypes'
import { isNoDeadline } from '../../lib/platform'
import { Pill, StatePill } from './bits'
import { EventCountdown } from './EventCountdown'

export function eventLink(e: Pick<EventInfo, 'kind' | 'slug'>): string {
  return e.kind === 'jeopardy' ? `/e/${e.slug}` : `/t/${e.slug}`
}

function when(e: EventInfo): string | null {
  const fmt = (t: number) => (isNoDeadline(t) ? 'no deadline' : new Date(t * 1000).toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' }))
  if (e.starts_at && e.ends_at) return `${fmt(e.starts_at)} to ${fmt(e.ends_at)}`
  if (e.starts_at) return `Starts ${fmt(e.starts_at)}`
  if (e.ends_at) return `Ends ${fmt(e.ends_at)}`
  return null
}

export function EventCard({ event }: { event: EventInfo }) {
  const schedule = when(event)
  const Icon = event.kind === 'jeopardy' ? Swords : Trophy
  return (
    <Link
      to={eventLink(event)}
      className="group flex flex-col gap-4 rounded-card border border-line bg-surface p-6 shadow-[var(--shadow-card)] transition-[transform,box-shadow] hover:shadow-[var(--shadow-lift)] active:scale-[0.99]"
    >
      <div className="flex items-start justify-between gap-3">
        <h2 className="text-xl font-semibold tracking-tight">{event.name}</h2>
        <StatePill state={event.effective_state} />
      </div>
      {event.description && <p className="line-clamp-3 text-sm text-muted">{event.description}</p>}
      <EventCountdown event={event} compact />
      <div className="mt-auto flex flex-wrap items-center gap-2 text-xs text-muted">
        <Pill>
          <span className="inline-flex items-center gap-1">
            <Icon size={12} aria-hidden="true" /> {event.kind === 'jeopardy' ? 'Jeopardy' : 'Tournament'}
          </span>
        </Pill>
        <Pill>
          <span className="inline-flex items-center gap-1">
            <Users size={12} aria-hidden="true" /> {event.mode === 'team' ? 'Teams' : 'Solo'}
          </span>
        </Pill>
        {schedule && (
          <span className="inline-flex items-center gap-1">
            <CalendarClock size={12} aria-hidden="true" /> {schedule}
          </span>
        )}
      </div>
      <span className="inline-flex items-center gap-1 text-sm font-medium text-accent group-hover:underline">
        View event <ArrowRight size={14} aria-hidden="true" />
      </span>
    </Link>
  )
}
