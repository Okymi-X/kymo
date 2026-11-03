import { useState } from 'react'
import { useEvents } from '../api/platformHooks'
import { EventCard } from '../components/platform/EventCard'
import { Empty, Loading } from '../components/platform/bits'
import { PageTitle, Segmented } from '../components/ui'
import { usePageTitle } from '../lib/usePageTitle'

type Filter = 'all' | 'jeopardy' | 'tournament'


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
