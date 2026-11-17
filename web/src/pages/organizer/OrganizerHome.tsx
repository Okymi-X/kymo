import { useQuery } from '@tanstack/react-query'
import { Link } from 'react-router'
import { useOpenAlerts } from '../../components/monitor/useOpenAlerts'
import { platform } from '../../api/platform'
import { useMyPermissions } from '../../api/platformHooks'
import { Empty, Loading, Panel, StatePill, Stat } from '../../components/platform/bits'
import { PageTitle } from '../../components/ui'
import { ORG, ORG_LINKS, orgEvent, orgEvents } from '../../lib/organizer'
import { usePageTitle } from '../../lib/usePageTitle'

/** The console's first page: what is running, what is next, and where each tool is. */
export function OrganizerHome() {
  usePageTitle('Organizer')
  const mine = useMyPermissions()
  const granted = new Set(mine.data?.permissions ?? [])
  const events = useQuery({ queryKey: ['admin-events'], queryFn: platform.adminEvents, enabled: granted.has('manage_events') })
  const list = events.data ?? []
  const live = list.filter((e) => e.effective_state === 'live')
  const upcoming = list.filter((e) => e.effective_state === 'registration_open')
  const drafts = list.filter((e) => e.state === 'draft')
  const shown = [...live, ...upcoming, ...drafts].slice(0, 8)
  const more = live.length + upcoming.length + drafts.length - shown.length
  const links = ORG_LINKS.filter((l) => granted.has(l.needs))
  const alerts = useOpenAlerts(granted.has('monitor'))
  const labs = useQuery({ queryKey: ['labs'], queryFn: platform.labs, enabled: granted.has('manage_settings') })
  const brokenLabs = (labs.data?.labs ?? []).filter((l) => l.enabled && l.last_error !== null)
  return (
    <div className="flex flex-col gap-6">
      <PageTitle>Organizer</PageTitle>
      {granted.has('manage_events') ? (
        <>
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
            <Stat label="Live now" value={live.length} tone={live.length > 0 ? 'text-win' : undefined} />
            <Stat label="Coming up" value={upcoming.length} />
            <Stat label="Drafts" value={drafts.length} />
            <Stat label="All events" value={list.length} />
          </div>
          {(alerts > 0 || brokenLabs.length > 0) && (
            <Panel className="flex flex-col gap-2 border-danger/50">
              <h2 className="text-sm font-semibold">Needs attention</h2>
              {alerts > 0 && (
                <Link to={`${ORG}/monitor`} className="text-sm font-medium text-accent hover:underline">
                  {alerts} open {alerts === 1 ? 'alert' : 'alerts'} about players in the monitor
                </Link>
              )}
              {brokenLabs.length > 0 && (
                <Link to={`${ORG}/labs`} className="text-sm font-medium text-accent hover:underline">
                  {brokenLabs.map((l) => l.name).join(', ')} {brokenLabs.length === 1 ? 'is' : 'are'} not reachable
                </Link>
              )}
            </Panel>
          )}
          <Panel className="flex flex-col gap-3">
            <div className="flex items-center justify-between">
              <h2 className="text-sm font-semibold">Running and coming up</h2>
              <Link to={orgEvents} className="text-sm font-medium text-accent hover:underline">
                Manage events
              </Link>
            </div>
            {events.isPending ? (
              <Loading />
            ) : shown.length === 0 ? (
              <Empty>No events yet. Create one under Events.</Empty>
            ) : (
              <ul className="divide-y divide-line">
                {shown.map((e) => (
                  <li key={e.id} className="flex flex-wrap items-center gap-3 py-2.5">
                    <Link to={orgEvent(e.id)} className="min-w-0 flex-1 truncate font-medium text-accent hover:underline">
                      {e.name}
                    </Link>
                    <span className="text-xs text-muted">{e.kind === 'jeopardy' ? 'Jeopardy' : 'Tournament'}</span>
                    <StatePill state={e.effective_state} />
                  </li>
                ))}
              </ul>
            )}
            {more > 0 && <p className="text-xs text-muted">and {more} more under Manage events.</p>}
          </Panel>
        </>
      ) : (
        <p className="text-sm text-muted">Your role has access to the tools on the left.</p>
      )}
      {links.length === 0 && !mine.isPending && <Empty>Your role has no organizer tools. Ask an admin to change what it may do.</Empty>}
    </div>
  )
}
