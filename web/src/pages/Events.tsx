import { CalendarClock, Swords, Trophy, Users } from 'lucide-react'
import { useState } from 'react'
import { Link } from 'react-router'
import { useEvents } from '../api/platformHooks'
import type { EventInfo } from '../api/platformTypes'
import { Empty, Loading, Pill, StatePill } from '../components/platform/bits'
import { PageTitle, Segmented } from '../components/ui'
import { usePageTitle } from '../lib/usePageTitle'

type Filter = 'all' | 'jeopardy' | 'tournament'

export function eventLink(e: Pick<EventInfo, 'kind' | 'slug'>): string {
  return e.kind === 'jeopardy' ? `/e/${e.slug}` : `/t/${e.slug}`
}

function when(e: EventInfo): string | null {
  const fmt = (t: number) => new Date(t * 1000).toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' })
  if (e.starts_at && e.ends_at) return `${fmt(e.starts_at)} to ${fmt(e.ends_at)}`
  if (e.starts_at) return `Starts ${fmt(e.starts_at)}`
  if (e.ends_at) return `Ends ${fmt(e.ends_at)}`
  return null
}

function EventCard({ event }: { event: EventInfo }) {
  const schedule = when(event)
  const Icon = event.kind === 'jeopardy' ? Swords : Trophy
  return (
    <Link
      to={eventLink(event)}
      className="group flex flex-col gap-3 rounded-card border border-line bg-surface p-5 shadow-[var(--shadow-card)] transition-[transform,box-shadow] hover:shadow-[var(--shadow-lift)] active:scale-[0.99]"
    >
      <div className="flex items-start justify-between gap-3">
        <h2 className="text-lg font-semibold tracking-tight">{event.name}</h2>
        <StatePill state={event.effective_state} />
      </div>
      {event.description && (
        <div className="flex flex-col gap-1">
          <p className="line-clamp-3 text-sm text-muted">{event.description}</p>
          {/* The whole card opens the event page, where the full text is. Say so when it was cut. */}
          {event.description.length > 140 && <span className="text-xs font-medium text-accent group-hover:underline">Read more</span>}
        </div>
      )}
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
    </Link>
  )
}

export function Events() {
  usePageTitle('Events')
  const events = useEvents()
  const [filter, setFilter] = useState<Filter>('all')
  const shown = (events.data ?? []).filter((e) => filter === 'all' || e.kind === filter)
  return (
    <div className="flex flex-col gap-8">
      <div className="flex flex-col gap-3">
        <PageTitle>Events</PageTitle>
        <p className="max-w-xl text-muted">Jeopardy CTFs and tournaments. Sign in to register and play.</p>
      </div>
      <Segmented<Filter>
        label="Kind"
        value={filter}
        onChange={setFilter}
        options={[
          { value: 'all', label: 'All' },
          { value: 'jeopardy', label: 'Jeopardy' },
          { value: 'tournament', label: 'Tournaments' },
        ]}
      />
      {events.isPending ? (
        <Loading />
      ) : shown.length === 0 ? (
        <Empty>No events yet.</Empty>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {shown.map((e) => (
            <EventCard key={e.id} event={e} />
          ))}
        </div>
      )}
    </div>
  )
}
