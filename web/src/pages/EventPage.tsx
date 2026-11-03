import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { CalendarClock } from 'lucide-react'
import { Link, Navigate, useParams, useSearchParams } from 'react-router'
import { useIsAdmin, useWho } from '../api/client'
import { platform } from '../api/platform'
import { useEvent, useMyTeam } from '../api/platformHooks'
import { isNoDeadline, registrationStatus } from '../lib/platform'
import type { EventInfo } from '../api/platformTypes'
import { ChallengeBoard } from '../components/platform/ChallengeBoard'
import { Empty, Loading, Panel, Pill, StatePill } from '../components/platform/bits'
import { MyProgress } from '../components/platform/MyProgress'
import { MyInstances } from '../components/platform/MyInstances'
import { VpnPanel } from '../components/platform/VpnPanel'
import { Scoreboard } from '../components/platform/Scoreboard'
import { Tabs, type TabItem } from '../components/Tabs'
import { BackLink } from '../components/BackLink'
import { Button, ErrorText, PageTitle } from '../components/ui'
import { usePageTitle } from '../lib/usePageTitle'
import { NotFound } from './NotFound'

type View = 'challenges' | 'scoreboard' | 'progress'

const fmt = (t: number) => new Date(t * 1000).toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' })

function Schedule({ e }: { e: EventInfo }) {
  const rows: [string, number | null][] = [
    ['Registration opens', e.registration_opens],
    ['Registration closes', e.registration_closes],
    ['Starts', e.starts_at],
    ['Ends', e.ends_at],
  ]
  const shown = rows.filter((r): r is [string, number] => r[1] !== null)
  if (shown.length === 0) return null
  return (
    <dl className="flex flex-wrap gap-x-6 gap-y-1 text-sm text-muted">
      {shown.map(([label, t]) => (
        <div key={label} className="flex items-center gap-1.5">
          <CalendarClock size={14} aria-hidden="true" />
          <dt>{label}</dt>
          <dd className="text-ink">{isNoDeadline(t) ? 'No deadline' : fmt(t)}</dd>
        </div>
      ))}
    </dl>
  )
}

function Registration({ event }: { event: EventInfo }) {
  const who = useWho()
  const admin = useIsAdmin()
  const team = useMyTeam().data
  const client = useQueryClient()
  const done = () => void client.invalidateQueries({ queryKey: ['event', event.slug] })
  const join = useMutation({ mutationFn: () => platform.register_for(event.slug), onSuccess: done })
  const leave = useMutation({ mutationFn: () => platform.unregister(event.slug), onSuccess: done })
  const next = `/e/${event.slug}`
  const status = registrationStatus(event, Math.floor(Date.now() / 1000))
  // Only a visitor needs to know whether they could create an account.
  const signUpInfo = useQuery({ queryKey: ['sign-ups-open'], queryFn: platform.signUpsOpen, enabled: !who }).data
  const signUps = signUpInfo?.open

  if (!who) {
    return (
      <Panel className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-col gap-1">
          <p className="font-medium">{status.open ? 'Sign in to register for this event.' : 'Registration is not open for this event.'}</p>
          <p className="text-sm text-muted">
            {!status.open
              ? status.reason
              : signUps === false
                ? `New accounts are made by the organizers. ${signUpInfo?.contact ? `To get one: ${signUpInfo.contact}` : 'If you do not have one yet, ask an organizer.'}`
                : 'No account yet? Create one first, then come back to register.'}
          </p>
        </div>
        <span className="flex gap-2">
          <Link to={`/signin?next=${encodeURIComponent(next)}`} className="inline-flex h-10 items-center rounded-control bg-accent px-3.5 font-medium text-bg hover:opacity-90">
            Sign in
          </Link>
          {status.open && signUps !== false && (
            <Link to={`/signin?mode=register&next=${encodeURIComponent(next)}`} className="inline-flex h-10 items-center rounded-control border border-line px-3.5 font-medium hover:bg-line/40">
              Create account
            </Link>
          )}
        </span>
      </Panel>
    )
  }
  if (event.registered) {
    return (
      <Panel className="flex flex-wrap items-center justify-between gap-3">
        <p>
          <span className="font-medium text-win">You are registered</span>
          {event.mode === 'team' && team ? ` as team ${team.name}.` : '.'}
        </p>
        <Button onClick={() => leave.mutate()} disabled={leave.isPending}>
          Withdraw
        </Button>
        <ErrorText error={leave.error} />
      </Panel>
    )
  }
  if (!status.open) {
    return (
      <Panel>
        <p className="font-medium">Registration is not open for this event.</p>
        <p className="text-sm text-muted">{status.reason}</p>
      </Panel>
    )
  }
  if (event.mode === 'team' && !team) {
    return (
      <Panel className="flex flex-wrap items-center justify-between gap-3">
        <p className="text-muted">This is a team event. Create or join a team first.</p>
        <Link to="/team" className="inline-flex h-10 items-center rounded-control bg-accent px-3.5 font-medium text-bg hover:opacity-90">
          Go to my team
        </Link>
      </Panel>
    )
  }
  return (
    <Panel className="flex flex-wrap items-center justify-between gap-3">
      <p className="text-muted">
        {event.mode === 'team' && team ? `Register your team ${team.name} for this event.` : 'Register to see the challenges and score points.'}
        {admin && ' (Admins can also preview from the admin page.)'}
      </p>
      <Button variant="primary" onClick={() => join.mutate()} disabled={join.isPending}>
        Register
      </Button>
      <ErrorText error={join.error} />
    </Panel>
  )
}

export function EventPage() {
  const { slug = '' } = useParams()
  const q = useEvent(slug)
  const who = useWho()
  const [params, setParams] = useSearchParams()
  usePageTitle(q.data?.name)
  if (q.isPending) return <Loading />
  if (q.error || !q.data) return <NotFound />
  const event = q.data
  if (event.kind === 'tournament') return <Navigate to={`/t/${event.slug}`} replace />

  const tabs: TabItem<View>[] = [
    { id: 'challenges', label: 'Challenges' },
    { id: 'scoreboard', label: 'Scoreboard' },
    ...(event.registered ? [{ id: 'progress' as const, label: 'My progress' }] : []),
  ]
  const requested = params.get('view') as View | null
  const view: View = tabs.some((t) => t.id === requested) ? (requested as View) : 'challenges'
  const live = event.effective_state

  return (
    <div className="flex flex-col gap-6">
      <BackLink to="/events">Events</BackLink>
      <div className="flex flex-col gap-3">
        <div className="flex flex-wrap items-center gap-3">
          <PageTitle>{event.name}</PageTitle>
          <StatePill state={live} />
          <Pill>{event.mode === 'team' ? 'Teams' : 'Solo'}</Pill>
        </div>
        {event.description && <p className="max-w-2xl whitespace-pre-wrap text-muted">{event.description}</p>}
        <Schedule e={event} />
      </div>
      {live !== 'archived' && <Registration event={event} />}
      {event.network_mode === 'vpn' && event.registered && <VpnPanel />}
      {event.registered && <MyInstances slug={event.slug} />}
      <Tabs<View> items={tabs} value={view} label="Event" onChange={(v) => setParams({ view: v }, { replace: true })}>
        {view === 'challenges' &&
          (event.registered ? (
            live === 'draft' || live === 'registration_open' ? (
              <Empty>The challenges open when the event starts.</Empty>
            ) : (
              <ChallengeBoard slug={event.slug} vpn={event.network_mode === 'vpn'} />
            )
          ) : !who ? (
            <Empty>Sign in and register to see the challenges.</Empty>
          ) : registrationStatus(event, Math.floor(Date.now() / 1000)).open ? (
            <Empty>Register for this event to see the challenges.</Empty>
          ) : (
            <Empty>The challenges are only shown to registered players, and registration is closed.</Empty>
          ))}
        {view === 'scoreboard' && <Scoreboard slug={event.slug} />}
        {view === 'progress' && <MyProgress event={event} />}
      </Tabs>
    </div>
  )
}
