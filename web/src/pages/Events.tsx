import { useState } from 'react'
import { useEvents } from '../api/platformHooks'
import { EventCard, eventLink } from '../components/platform/EventCard'
import { Empty, Loading } from '../components/platform/bits'
import { PageTitle, Segmented } from '../components/ui'
import { filterEvents } from '../lib/eventFilter'
import { usePageTitle } from '../lib/usePageTitle'

type Filter = 'all' | 'jeopardy' | 'tournament'


export function Events() {
  usePageTitle('Events')
  const events = useEvents()
  const [filter, setFilter] = useState<Filter>('all')
  // Running events first, then the ones that open soon, then the finished ones.
  const shown = filterEvents((events.data ?? []).filter((e) => filter === 'all' || e.kind === filter), 'all', '')
  return (
    <div className="flex flex-col gap-8">
      <div className="flex flex-col gap-3">
        <PageTitle>Events</PageTitle>
        <p className="max-w-xl text-muted">Jeopardy CTFs and tournaments. Open one to register and play.</p>
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
        <Empty>{filter === 'all' ? 'No events yet.' : `No ${filter === 'jeopardy' ? 'Jeopardy' : 'tournament'} events yet.`}</Empty>
      ) : (
        <div className="grid auto-rows-fr gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {shown.map((e) => (
            <EventCard key={e.id} event={e} to={eventLink(e)} />
          ))}
        </div>
      )}
    </div>
  )
}
