import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Download, Plus, X } from 'lucide-react'
import { useEffect, useState, type FormEvent } from 'react'
import { useNavigate, useParams, useSearchParams } from 'react-router'
import { platform } from '../../api/platform'
import type { AdminChallenge, EventInfo, JeopardySettings, SubmissionLine } from '../../api/platformTypes'
import { BackLink } from '../../components/BackLink'
import { ConfirmDialog } from '../../components/ConfirmDialog'
import { DataTable, Empty, Gate, Loading, Panel, Pill, StatePill, TD, Toggle } from '../../components/platform/bits'
import { Pager } from '../../components/Pager'
import { Tabs, type TabItem } from '../../components/Tabs'
import { Button, ErrorText, Field, Input, PageTitle, Select, Textarea } from '../../components/ui'
import { fromLocalInput, nextStates, scoringLabel, stateLabel, toLocalInput, verdictText } from '../../lib/platform'
import { timeAgo } from '../../lib/time'
import { usePageTitle } from '../../lib/usePageTitle'
import { ChallengeEditor } from './ChallengeEditor'

type View = 'overview' | 'challenges' | 'participants' | 'submissions' | 'instances'

const TIE_BREAKS: { id: JeopardySettings['tie_breaks'][number]; label: string }[] = [
  { id: 'earliest_last_solve', label: 'Reached the score first' },
  { id: 'most_solves', label: 'More solved challenges' },
  { id: 'first_blood', label: 'More first bloods' },
]

function Overview({ event }: { event: EventInfo }) {
  const client = useQueryClient()
  const navigate = useNavigate()
  const done = () => {
    void client.invalidateQueries({ queryKey: ['admin-event', event.id] })
    void client.invalidateQueries({ queryKey: ['admin-events'] })
  }
  const move = useMutation({ mutationFn: (state: string) => platform.setEventState(event.id, state), onSuccess: done })
  const [removing, setRemoving] = useState(false)
  const remove = useMutation({
    mutationFn: () => platform.deleteEvent(event.id),
    onSuccess: () => {
      done()
      navigate('/admin/events')
    },
  })
  const [f, setF] = useState(() => ({
    name: event.name,
    description: event.description,
    mode: event.mode,
    opens: toLocalInput(event.registration_opens),
    closes: toLocalInput(event.registration_closes),
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
        registration_closes: fromLocalInput(f.closes),
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
  const exportBundle = useMutation({ mutationFn: () => platform.exportEvent(event.id, event.slug) })
  const submit = (e: FormEvent) => {
    e.preventDefault()
    save.mutate()
  }
  const teams = f.mode === 'team'
  const options = nextStates[event.state]
  return (
    <div className="flex flex-col gap-6">
      <Panel className="flex flex-col gap-3">
        <div className="flex flex-wrap items-center gap-3">
          <h2 className="text-lg font-semibold tracking-tight">State</h2>
          <StatePill state={event.state} />
          {event.effective_state !== event.state && <span className="text-sm text-muted">now acting as {stateLabel[event.effective_state].toLowerCase()} by its schedule</span>}
        </div>
        <div className="flex flex-wrap gap-2">
          {options.map((s) => (
            <Button key={s} onClick={() => move.mutate(s)} disabled={move.isPending}>
              Move to {stateLabel[s].toLowerCase()}
            </Button>
          ))}
          {(event.state === 'draft' || event.state === 'archived') && (
            <Button variant="danger" onClick={() => setRemoving(true)}>
              Delete event
            </Button>
          )}
          {event.kind === 'jeopardy' && (
            <Button onClick={() => exportBundle.mutate()} disabled={exportBundle.isPending}>
              <Download size={15} aria-hidden="true" /> Export bundle
            </Button>
          )}
        </div>
        <ErrorText error={move.error ?? remove.error ?? exportBundle.error} />
      </Panel>

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
          <Field label="Description and rules">
            <Textarea value={f.description} onChange={(e) => setF({ ...f, description: e.target.value })} maxLength={2000} />
          </Field>
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
            <Field label="Registration opens">
              <Input type="datetime-local" value={f.opens} onChange={(e) => setF({ ...f, opens: e.target.value })} />
            </Field>
            <Field label="Registration closes">
              <Input type="datetime-local" value={f.closes} onChange={(e) => setF({ ...f, closes: e.target.value })} />
            </Field>
            <Field label="Starts">
              <Input type="datetime-local" value={f.starts} onChange={(e) => setF({ ...f, starts: e.target.value })} />
            </Field>
            <Field label="Ends">
              <Input type="datetime-local" value={f.ends} onChange={(e) => setF({ ...f, ends: e.target.value })} />
            </Field>
          </div>
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

      {event.kind === 'jeopardy' && <NetworkMode event={event} />}

      {event.kind === 'jeopardy' && <JeopardySettingsForm eventId={event.id} />}

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

function NetworkMode({ event }: { event: EventInfo }) {
  const vpn = useQuery({ queryKey: ['admin-vpn'], queryFn: platform.adminVpn })
  const client = useQueryClient()
  const [local, setLocal] = useState<'published' | 'vpn' | null>(null)
  const set = useMutation({
    mutationFn: (mode: 'published' | 'vpn') => platform.setNetworkMode(event.id, mode),
    onSettled: async () => {
      await client.invalidateQueries({ queryKey: ['admin-event', event.id] })
      setLocal(null)
    },
  })
  const enabled = vpn.data?.enabled ?? false
  return (
    <Panel className="flex flex-col gap-3">
      <h2 className="text-lg font-semibold tracking-tight">Instance network</h2>
      <p className="text-sm text-muted">
        <strong>Published ports</strong>: each instance gets a port on the server; anyone who can reach the server can reach it. <strong>Private VPN</strong>: each player or team gets an isolated network and connects through their own OpenVPN profile; instances cannot see each other.
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
          className="w-64"
        >
          <option value="published">Published ports</option>
          <option value="vpn" disabled={!enabled && event.network_mode !== 'vpn'}>
            Private VPN{enabled ? '' : ' (not enabled on this server)'}
          </option>
        </Select>
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
        <p className="text-sm text-muted">Wrong-format flags never count as an attempt. Dynamic challenges lose points for earlier solvers too, the board always recomputes.</p>
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
        <DataTable caption="Challenges" head={['Challenge', 'Category', 'Points', 'Solves', 'State']}>
          {challenges.map((c) => (
            <tr key={c.id} className="hover:bg-line/30">
              <td className={TD}>
                <button type="button" onClick={() => setEditing(c.id)} className="text-left font-medium text-accent hover:underline">
                  {c.title}
                </button>
                {c.requires_instance && <span className="ml-2 text-xs text-muted">instance</span>}
              </td>
              <td className={TD}>{categories.find((k) => k.id === c.category_id)?.name ?? ''}</td>
              <td className={`${TD} font-mono text-xs`}>{scoringLabel(c.scoring)}</td>
              <td className={`${TD} font-mono tabular-nums`}>{c.solves}</td>
              <td className={TD}>
                <Pill tone={c.state === 'published' ? 'text-win' : 'text-muted'}>{c.state}</Pill>
              </td>
            </tr>
          ))}
        </DataTable>
      )}
      {editing !== null && (editing === 'new' || current) && (
        <ChallengeEditor key={String(editing)} eventId={event.id} categories={categories} challenge={current} onClose={() => setEditing(null)} />
      )}
    </div>
  )
}

function Participants({ event }: { event: EventInfo }) {
  const q = useQuery({ queryKey: ['participants', event.id], queryFn: () => platform.participants(event.id) })
  if (q.isPending) return <Loading />
  const rows = q.data ?? []
  if (rows.length === 0) return <Empty>Nobody has registered yet.</Empty>
  return (
    <DataTable caption="Participants" head={[event.mode === 'team' ? 'Team' : 'Player', 'Members', 'Registered']}>
      {rows.map((p) => (
        <tr key={p.id}>
          <td className={`${TD} font-medium`}>{p.name}</td>
          <td className={`${TD} font-mono tabular-nums`}>{p.members}</td>
          <td className={`${TD} text-muted`}>{timeAgo(p.registered_at)}</td>
        </tr>
      ))}
    </DataTable>
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
  if (q.isPending) return <Loading />
  if (q.error) return <ErrorText error={q.error} />
  const rows = q.data?.instances ?? []
  if (rows.length === 0) return <Empty>No instances have been started.</Empty>
  return (
    <div className="flex flex-col gap-3">
      <DataTable caption="Instances" head={['Run', 'Owner', 'State', 'Port', 'Started', 'Expires', '']}>
        {rows.map((r) => (
          <tr key={r.id}>
            <td className={`${TD} font-mono`}>#{r.id}</td>
            <td className={TD}>
              {r.participant.kind} {r.participant.id}
            </td>
            <td className={TD}>{r.state}</td>
            <td className={`${TD} font-mono`}>{r.host_port ?? ''}</td>
            <td className={`${TD} text-muted`}>{timeAgo(r.started_at)}</td>
            <td className={`${TD} text-muted`}>{new Date(r.expires_at * 1000).toLocaleTimeString()}</td>
            <td className={TD}>
              {(r.state === 'running' || r.state === 'starting') && (
                <Button variant="danger" onClick={() => stop.mutate(r.id)} disabled={stop.isPending}>
                  Stop
                </Button>
              )}
            </td>
          </tr>
        ))}
      </DataTable>
      <ErrorText error={stop.error} />
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
    ...(jeopardy ? [{ id: 'challenges' as const, label: 'Challenges' }] : []),
    { id: 'participants', label: 'Participants' },
    ...(jeopardy ? [{ id: 'submissions' as const, label: 'Submissions' }, { id: 'instances' as const, label: 'Instances' }] : []),
  ]
  const requested = params.get('view') as View | null
  const view: View = tabs.some((t) => t.id === requested) ? (requested as View) : 'overview'
  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-center gap-3">
        <PageTitle>{event.name}</PageTitle>
        <StatePill state={event.effective_state} />
        <Pill>{event.mode === 'team' ? 'Teams' : 'Solo'}</Pill>
      </div>
      <div className="flex gap-4 text-sm">
        <a href={jeopardy ? `/e/${event.slug}` : `/t/${event.slug}`} className="text-accent hover:underline">
          Open the public page
        </a>
      </div>
      <Tabs<View> items={tabs} value={view} label="Event admin" onChange={(v) => setParams({ view: v }, { replace: true })}>
        {view === 'overview' && <Overview event={event} />}
        {view === 'challenges' && <Challenges event={event} />}
        {view === 'participants' && <Participants event={event} />}
        {view === 'submissions' && <Submissions event={event} />}
        {view === 'instances' && <Instances event={event} />}
      </Tabs>
    </div>
  )
}

export function AdminEvent() {
  return (
    <div className="flex flex-col gap-6">
      <BackLink to="/admin/events">Manage events</BackLink>
      <Gate need="admin">
        <Body />
      </Gate>
    </div>
  )
}
