import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Download, ExternalLink, Plus, X } from 'lucide-react'
import { useEffect, useRef, useState, type FormEvent } from 'react'
import { Link, useNavigate, useParams, useSearchParams } from 'react-router'
import { platform } from '../../api/platform'
import type { AdminChallenge, Candidate, EventInfo, JeopardySettings, Participant, RegistrationPolicy, SubmissionLine } from '../../api/platformTypes'
import { BackLink } from '../../components/BackLink'
import { ConfirmDialog } from '../../components/ConfirmDialog'
import { DataTable, Empty, Gate, Loading, Panel, Pill, StatePill, TD, Toggle } from '../../components/platform/bits'
import { Pager } from '../../components/Pager'
import { Tabs, type TabItem } from '../../components/Tabs'
import { Button, ErrorText, Field, Input, PageTitle, Select, StatusPill, Textarea } from '../../components/ui'
import { fromLocalInput, scoringLabel, stateLabel, toLocalInput, verdictText, labLabel } from '../../lib/platform'
import { timeAgo } from '../../lib/time'
import { usePageTitle } from '../../lib/usePageTitle'
import { useTournament } from '../../api/hooks'
import { RosterEditor } from '../../components/RosterEditor'
import { TournamentAdmin } from '../../components/TournamentAdmin'
import { formatLabel } from '../../lib/labels'
import { ChallengeEditor } from './ChallengeEditor'
import { challengeType } from '../../lib/delivery'
import { StateStepper } from '../../components/platform/StateStepper'
import { movesFrom, statusLine } from '../../lib/lifecycle'
import { canGoLive, readiness, type Check } from '../../lib/readiness'
import { InstanceDetails } from './InstanceDetails'
import { IntegrityTab } from './Integrity'
import { RulesEditor } from '../../components/platform/RulesEditor'
import { TournamentChallengeSettings } from './TournamentChallenges'

type View = 'overview' | 'settings' | 'tournament' | 'challenges' | 'participants' | 'submissions' | 'integrity' | 'infrastructure' | 'instances'

const TIE_BREAKS: { id: JeopardySettings['tie_breaks'][number]; label: string }[] = [
  { id: 'earliest_last_solve', label: 'Reached the score first' },
  { id: 'most_solves', label: 'More solved challenges' },
  { id: 'first_blood', label: 'More first bloods' },
]

/** What is still missing before the event can go live, each with the tab that fixes it. */
function Readiness({ checks, loading, onOpen }: { checks: Check[]; loading: boolean; onOpen: (view: View) => void }) {
  const ready = canGoLive(checks)
  const tone = { ok: 'text-win', warn: 'text-accent', block: 'text-danger' } as const
  const mark = { ok: 'OK', warn: 'Check', block: 'Needed' } as const
  return (
    <Panel className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-3">
        <h2 className="text-lg font-semibold tracking-tight">Before it goes live</h2>
        {!loading && <Pill tone={ready ? 'text-win' : 'text-danger'}>{ready ? 'Ready' : 'Not ready'}</Pill>}
      </div>
      {loading ? (
        <Loading />
      ) : (
        <ul className="flex flex-col divide-y divide-line">
          {checks.map((c) => (
            <li key={c.id} className="flex flex-wrap items-center gap-3 py-2 text-sm">
              <span className={`w-14 shrink-0 text-xs font-semibold ${tone[c.level]}`}>{mark[c.level]}</span>
              <span className="min-w-0 flex-1">{c.text}</span>
              {c.level !== 'ok' && (
                <button type="button" onClick={() => onOpen(c.view)} className="text-sm font-medium text-accent hover:underline">
                  Fix
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
    </Panel>
  )
}

/** What an organizer wants to know about a running event, and the places to act on it. */
function AtAGlance({ event, onOpen }: { event: EventInfo; onOpen: (view: View) => void }) {
  const where = event.lab_host_id === null ? 'This server' : 'Lab host'
  const tile = (label: string, value: string) => (
    <div className="flex flex-col gap-0.5">
      <dt className="text-xs text-muted">{label}</dt>
      <dd className="text-sm font-medium">{value}</dd>
    </div>
  )
  return (
    <Panel className="flex flex-col gap-4">
      <dl className="grid gap-4 sm:grid-cols-3">
        {tile('Scoreboard', event.scoreboard_public ? 'Public' : 'Players only')}
        {tile('Instances', event.kind === 'jeopardy' ? `${where} · ${event.network_mode === 'vpn' ? 'VPN' : 'published ports'}` : 'Per match')}
        {tile('Scoring', event.mode === 'team' ? 'Teams' : 'Solo')}
      </dl>
      <div className="-ml-3.5 flex flex-wrap gap-1">
        {event.kind === 'jeopardy' && (
          <Button variant="ghost" onClick={() => onOpen('submissions')}>
            Submissions
          </Button>
        )}
        <Button variant="ghost" onClick={() => onOpen('participants')}>
          Participants
        </Button>
        <Button variant="ghost" onClick={() => onOpen('instances')}>
          Instances
        </Button>
      </div>
    </Panel>
  )
}

function Overview({ event, onOpen }: { event: EventInfo; onOpen: (view: View) => void }) {
  const client = useQueryClient()
  const navigate = useNavigate()
  const done = () => {
    void client.invalidateQueries({ queryKey: ['admin-event', event.id] })
    void client.invalidateQueries({ queryKey: ['admin-events'] })
  }
  const move = useMutation({ mutationFn: (state: string) => platform.setEventState(event.id, state), onSuccess: done })
  const regSwitch = useMutation({ mutationFn: (closed: boolean) => platform.setRegistrationClosed(event.id, closed), onSuccess: done })
  const [removing, setRemoving] = useState(false)
  const remove = useMutation({
    mutationFn: () => platform.deleteEvent(event.id),
    onSuccess: () => {
      done()
      navigate('/organizer/events')
    },
  })
  const exportBundle = useMutation({ mutationFn: () => platform.exportEvent(event.id, event.slug) })
  const exportBoard = useMutation({ mutationFn: () => platform.exportScoreboard(event.id, event.slug) })
  const [now] = useState(() => Date.now())
  const cats = useQuery({ queryKey: ['admin-categories', event.id], queryFn: () => platform.categories(event.id), enabled: event.kind === 'jeopardy' })
  const list = useQuery({ queryKey: ['admin-challenges', event.id], queryFn: () => platform.adminChallenges(event.id), enabled: event.kind === 'jeopardy' })
  const checks = readiness({ event, categories: cats.data?.length ?? 0, challenges: list.data ?? [] })
  const loadingChecks = event.kind === 'jeopardy' && (cats.isPending || list.isPending)
  return (
    <div className="flex flex-col gap-6">
      <Panel className="flex flex-col gap-5">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1">
            <span className="text-sm font-medium">{statusLine(event, now)}</span>
            {event.effective_state !== event.state && <span className="text-xs text-muted">acting as {stateLabel[event.effective_state].toLowerCase()} by its schedule</span>}
            {event.started_early && <Pill tone="text-accent">Started early</Pill>}
          </div>
          <div className="flex flex-wrap items-center gap-2">
            {movesFrom(event.state).map((m) => (
              <Button key={m.to} variant={m.tone === 'primary' ? 'primary' : m.tone === 'danger' ? 'danger' : undefined} onClick={() => move.mutate(m.to)} disabled={move.isPending}>
                {m.label}
              </Button>
            ))}
          </div>
        </div>
        <StateStepper state={event.state} />
        <div className="flex flex-wrap items-center justify-between gap-3 border-t border-line pt-4 text-sm">
          {event.state === 'draft' || event.state === 'archived' ? <span /> : (
          <span className="flex items-center gap-3">
            <span className="text-muted">Registration</span>
            <span className={event.registration_open ? 'font-medium text-win' : 'font-medium'}>{event.registration_open ? 'Open' : (event.registration_note ?? 'Closed')}</span>
            <Button variant="ghost" onClick={() => regSwitch.mutate(!event.registration_closed)} disabled={regSwitch.isPending}>
              {event.registration_closed ? 'Reopen' : 'Close now'}
            </Button>
          </span>
          )}
          {(event.state === 'draft' || event.state === 'archived') && (
            <button type="button" className="text-sm text-danger hover:underline" onClick={() => setRemoving(true)}>
              Delete event
            </button>
          )}
        </div>
        <ErrorText error={move.error ?? remove.error ?? exportBundle.error} />
      </Panel>

      {(event.state === 'draft' || event.state === 'registration_open') && <Readiness checks={checks} loading={loadingChecks} onOpen={onOpen} />}
      {(event.state === 'live' || event.state === 'paused') && <AtAGlance event={event} onOpen={onOpen} />}
      {event.kind === 'jeopardy' && (
        <div className="flex flex-wrap items-center gap-1 text-sm text-muted">
          <span>Export</span>
          <Button variant="ghost" onClick={() => exportBundle.mutate()} disabled={exportBundle.isPending}>
            <Download size={14} aria-hidden="true" /> Event bundle
          </Button>
          <Button variant="ghost" onClick={() => exportBoard.mutate()} disabled={exportBoard.isPending}>
            <Download size={14} aria-hidden="true" /> Scoreboard CSV
          </Button>
        </div>
      )}

      {removing && (
        <ConfirmDialog
          title={`Delete ${event.name}?`}
          message="This removes the event with its challenges, submissions and registrations. It cannot be undone."
          confirmLabel="Delete"
          danger
          pending={remove.isPending}
          onConfirm={() => remove.mutate()}
          onClose={() => setRemoving(false)}
        />
      )}
    </div>
  )
}


function Settings({ event }: { event: EventInfo }) {
  const client = useQueryClient()
  const done = () => {
    void client.invalidateQueries({ queryKey: ['admin-event', event.id] })
    void client.invalidateQueries({ queryKey: ['admin-events'] })
  }
  const [f, setF] = useState(() => ({
    name: event.name,
    description: event.description,
    mode: event.mode,
    opens: toLocalInput(event.registration_opens),
    closes: toLocalInput(event.registration_closes),
    policy: event.registration_policy,
    starts: toLocalInput(event.starts_at),
    ends: toLocalInput(event.ends_at),
    min: String(event.team_min),
    max: String(event.team_max),
    invites: event.allow_invites,
    locked: event.roster_locked,
  }))
  const save = useMutation({
    mutationFn: () =>
      platform.updateEvent(event.id, {
        name: f.name.trim(),
        description: f.description,
        mode: f.mode,
        registration_opens: fromLocalInput(f.opens),
        registration_closes: f.policy === 'at_time' ? fromLocalInput(f.closes) : null,
        registration_policy: f.policy,
        starts_at: fromLocalInput(f.starts),
        ends_at: fromLocalInput(f.ends),
        team_min: Math.trunc(Number(f.min)),
        team_max: Math.trunc(Number(f.max)),
        allow_invites: f.invites,
        roster_locked: f.locked,
      }),
    onSuccess: done,
  })
  // The toggle follows the click at once and returns to what the server says when the request ends.
  const [boardLocal, setBoardLocal] = useState<boolean | null>(null)
  const board = useMutation({
    mutationFn: (v: boolean) => platform.setScoreboardPublic(event.id, v),
    onSettled: async () => {
      await client.invalidateQueries({ queryKey: ['admin-event', event.id] })
      setBoardLocal(null)
    },
  })
  const submit = (e: FormEvent) => {
    e.preventDefault()
    save.mutate()
  }
  const teams = f.mode === 'team'
  return (
    <div className="flex flex-col gap-6">
      <Panel>
        <form onSubmit={submit} className="flex flex-col gap-4">
          <h2 className="text-lg font-semibold tracking-tight">Details</h2>
          <div className="grid gap-4 sm:grid-cols-2">
            <Field label="Name">
              <Input value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} maxLength={80} />
            </Field>
            <Field label="Players">
              <Select value={f.mode} onChange={(e) => setF({ ...f, mode: e.target.value as 'solo' | 'team' })}>
                <option value="team">Teams</option>
                <option value="solo">Solo</option>
              </Select>
            </Field>
          </div>
          <Field label="Description">
            <Textarea value={f.description} onChange={(e) => setF({ ...f, description: e.target.value })} maxLength={2000} />
          </Field>
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
            <Field label="Registration opens">
              <Input type="datetime-local" value={f.opens} onChange={(e) => setF({ ...f, opens: e.target.value })} />
            </Field>
            <Field label="Registration closes" hint={f.policy === 'at_time' ? undefined : 'Set by the policy below.'}>
              <Select value={f.policy} onChange={(e) => setF({ ...f, policy: e.target.value as RegistrationPolicy })}>
                <option value="at_start">When the event starts</option>
                <option value="at_time">At a chosen time</option>
                <option value="manual">When I close it</option>
              </Select>
            </Field>
            <Field label="Starts">
              <Input type="datetime-local" value={f.starts} onChange={(e) => setF({ ...f, starts: e.target.value })} />
            </Field>
            <Field label="Ends">
              <Input type="datetime-local" value={f.ends} onChange={(e) => setF({ ...f, ends: e.target.value })} />
            </Field>
          </div>
          {f.policy === 'at_time' && (
            <Field label="Closing time" hint="Registration stays open after the event starts, until this moment.">
              <Input type="datetime-local" value={f.closes} onChange={(e) => setF({ ...f, closes: e.target.value })} className="max-w-xs" />
            </Field>
          )}
          {teams && (
            <div className="flex flex-wrap items-end gap-4">
              <Field label="Smallest team">
                <Input type="number" min={1} value={f.min} onChange={(e) => setF({ ...f, min: e.target.value })} className="w-24 font-mono" />
              </Field>
              <Field label="Largest team">
                <Input type="number" min={1} value={f.max} onChange={(e) => setF({ ...f, max: e.target.value })} className="w-24 font-mono" />
              </Field>
              <Toggle label="Members may invite" checked={f.invites} onChange={(v) => setF({ ...f, invites: v })} />
              <Toggle label="Rosters lock when the event starts" checked={f.locked} onChange={(v) => setF({ ...f, locked: v })} />
            </div>
          )}
          <p className="text-sm text-muted">Times use your browser's time zone. The event goes live and ends by itself at the start and end times unless it is paused.</p>
          <ErrorText error={save.error} />
          <div className="flex items-center gap-3">
            <Button type="submit" variant="primary" disabled={!f.name.trim() || save.isPending}>
              Save details
            </Button>
            {save.isSuccess && <span className="text-sm text-win">Saved</span>}
          </div>
        </form>
      </Panel>

      <Panel className="flex flex-col gap-3">
        <h2 className="text-lg font-semibold tracking-tight">Public scoreboard</h2>
        <p className="text-sm text-muted">When off, only registered players and staff can see the scoreboard.</p>
        <Toggle
          label="Anyone can see the scoreboard"
          checked={boardLocal ?? event.scoreboard_public}
          onChange={(v) => {
            setBoardLocal(v)
            board.mutate(v)
          }}
        />
        <ErrorText error={board.error} />
      </Panel>

      <RulesEditor eventId={event.id} slug={event.slug} />

      {event.kind === 'tournament' && <TournamentChallengeSettings event={event} />}


    </div>
  )
}

function Infrastructure({ event }: { event: EventInfo }) {
  return (
    <div className="flex flex-col gap-6">
      <InstanceHost event={event} />

      <NetworkMode event={event} />

      <MaxStarts event={event} />

      {event.kind === 'jeopardy' && <JeopardySettingsForm eventId={event.id} />}
    </div>
  )
}

/** The default start budget for this event's challenges; works the same for a tournament or a Jeopardy event. */
function MaxStarts({ event }: { event: EventInfo }) {
  const client = useQueryClient()
  const [local, setLocal] = useState('')
  const set = useMutation({
    mutationFn: (value: number) => platform.setMaxStarts(event.id, value),
    onSuccess: async () => {
      await client.invalidateQueries({ queryKey: ['admin-event', event.id] })
      setLocal('')
    },
  })
  return (
    <Panel className="flex flex-col gap-3">
      <h2 className="text-lg font-semibold tracking-tight">Instance start limit</h2>
      <Field label="Starts plus restarts allowed, by default" hint="A challenge can set its own value instead, under its Delivery settings. Applies the same way for a tournament or a Jeopardy event.">
        <span className="flex gap-2">
          <Input
            type="number"
            min={1}
            max={1000}
            value={local === '' ? event.max_starts : local}
            onChange={(e) => setLocal(e.target.value)}
            className="max-w-xs font-mono"
          />
          {local !== '' && Number(local) !== event.max_starts && (
            <Button type="button" variant="primary" disabled={set.isPending} onClick={() => set.mutate(Math.trunc(Number(local)))}>
              Save
            </Button>
          )}
        </span>
      </Field>
      <ErrorText error={set.error} />
    </Panel>
  )
}

/** Where this event's instances run: this server, or one of the lab hosts. */
function InstanceHost({ event }: { event: EventInfo }) {
  const client = useQueryClient()
  const labs = useQuery({ queryKey: ['labs'], queryFn: platform.labs })
  const vpn = useQuery({ queryKey: ['admin-vpn'], queryFn: platform.adminVpn })
  const [local, setLocal] = useState<string | null>(null)
  const list = labs.data?.labs ?? []
  const set = useMutation({
    mutationFn: (value: string) => {
      const target = value === '' ? null : list.find((l) => l.id === Number(value))
      // The mode goes with the host: a lab host that only runs instances through its VPN needs
      // the event on the VPN, and a place with no gateway cannot have it.
      const hasGateway = target ? target.vpn !== null : (vpn.data?.local ?? false)
      const mode = target?.vpn?.only ? 'vpn' : event.network_mode === 'vpn' && !hasGateway ? 'published' : undefined
      return platform.setEventLab(event.id, target ? target.id : null, mode)
    },
    onSettled: async () => {
      await client.invalidateQueries({ queryKey: ['admin-event', event.id] })
      void client.invalidateQueries({ queryKey: ['labs'] })
      setLocal(null)
    },
  })
  const current = local ?? (event.lab_host_id === null ? '' : String(event.lab_host_id))
  return (
    <Panel className="flex flex-col gap-3">
      <h2 className="text-lg font-semibold tracking-tight">Where instances run</h2>
      <p className="text-sm text-muted">
        This server or a lab host runs the instances. With the private VPN, players reach the lab host's gateway and nothing is published.{' '}
        {list.length === 0 && (
          <>
            There is no lab host yet: <Link to="/organizer/labs" className="font-medium text-accent hover:underline">add one</Link>.
          </>
        )}
      </p>
      <Select aria-label="Where instances run" value={current} disabled={set.isPending} className="w-72" onChange={(e) => {
        setLocal(e.target.value)
        set.mutate(e.target.value)
      }}>
        <option value="">This server</option>
        {list.map((l) => (
          <option key={l.id} value={l.id} disabled={!l.enabled && event.lab_host_id !== l.id}>
            {labLabel(l)}
          </option>
        ))}
      </Select>
      <p className="text-xs text-muted">Stop all running instances before moving the event. Instances that are already running stay where they started.</p>
      <ErrorText error={set.error} />
    </Panel>
  )
}

function NetworkMode({ event }: { event: EventInfo }) {
  const vpn = useQuery({ queryKey: ['admin-vpn'], queryFn: platform.adminVpn })
  const labs = useQuery({ queryKey: ['labs'], queryFn: platform.labs })
  const client = useQueryClient()
  const [local, setLocal] = useState<'published' | 'vpn' | null>(null)
  const set = useMutation({
    mutationFn: (mode: 'published' | 'vpn') => platform.setNetworkMode(event.id, mode),
    onSettled: async () => {
      await client.invalidateQueries({ queryKey: ['admin-event', event.id] })
      setLocal(null)
    },
  })
  const lab = event.lab_host_id === null ? null : (labs.data?.labs ?? []).find((l) => l.id === event.lab_host_id) ?? null
  // The gateway players would connect to: the lab host's own, or this server's.
  const gateway = lab ? lab.vpn : vpn.data?.enabled && vpn.data.local ? { host: vpn.data.host ?? '', port: vpn.data.port ?? 1194, only: false } : null
  // A lab host's gateway is what that host reported; this server's own VPN setting says nothing about it.
  const enabled = lab ? true : (vpn.data?.enabled ?? false)
  const hasGateway = enabled && gateway !== null && gateway.host !== '' && gateway.host !== '-'
  // Reachability is a separate state from having no gateway: say which one it is.
  const down = lab !== null && lab.enabled && (lab.last_error !== null || lab.last_ok_at === null)
  const loopback = hasGateway && /^(127\.|localhost$|::1$)/.test(gateway?.host ?? '')
  const publishedAllowed = !gateway?.only
  return (
    <Panel className="flex flex-col gap-3">
      <h2 className="text-lg font-semibold tracking-tight">Instance network</h2>
      {loopback && event.network_mode === 'vpn' && (
        <p role="alert" className="rounded-control border border-danger/40 bg-danger/10 p-3 text-sm text-danger">
          Players cannot connect: the VPN address is {gateway?.host}, which only works on the machine itself. Set the VPN host ({lab ? <code className="font-mono">KYMO_LAB_VPN_HOST</code> : <code className="font-mono">KYMO_VPN_HOST</code>}) to its public address or name and restart, then have players download a new profile.
        </p>
      )}
      {event.network_mode === 'vpn' && !hasGateway && (
        <p role="alert" className="rounded-control border border-danger/40 bg-danger/10 p-3 text-sm text-danger">
          {lab ? `The lab host ${lab.name} has no VPN gateway.` : 'This server has no VPN gateway of its own.'} Players cannot reach instances until you pick a host with one.
        </p>
      )}
      {event.network_mode === 'vpn' && hasGateway && down && lab && (
        <p role="alert" className="rounded-control border border-danger/40 bg-danger/10 p-3 text-sm text-danger">
          The VPN gateway is set up at {gateway?.host}:{gateway?.port}, but the lab host {lab.name} is not answering right now{lab.last_error ? ` (${lab.last_error})` : ''}. Check that it is running; players can connect again once it is.
        </p>
      )}
      <p className="text-sm text-muted">
        <strong>Published ports</strong>: each instance gets a port on the machine that runs it; anyone who can reach that machine can reach it. <strong>Private VPN</strong>: each player or team gets an isolated network and connects through their own OpenVPN profile; nothing is published, and instances cannot see each other.
      </p>
      <div className="flex flex-wrap items-center gap-3">
        <Select
          aria-label="Instance network"
          value={local ?? event.network_mode}
          onChange={(e) => {
            setLocal(e.target.value as 'published' | 'vpn')
            set.mutate(e.target.value as 'published' | 'vpn')
          }}
          disabled={set.isPending}
          className="w-72"
        >
          <option value="published" disabled={!publishedAllowed && event.network_mode !== 'published'}>
            Published ports{publishedAllowed ? '' : ' (this lab host refuses them)'}
          </option>
          <option value="vpn" disabled={!hasGateway && event.network_mode !== 'vpn'}>
            Private VPN{hasGateway ? '' : lab ? ' (this lab host has no gateway)' : !enabled ? ' (not enabled on this server)' : ' (no gateway on this server)'}
          </option>
        </Select>
        {event.network_mode === 'vpn' && gateway && <span className="text-sm text-muted">Players connect to <code className="font-mono text-ink">{gateway.host}:{gateway.port}</code> (UDP)</span>}
      </div>
      <p className="text-xs text-muted">Stop all running instances before switching. Players need to download their VPN profile from the event page.</p>
      <ErrorText error={set.error} />
    </Panel>
  )
}

function JeopardySettingsForm({ eventId }: { eventId: number }) {
  const q = useQuery({ queryKey: ['jeopardy-settings', eventId], queryFn: () => platform.jeopardySettings(eventId) })
  const [s, setS] = useState<JeopardySettings | null>(null)
  useEffect(() => {
    if (q.data) setS(q.data)
  }, [q.data])
  const client = useQueryClient()
  const save = useMutation({
    mutationFn: (body: JeopardySettings) => platform.saveJeopardySettings(eventId, body),
    onSuccess: () => void client.invalidateQueries({ queryKey: ['jeopardy-settings', eventId] }),
  })
  if (!s) return <Loading />
  const toggle = (id: JeopardySettings['tie_breaks'][number], on: boolean) =>
    setS({ ...s, tie_breaks: on ? [...s.tie_breaks, id] : s.tie_breaks.filter((t) => t !== id) })
  return (
    <Panel>
      <form
        className="flex flex-col gap-4"
        onSubmit={(e) => {
          e.preventDefault()
          save.mutate(s)
        }}
      >
        <h2 className="text-lg font-semibold tracking-tight">Scoring and submissions</h2>
        <fieldset className="flex flex-col gap-2">
          <legend className="mb-1 text-xs font-medium text-ink/70">Tie-breakers, in the order you tick them</legend>
          {TIE_BREAKS.map((t) => (
            <Toggle key={t.id} label={`${t.label}${s.tie_breaks.includes(t.id) ? ` (${s.tie_breaks.indexOf(t.id) + 1})` : ''}`} checked={s.tie_breaks.includes(t.id)} onChange={(v) => toggle(t.id, v)} />
          ))}
        </fieldset>
        <div className="grid gap-4 sm:grid-cols-3">
          <Field label="Flag attempts allowed">
            <Input type="number" min={1} value={s.rate_max_attempts} onChange={(e) => setS({ ...s, rate_max_attempts: Math.trunc(Number(e.target.value)) })} className="font-mono" />
          </Field>
          <Field label="Per (seconds)">
            <Input type="number" min={1} value={s.rate_window_secs} onChange={(e) => setS({ ...s, rate_window_secs: Math.trunc(Number(e.target.value)) })} className="font-mono" />
          </Field>
          <Field label="Lock out for (seconds)">
            <Input type="number" min={0} value={s.rate_lockout_secs} onChange={(e) => setS({ ...s, rate_lockout_secs: Math.trunc(Number(e.target.value)) })} className="font-mono" />
          </Field>
        </div>
        <Field label="Freeze the public scoreboard at" hint="From then on the public board stays as it was until the event ends. Staff still see it live. Empty: no freeze.">
          <span className="flex gap-2">
            <Input type="datetime-local" value={toLocalInput(s.freeze_at)} onChange={(e) => setS({ ...s, freeze_at: fromLocalInput(e.target.value) })} className="max-w-xs" />
            {s.freeze_at !== null && (
              <Button variant="ghost" onClick={() => setS({ ...s, freeze_at: null })}>
                Clear
              </Button>
            )}
          </span>
        </Field>
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="When a player submits another player's flag" hint="Applies to dynamic flags, which differ for every player or team.">
            <Select value={s.sharing_action} onChange={(e) => setS({ ...s, sharing_action: e.target.value as JeopardySettings['sharing_action'] })}>
              <option value="off">Do nothing</option>
              <option value="manual">Warn, then report to the organizers</option>
              <option value="disqualify">Warn, then disqualify from the event</option>
              <option value="auto_ban">Warn, then ban the account</option>
            </Select>
          </Field>
          <Field label="Warnings before the action">
            <Input type="number" min={0} max={10} value={s.sharing_warnings} disabled={s.sharing_action === 'off'} onChange={(e) => setS({ ...s, sharing_warnings: Math.trunc(Number(e.target.value)) })} className="font-mono" />
          </Field>
        </div>
        <ErrorText error={save.error} />
        <div className="flex items-center gap-3">
          <Button type="submit" variant="primary" disabled={save.isPending}>
            Save settings
          </Button>
          {save.isSuccess && <span className="text-sm text-win">Saved</span>}
        </div>
      </form>
    </Panel>
  )
}

function Challenges({ event }: { event: EventInfo }) {
  const client = useQueryClient()
  const cats = useQuery({ queryKey: ['admin-categories', event.id], queryFn: () => platform.categories(event.id) })
  const list = useQuery({ queryKey: ['admin-challenges', event.id], queryFn: () => platform.adminChallenges(event.id) })
  const [name, setName] = useState('')
  const addCat = useMutation({
    mutationFn: () => platform.createCategory(event.id, name.trim()),
    onSuccess: () => {
      setName('')
      void client.invalidateQueries({ queryKey: ['admin-categories', event.id] })
    },
  })
  const delCat = useMutation({
    mutationFn: (id: number) => platform.deleteCategory(id),
    onSuccess: () => void client.invalidateQueries({ queryKey: ['admin-categories', event.id] }),
  })
  const [editing, setEditing] = useState<number | 'new' | null>(null)
  const categories = cats.data ?? []
  const challenges = list.data ?? []
  const current: AdminChallenge | null = typeof editing === 'number' ? (challenges.find((c) => c.id === editing) ?? null) : null
  return (
    <div className="flex flex-col gap-6">
      <Panel className="flex flex-col gap-3">
        <h2 className="text-sm font-semibold">Categories</h2>
        <div className="flex flex-wrap gap-2">
          {categories.map((c) => (
            <span key={c.id} className="inline-flex items-center gap-1 rounded-full border border-line py-1 pl-3 pr-1.5 text-sm">
              {c.name}
              <button type="button" aria-label={`Delete category ${c.name}`} onClick={() => delCat.mutate(c.id)} className="rounded-full p-1 text-muted hover:text-danger">
                <X size={13} aria-hidden="true" />
              </button>
            </span>
          ))}
        </div>
        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault()
            if (name.trim()) addCat.mutate()
          }}
        >
          <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="New category" aria-label="New category" maxLength={60} />
          <Button type="submit" disabled={!name.trim() || addCat.isPending}>
            <Plus size={15} aria-hidden="true" /> Add
          </Button>
        </form>
        <ErrorText error={addCat.error ?? delCat.error} />
      </Panel>

      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold tracking-tight">Challenges ({challenges.length})</h2>
        <Button variant="primary" onClick={() => setEditing('new')} disabled={categories.length === 0}>
          <Plus size={15} aria-hidden="true" /> New challenge
        </Button>
      </div>
      {categories.length === 0 && <p className="text-sm text-muted">Add a category first.</p>}
      {list.isPending ? (
        <Loading />
      ) : challenges.length === 0 ? (
        <Empty>No challenges yet.</Empty>
      ) : (
        <DataTable caption="Challenges" head={event.kind === 'tournament' ? ['Challenge', 'Category', 'Type', 'Points', 'State'] : ['Challenge', 'Category', 'Type', 'Points', 'Solves', 'State']}>
          {challenges.map((c) => (
            <tr key={c.id} className="hover:bg-line/30">
              <td className={TD}>
                <button type="button" onClick={() => setEditing(c.id)} className="text-left font-medium text-accent hover:underline">
                  {c.title}
                </button>
              </td>
              <td className={TD}>{categories.find((k) => k.id === c.category_id)?.name ?? ''}</td>
              <td className={`${TD} text-xs text-muted`}>{challengeType(c)}</td>
              <td className={`${TD} font-mono text-xs`}>{scoringLabel(c.scoring)}</td>
              {event.kind !== 'tournament' && <td className={`${TD} font-mono tabular-nums`}>{c.solves}</td>}
              <td className={TD}>
                <Pill tone={c.state === 'published' ? 'text-win' : 'text-muted'}>{c.state}</Pill>
              </td>
            </tr>
          ))}
        </DataTable>
      )}
      {editing !== null && (editing === 'new' || current) && (
        <ChallengeEditor key={String(editing)} eventId={event.id} eventKind={event.kind} categories={categories} eventLabHostId={event.lab_host_id} challenge={current} onClose={() => setEditing(null)} />
      )}
    </div>
  )
}

function TournamentTab({ event }: { event: EventInfo }) {
  const q = useTournament(event.slug)
  const t = q.data
  const navigate = useNavigate()
  const before = useRef(t?.status)
  // Starting the tournament is the last setup step: go and look at the bracket.
  useEffect(() => {
    if (before.current === 'draft' && t && t.status !== 'draft') navigate(`/t/${t.slug}`)
    before.current = t?.status
  }, [t, navigate])
  if (!t) return <Loading />
  return (
    <div className="flex flex-col gap-6">
      <Panel className="flex flex-col gap-3">
        <div className="flex flex-wrap items-center gap-3">
          <h2 className="text-lg font-semibold tracking-tight">{t.name}</h2>
          <Pill>{formatLabel[t.format]}</Pill>
          <StatusPill status={t.status} paused={t.paused} />
          <span className="text-sm text-muted">{t.teams.length} teams</span>
        </div>
        <TournamentAdmin t={t} />
        <p className="text-sm text-muted">
          Results are entered on the <a href={`/t/${t.slug}`} className="font-medium text-accent hover:underline">tournament page</a>, where players and referees see their matches too.
        </p>
      </Panel>
      {t.status === 'draft' ? <RosterEditor key={`${t.id}-draft`} tournament={t} /> : <p className="text-sm text-muted">The roster is fixed once the tournament has started. Reset the bracket above to change it.</p>}
    </div>
  )
}

function Participants({ event }: { event: EventInfo }) {
  const client = useQueryClient()
  const key = ['participants', event.id]
  const q = useQuery({ queryKey: key, queryFn: () => platform.participants(event.id) })
  const [search, setSearch] = useState('')
  const [removing, setRemoving] = useState<Participant | null>(null)
  const over = event.state === 'ended' || event.state === 'archived'
  const found = useQuery({
    queryKey: ['candidates', event.id, search],
    queryFn: () => platform.candidates(event.id, search),
    enabled: !over && search.trim().length >= 1,
  })
  const refresh = (rows: Participant[]) => {
    client.setQueryData(key, rows)
    void client.invalidateQueries({ queryKey: ['candidates', event.id] })
    void client.invalidateQueries({ queryKey: ['admin-event', event.id] })
  }
  const add = useMutation({
    mutationFn: (c: Candidate) => platform.addParticipant(event.id, event.mode === 'solo' ? { user_id: c.id } : { team_id: c.id }),
    onSuccess: refresh,
  })
  const remove = useMutation({
    mutationFn: (p: Participant) => platform.removeParticipant(event.id, p.id),
    onSuccess: (rows) => {
      setRemoving(null)
      refresh(rows)
    },
  })
  if (q.isPending) return <Loading />
  const rows = q.data ?? []
  const noun = event.mode === 'team' ? 'team' : 'player'
  return (
    <div className="flex flex-col gap-4">
      {over ? (
        <p className="text-sm text-muted">This event is over, so its participants are history and no longer change.</p>
      ) : (
        <Panel className="flex flex-col gap-3">
          <h2 className="text-sm font-semibold">Add a {noun}</h2>
          <p className="text-sm text-muted">
            Works whether registration is open or closed. {event.mode === 'team' ? 'The team keeps its own members; only its registration for this event is added, and it must meet the size rules.' : 'Only the registration is added.'}
          </p>
          <Input value={search} onChange={(e) => setSearch(e.target.value)} placeholder={event.mode === 'team' ? 'Search teams by name' : 'Search players by username'} aria-label={`Search ${noun}s`} />
          {search.trim() !== '' && found.data && found.data.length === 0 && <p className="text-sm text-muted">Nobody matches, or they are already registered.</p>}
          <ul className="flex flex-col gap-1.5">
            {(found.data ?? []).map((c) => (
              <li key={c.id} className="flex items-center gap-3 rounded-control border border-line px-3 py-2 text-sm">
                <span className="min-w-0 flex-1 truncate font-medium">{c.name}</span>
                <span className="text-xs text-muted">{c.detail}</span>
                <Button onClick={() => add.mutate(c)} disabled={add.isPending}>
                  Add
                </Button>
              </li>
            ))}
          </ul>
          <ErrorText error={add.error} />
        </Panel>
      )}
      {rows.length === 0 ? (
        <Empty>Nobody has registered yet.</Empty>
      ) : (
        <DataTable caption="Participants" head={[event.mode === 'team' ? 'Team' : 'Player', 'Members', 'Registered', '']}>
          {rows.map((p) => (
            <tr key={p.id}>
              <td className={`${TD} font-medium`}>
                {p.name}
                {p.disqualified_at !== null && (
                  <span className="ml-2" title={p.disqualified_reason}>
                    <Pill tone="text-danger">Disqualified</Pill>
                  </span>
                )}
              </td>
              <td className={`${TD} font-mono tabular-nums`}>{p.members}</td>
              <td className={`${TD} text-muted`}>{timeAgo(p.registered_at)}</td>
              <td className={TD}>
                {!over && (
                  <Button variant="danger" onClick={() => setRemoving(p)} aria-label={`Remove ${p.name}`}>
                    Remove
                  </Button>
                )}
              </td>
            </tr>
          ))}
        </DataTable>
      )}
      {removing && (
        <ConfirmDialog
          title={`Remove ${removing.name}?`}
          message={`${removing.name} leaves the standings and gets no new challenges, matches or instances, and any running instance is stopped. Their submissions, solves and past results stay on record, and the removal is written to the audit log.`}
          confirmLabel="Remove"
          danger
          pending={remove.isPending}
          onConfirm={() => remove.mutate(removing)}
          onClose={() => setRemoving(null)}
        />
      )}
      <ErrorText error={remove.error} />
    </div>
  )
}

/** A submitted value. A correct flag is the answer itself, so it stays hidden until asked for. */
function SubmittedValue({ eventId, line }: { eventId: number; line: SubmissionLine }) {
  const reveal = useMutation({ mutationFn: () => platform.revealSubmission(eventId, line.id) })
  if (!line.hidden) return <>{line.value}</>
  if (reveal.data) {
    return (
      <span className="flex flex-col items-start gap-1">
        <span>{reveal.data.value}</span>
        <button type="button" onClick={() => reveal.reset()} className="font-sans text-xs font-medium text-accent hover:underline">
          Hide
        </button>
      </span>
    )
  }
  return (
    <span className="flex flex-col items-start gap-1">
      <span className="text-muted" aria-label="Correct flag, hidden">
        ••••••••
      </span>
      <button type="button" onClick={() => reveal.mutate()} disabled={reveal.isPending} className="font-sans text-xs font-medium text-accent hover:underline" title="Shows the correct flag and records that you looked">
        Reveal
      </button>
      <ErrorText error={reveal.error} />
    </span>
  )
}

function Submissions({ event }: { event: EventInfo }) {
  const [page, setPage] = useState(1)
  const [verdict, setVerdict] = useState('')
  const [ip, setIp] = useState('')
  const q = useQuery({
    queryKey: ['submissions', event.id, page, verdict, ip],
    queryFn: () => platform.submissions(event.id, { page, verdict: verdict || undefined, ip: ip.trim() || undefined }),
  })
  const pages = q.data ? Math.max(1, Math.ceil(q.data.total / q.data.per_page)) : 1
  const from = q.data && q.data.total > 0 ? (page - 1) * q.data.per_page + 1 : 0
  const to = q.data ? Math.min(q.data.total, page * q.data.per_page) : 0
  const tone = (v: SubmissionLine['verdict']) => (v === 'correct' ? 'text-win' : v === 'incorrect' ? 'text-danger' : 'text-muted')
  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-end gap-3">
        <Field label="Result">
          <Select
            value={verdict}
            onChange={(e) => {
              setVerdict(e.target.value)
              setPage(1)
            }}
          >
            <option value="">All</option>
            <option value="correct">Correct</option>
            <option value="incorrect">Incorrect</option>
            <option value="bad_format">Wrong format</option>
          </Select>
        </Field>
        <Field label="Address">
          <Input
            value={ip}
            onChange={(e) => {
              setIp(e.target.value)
              setPage(1)
            }}
            placeholder="10.0.0.5"
            className="font-mono"
          />
        </Field>
      </div>
      {q.isPending ? (
        <Loading />
      ) : q.data && q.data.items.length > 0 ? (
        <>
          <DataTable caption="Flag submissions" head={['When', 'Challenge', 'Player', 'Address', 'Submitted', 'Result']}>
            {q.data.items.map((s) => (
              <tr key={s.id}>
                <td className={`${TD} whitespace-nowrap text-muted`} title={new Date(s.at * 1000).toLocaleString()}>
                  {timeAgo(s.at)}
                </td>
                <td className={TD}>{s.challenge_title}</td>
                <td className={TD}>{s.username ?? `#${s.user_id}`}</td>
                <td className={`${TD} font-mono text-xs`}>{s.ip}</td>
                <td className={`${TD} max-w-56 break-all font-mono text-xs`}>
                  <SubmittedValue eventId={event.id} line={s} />
                </td>
                <td className={`${TD} ${tone(s.verdict)}`}>{verdictText[s.verdict]}</td>
              </tr>
            ))}
          </DataTable>
          <Pager page={page} pages={pages} from={from} to={to} total={q.data.total} onChange={setPage} noun="submissions" />
        </>
      ) : (
        <Empty>No submissions match.</Empty>
      )}
      <ErrorText error={q.error} />
    </div>
  )
}

function Instances({ event }: { event: EventInfo }) {
  const client = useQueryClient()
  const q = useQuery({ queryKey: ['admin-instances', event.id], queryFn: () => platform.adminInstances(event.id), refetchInterval: 10_000 })
  const stop = useMutation({
    mutationFn: (id: number) => platform.stopAdminInstance(id),
    onSuccess: () => void client.invalidateQueries({ queryKey: ['admin-instances', event.id] }),
  })
  const [details, setDetails] = useState<number | null>(null)
  const [page, setPage] = useState(1)
  const [stateFilter, setStateFilter] = useState('')
  const [search, setSearch] = useState('')
  if (q.isPending) return <Loading />
  if (q.error) return <ErrorText error={q.error} />
  const all = q.data?.instances ?? []
  if (all.length === 0) return <Empty>No instances have been started.</Empty>
  // A long history of stopped and failed runs is paged and filterable, so a live problem is easy to find.
  const needle = search.trim().toLowerCase()
  const matching = all.filter(
    (r) =>
      (stateFilter === '' || r.state === stateFilter) &&
      (needle === '' || `${r.challenge_title ?? ''} ${r.username ?? ''} ${r.participant.team ?? ''} #${r.id}`.toLowerCase().includes(needle)),
  )
  const states = [...new Set(all.map((r) => r.state))].sort()
  const size = 25
  const pages = Math.max(1, Math.ceil(matching.length / size))
  const at = Math.min(page, pages)
  const rows = matching.slice((at - 1) * size, at * size)
  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-end gap-3">
        <Field label="Search">
          <Input type="search" value={search} onChange={(e) => { setSearch(e.target.value); setPage(1) }} placeholder="Challenge, player, team or #run" className="w-64 max-w-full" />
        </Field>
        <Field label="State">
          <Select value={stateFilter} onChange={(e) => { setStateFilter(e.target.value); setPage(1) }}>
            <option value="">All states</option>
            {states.map((s) => (
              <option key={s} value={s}>
                {s} ({all.filter((r) => r.state === s).length})
              </option>
            ))}
          </Select>
        </Field>
      </div>
      {rows.length === 0 && <Empty>No instances match these filters.</Empty>}
      {rows.length > 0 && (
      <DataTable caption="Instances" head={['Run', 'Challenge', 'Owner', 'State', 'Port', 'Started', 'Expires', '']}>
        {rows.map((r) => (
          <tr key={r.id}>
            <td className={`${TD} font-mono`}>#{r.id}</td>
            <td className={TD}>{r.challenge_title ?? `#${r.challenge_id}`}</td>
            <td className={TD} title={`user id ${r.user_id}`}>
              <span className="font-medium">{r.participant.team ? `Team ${r.participant.team}` : (r.username ?? `user #${r.user_id}`)}</span>
              {r.participant.team && r.username && <span className="block text-xs text-muted">started by {r.username}</span>}
            </td>
            <td className={TD}>{r.state}</td>
            <td className={`${TD} font-mono`}>{r.host_port ?? ''}</td>
            <td className={`${TD} text-muted`}>{timeAgo(r.started_at)}</td>
            <td className={`${TD} text-muted`}>{new Date(r.expires_at * 1000).toLocaleTimeString()}</td>
            <td className={`${TD} flex flex-wrap gap-2`}>
              <Button onClick={() => setDetails(r.id)}>Details</Button>
              {(r.state === 'running' || r.state === 'starting') && (
                <Button variant="danger" onClick={() => stop.mutate(r.id)} disabled={stop.isPending}>
                  Stop
                </Button>
              )}
            </td>
          </tr>
        ))}
      </DataTable>
      )}
      <Pager page={at} pages={pages} from={(at - 1) * size + 1} to={Math.min(at * size, matching.length)} total={matching.length} onChange={setPage} noun="instances" />
      <ErrorText error={stop.error} />
      {details !== null && <InstanceDetails run={details} onClose={() => setDetails(null)} />}
    </div>
  )
}

function Body() {
  const { id = '' } = useParams()
  const eventId = Number(id)
  const [params, setParams] = useSearchParams()
  const q = useQuery({
    queryKey: ['admin-event', eventId],
    queryFn: async () => (await platform.adminEvents()).find((e) => e.id === eventId) ?? null,
    refetchInterval: 20_000,
  })
  usePageTitle(q.data?.name)
  if (q.isPending) return <Loading />
  const event = q.data
  if (!event) return <Empty>That event does not exist.</Empty>
  const jeopardy = event.kind === 'jeopardy'
  const tabs: TabItem<View>[] = [
    { id: 'overview', label: 'Overview' },
    { id: 'settings', label: 'Settings' },
    ...(jeopardy ? [] : [{ id: 'tournament' as const, label: 'Tournament' }]),
    { id: 'challenges', label: 'Challenges' },
    { id: 'participants', label: 'Participants' },
    ...(jeopardy ? [{ id: 'submissions' as const, label: 'Submissions' }, { id: 'integrity' as const, label: 'Integrity' }] : []),
    { id: 'infrastructure', label: 'Infrastructure' },
    { id: 'instances', label: 'Instances' },
  ]
  const requested = params.get('view') as View | null
  const view: View = tabs.some((t) => t.id === requested) ? (requested as View) : 'overview'
  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-center gap-3">
        <PageTitle>{event.name}</PageTitle>
        <StatePill state={event.effective_state} />
        <Pill>{jeopardy ? 'Jeopardy' : 'Tournament'}</Pill>
        <Pill>{event.mode === 'team' ? 'Teams' : 'Solo'}</Pill>
        <a href={jeopardy ? `/e/${event.slug}` : `/t/${event.slug}`} className="ml-auto inline-flex items-center gap-1.5 rounded-control border border-line px-3 py-1.5 text-sm text-muted transition-colors hover:border-field-line hover:text-ink">
          Public page <ExternalLink size={14} aria-hidden="true" />
        </a>
      </div>
      <Tabs<View> items={tabs} value={view} label="Event admin" onChange={(v) => setParams({ view: v }, { replace: true })}>
        {view === 'overview' && <Overview event={event} onOpen={(v) => setParams({ view: v }, { replace: true })} />}
        {view === 'settings' && <Settings event={event} />}
        {view === 'tournament' && <TournamentTab event={event} />}
        {view === 'challenges' && <Challenges event={event} />}
        {view === 'participants' && <Participants event={event} />}
        {view === 'submissions' && <Submissions event={event} />}
        {view === 'integrity' && <IntegrityTab event={event} />}
        {view === 'infrastructure' && <Infrastructure event={event} />}
        {view === 'instances' && <Instances event={event} />}
      </Tabs>
    </div>
  )
}

export function AdminEvent() {
  return (
    <div className="flex flex-col gap-6">
      <BackLink to="/organizer/events">Manage events</BackLink>
      <Gate need="admin">
        <Body />
      </Gate>
    </div>
  )
}
