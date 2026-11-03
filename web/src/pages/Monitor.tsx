import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState, type FormEvent } from 'react'
import { useIsAdmin, useWho } from '../api/client'
import { platform } from '../api/platform'
import type { RateRule, RateRuleBody } from '../api/platformTypes'
import { ConfirmDialog } from '../components/ConfirmDialog'
import { DataTable, Empty, Gate, Loading, Panel, Pill, Stat, TD, Toggle } from '../components/platform/bits'
import { Pager } from '../components/Pager'
import { Tabs, type TabItem } from '../components/Tabs'
import { Button, ErrorText, Field, Input, PageTitle, Select } from '../components/ui'
import { shortDuration } from '../lib/platform'
import { timeAgo } from '../lib/time'
import { usePageTitle } from '../lib/usePageTitle'

type View = 'traffic' | 'rules' | 'bans' | 'audit'

function statusTone(status: number): string {
  return status >= 500 ? 'text-danger' : status === 429 || status === 403 ? 'text-danger' : status >= 400 ? 'text-muted' : ''
}

function Top({ title, rows }: { title: string; rows: { key: string; count: number }[] }) {
  return (
    <Panel>
      <h3 className="mb-2 text-sm font-semibold">{title}</h3>
      {rows.length === 0 ? (
        <p className="text-sm text-muted">Nothing yet.</p>
      ) : (
        <ul className="flex flex-col gap-1 text-sm">
          {rows.map((r) => (
            <li key={r.key} className="flex justify-between gap-3">
              <span className="truncate font-mono text-xs">{r.key}</span>
              <span className="font-mono tabular-nums">{r.count}</span>
            </li>
          ))}
        </ul>
      )}
    </Panel>
  )
}

function Traffic() {
  const [minutes, setMinutes] = useState(60)
  const summary = useQuery({ queryKey: ['monitor-summary', minutes], queryFn: () => platform.monitorSummary(minutes), refetchInterval: 20_000 })
  const [ip, setIp] = useState('')
  const [route, setRoute] = useState('')
  const [status, setStatus] = useState('')
  const [page, setPage] = useState(1)
  const log = useQuery({
    queryKey: ['monitor-requests', ip, route, status, page],
    queryFn: () => platform.requests({ page, ip: ip.trim() || undefined, route: route.trim() || undefined, status: status ? Number(status) : undefined }),
    refetchInterval: 20_000,
  })
  const s = summary.data
  const pages = log.data ? Math.max(1, Math.ceil(log.data.total / log.data.per_page)) : 1
  const reset = <T,>(set: (v: T) => void) => (v: T) => {
    set(v)
    setPage(1)
  }
  return (
    <div className="flex flex-col gap-6">
      <Select value={minutes} onChange={(e) => setMinutes(Number(e.target.value))} aria-label="Time range" className="self-start">
        <option value={15}>Last 15 minutes</option>
        <option value={60}>Last hour</option>
        <option value={1440}>Last day</option>
        <option value={10080}>Last week</option>
      </Select>
      {s && (
        <>
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
            <Stat label="Requests" value={s.total} />
            <Stat label="Server errors" value={s.server_errors} tone={s.server_errors > 0 ? 'text-danger' : ''} />
            <Stat label="Rate limited" value={s.throttled} />
            <Stat label="Refused (403)" value={s.forbidden} />
          </div>
          <div className="grid gap-3 lg:grid-cols-3">
            <Top title="Busiest addresses" rows={s.top_ips} />
            <Top title="Busiest endpoints" rows={s.top_routes} />
            <Top title="Slowest endpoints (ms)" rows={s.slowest_routes} />
          </div>
        </>
      )}
      <div className="flex flex-wrap items-end gap-3">
        <Field label="Address">
          <Input value={ip} onChange={(e) => reset(setIp)(e.target.value)} placeholder="10.0.0.5" className="font-mono" />
        </Field>
        <Field label="Endpoint">
          <Input value={route} onChange={(e) => reset(setRoute)(e.target.value)} placeholder="/api/v1/auth/player-login" className="w-72 font-mono" />
        </Field>
        <Field label="Status">
          <Input value={status} onChange={(e) => reset(setStatus)(e.target.value.replace(/\D/g, ''))} placeholder="429" className="w-24 font-mono" inputMode="numeric" />
        </Field>
      </div>
      {log.isPending ? (
        <Loading />
      ) : log.data && log.data.requests.length > 0 ? (
        <>
          <DataTable caption="Requests" head={['When', 'Request', 'Status', 'Address', 'User', 'Time']}>
            {log.data.requests.map((r) => (
              <tr key={r.id}>
                <td className={`${TD} whitespace-nowrap text-muted`} title={new Date(r.at * 1000).toLocaleString()}>
                  {timeAgo(r.at)}
                </td>
                <td className={`${TD} font-mono text-xs`}>
                  {r.method} {r.path}
                </td>
                <td className={`${TD} font-mono ${statusTone(r.status)}`}>{r.status}</td>
                <td className={`${TD} font-mono text-xs`}>{r.ip}</td>
                <td className={TD}>{r.username ?? (r.user_id ? `#${r.user_id}` : '')}</td>
                <td className={`${TD} font-mono`}>{r.ms} ms</td>
              </tr>
            ))}
          </DataTable>
          <Pager page={page} pages={pages} from={(page - 1) * log.data.per_page + 1} to={Math.min(log.data.total, page * log.data.per_page)} total={log.data.total} onChange={setPage} noun="requests" />
        </>
      ) : (
        <Empty>No requests match.</Empty>
      )}
      <ErrorText error={log.error ?? summary.error} />
    </div>
  )
}

const blankRule: RateRuleBody = { endpoint: '', max_requests: 60, window_secs: 60, scope: 'ip', strikes_to_ban: null, ban_secs: 900, enabled: true }

function RuleForm({ rule, onDone }: { rule: RateRule | null; onDone: () => void }) {
  const client = useQueryClient()
  const [r, setR] = useState<RateRuleBody>(rule ? { ...rule } : blankRule)
  const save = useMutation({
    mutationFn: () => (rule ? platform.updateRule(rule.id, r) : platform.createRule(r)),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: ['rules'] })
      onDone()
    },
  })
  const submit = (e: FormEvent) => {
    e.preventDefault()
    save.mutate()
  }
  const autoBan = r.strikes_to_ban !== null
  return (
    <Panel>
      <form onSubmit={submit} className="flex flex-col gap-4">
        <h3 className="text-sm font-semibold">{rule ? 'Edit rule' : 'New rule'}</h3>
        <Field label="Endpoint (METHOD /path, /path/*, or *)">
          <Input value={r.endpoint} onChange={(e) => setR({ ...r, endpoint: e.target.value })} placeholder="POST /api/v1/events/{slug}/challenges/{id}/submit" className="font-mono" />
        </Field>
        <div className="grid gap-4 sm:grid-cols-4">
          <Field label="Requests">
            <Input type="number" min={1} value={r.max_requests} onChange={(e) => setR({ ...r, max_requests: Math.trunc(Number(e.target.value)) })} className="font-mono" />
          </Field>
          <Field label="Per (seconds)">
            <Input type="number" min={1} value={r.window_secs} onChange={(e) => setR({ ...r, window_secs: Math.trunc(Number(e.target.value)) })} className="font-mono" />
          </Field>
          <Field label="Counted per">
            <Select value={r.scope} onChange={(e) => setR({ ...r, scope: e.target.value as 'ip' | 'user' })}>
              <option value="ip">Address</option>
              <option value="user">Signed in user</option>
            </Select>
          </Field>
          <div className="flex items-end pb-2">
            <Toggle label="Enabled" checked={r.enabled} onChange={(v) => setR({ ...r, enabled: v })} />
          </div>
        </div>
        <div className="flex flex-wrap items-end gap-4">
          <Toggle label="Ban repeat offenders" checked={autoBan} onChange={(v) => setR({ ...r, strikes_to_ban: v ? 10 : null })} />
          {autoBan && (
            <>
              <Field label="Denied requests in 10 min">
                <Input type="number" min={1} value={r.strikes_to_ban ?? 10} onChange={(e) => setR({ ...r, strikes_to_ban: Math.trunc(Number(e.target.value)) })} className="w-28 font-mono" />
              </Field>
              <Field label="Ban for (seconds, 0 is permanent)">
                <Input type="number" min={0} value={r.ban_secs} onChange={(e) => setR({ ...r, ban_secs: Math.trunc(Number(e.target.value)) })} className="w-40 font-mono" />
              </Field>
            </>
          )}
        </div>
        <ErrorText error={save.error} />
        <div className="flex gap-2">
          <Button type="submit" variant="primary" disabled={!r.endpoint.trim() || save.isPending}>
            Save rule
          </Button>
          <Button variant="ghost" onClick={onDone}>
            Cancel
          </Button>
        </div>
      </form>
    </Panel>
  )
}

function Rules({ canEdit }: { canEdit: boolean }) {
  const client = useQueryClient()
  const q = useQuery({ queryKey: ['rules'], queryFn: platform.rules })
  const [editing, setEditing] = useState<RateRule | 'new' | null>(null)
  const [removing, setRemoving] = useState<RateRule | null>(null)
  const del = useMutation({
    mutationFn: (id: number) => platform.deleteRule(id),
    onSuccess: () => {
      setRemoving(null)
      void client.invalidateQueries({ queryKey: ['rules'] })
    },
  })
  const rules = q.data?.rules ?? []
  return (
    <div className="flex flex-col gap-4">
      <p className="max-w-2xl text-sm text-muted">Rules limit how often one address or user may call an endpoint. Staff are never limited. Endpoints are matched on the route pattern, so a rule on a path with parameters covers every value.</p>
      {canEdit && (
        <Button variant="primary" className="self-start" onClick={() => setEditing('new')}>
          New rule
        </Button>
      )}
      {editing && <RuleForm key={editing === 'new' ? 'new' : editing.id} rule={editing === 'new' ? null : editing} onDone={() => setEditing(null)} />}
      {q.isPending ? (
        <Loading />
      ) : (
        <DataTable caption="Rate rules" head={['Endpoint', 'Limit', 'Counted per', 'Ban', 'State', '']}>
          {rules.map((r) => (
            <tr key={r.id}>
              <td className={`${TD} break-all font-mono text-xs`}>{r.endpoint}</td>
              <td className={`${TD} whitespace-nowrap font-mono text-xs`}>
                {r.max_requests} / {shortDuration(r.window_secs)}
              </td>
              <td className={TD}>{r.scope === 'ip' ? 'Address' : 'User'}</td>
              <td className={`${TD} text-xs`}>{r.strikes_to_ban === null ? 'never' : `after ${r.strikes_to_ban} denials, ${r.ban_secs === 0 ? 'permanent' : shortDuration(r.ban_secs)}`}</td>
              <td className={TD}>{r.enabled ? <Pill tone="text-win">On</Pill> : <Pill>Off</Pill>}</td>
              <td className={TD}>
                {canEdit && (
                  <span className="flex gap-2">
                    <Button onClick={() => setEditing(r)}>Edit</Button>
                    <Button variant="danger" onClick={() => setRemoving(r)}>
                      Delete
                    </Button>
                  </span>
                )}
              </td>
            </tr>
          ))}
        </DataTable>
      )}
      {removing && (
        <ConfirmDialog
          title="Delete this rule?"
          message={`Requests to ${removing.endpoint} will no longer be limited by it.`}
          confirmLabel="Delete"
          danger
          pending={del.isPending}
          onConfirm={() => del.mutate(removing.id)}
          onClose={() => setRemoving(null)}
        />
      )}
      <ErrorText error={q.error ?? del.error} />
    </div>
  )
}

const DURATIONS: { label: string; secs: number | undefined }[] = [
  { label: '15 minutes', secs: 900 },
  { label: '1 hour', secs: 3600 },
  { label: '1 day', secs: 86400 },
  { label: '30 days', secs: 2592000 },
  { label: 'Permanent', secs: undefined },
]

function Bans({ canEdit }: { canEdit: boolean }) {
  const client = useQueryClient()
  const q = useQuery({ queryKey: ['bans'], queryFn: platform.bans, refetchInterval: 20_000 })
  const [kind, setKind] = useState<'ip' | 'user'>('ip')
  const [subject, setSubject] = useState('')
  const [reason, setReason] = useState('')
  const [duration, setDuration] = useState(1)
  const done = () => void client.invalidateQueries({ queryKey: ['bans'] })
  const create = useMutation({
    mutationFn: () => platform.createBan({ kind, subject: subject.trim(), reason: reason.trim(), duration_secs: DURATIONS[duration].secs }),
    onSuccess: () => {
      setSubject('')
      setReason('')
      done()
    },
  })
  const lift = useMutation({ mutationFn: (id: number) => platform.liftBan(id), onSuccess: done })
  const bans = q.data?.bans ?? []
  return (
    <div className="flex flex-col gap-6">
      {canEdit && (
        <Panel>
          <form
            className="flex flex-wrap items-end gap-3"
            onSubmit={(e) => {
              e.preventDefault()
              create.mutate()
            }}
          >
            <Field label="Ban">
              <Select value={kind} onChange={(e) => setKind(e.target.value as 'ip' | 'user')}>
                <option value="ip">Address</option>
                <option value="user">User id</option>
              </Select>
            </Field>
            <Field label={kind === 'ip' ? 'IP address' : 'User id'}>
              <Input value={subject} onChange={(e) => setSubject(e.target.value)} className="w-44 font-mono" placeholder={kind === 'ip' ? '203.0.113.9' : '42'} />
            </Field>
            <Field label="Reason">
              <Input value={reason} onChange={(e) => setReason(e.target.value)} className="w-64" maxLength={200} />
            </Field>
            <Field label="For">
              <Select value={duration} onChange={(e) => setDuration(Number(e.target.value))}>
                {DURATIONS.map((d, i) => (
                  <option key={d.label} value={i}>
                    {d.label}
                  </option>
                ))}
              </Select>
            </Field>
            <Button type="submit" variant="primary" disabled={!subject.trim() || !reason.trim() || create.isPending}>
              Ban
            </Button>
          </form>
          <div className="mt-2">
            <ErrorText error={create.error} />
          </div>
        </Panel>
      )}
      {q.isPending ? (
        <Loading />
      ) : bans.length === 0 ? (
        <Empty>No bans yet.</Empty>
      ) : (
        <DataTable caption="Bans" head={['Subject', 'Reason', 'Set', 'Until', 'State', '']}>
          {bans.map((b) => (
            <tr key={b.id}>
              <td className={`${TD} font-mono text-xs`}>
                {b.kind === 'ip' ? b.subject : `user #${b.subject}`}
                {b.auto && <span className="ml-2 font-sans text-muted">automatic</span>}
              </td>
              <td className={TD}>{b.reason}</td>
              <td className={`${TD} text-muted`}>{timeAgo(b.created_at)}</td>
              <td className={`${TD} text-muted`}>{b.expires_at ? new Date(b.expires_at * 1000).toLocaleString() : 'never'}</td>
              <td className={TD}>{b.active ? <Pill tone="text-danger">Active</Pill> : <Pill>{b.lifted_at ? 'Lifted' : 'Expired'}</Pill>}</td>
              <td className={TD}>
                {canEdit && b.active && (
                  <Button onClick={() => lift.mutate(b.id)} disabled={lift.isPending}>
                    Lift
                  </Button>
                )}
              </td>
            </tr>
          ))}
        </DataTable>
      )}
      <ErrorText error={q.error ?? lift.error} />
    </div>
  )
}

function Audit() {
  const [action, setAction] = useState('')
  const [page, setPage] = useState(1)
  const q = useQuery({ queryKey: ['audit', action, page], queryFn: () => platform.audit({ page, action: action.trim() || undefined }) })
  const pages = q.data ? Math.max(1, Math.ceil(q.data.total / q.data.per_page)) : 1
  return (
    <div className="flex flex-col gap-4">
      <Field label="Action">
        <Select
          value={action}
          onChange={(e) => {
            setAction(e.target.value)
            setPage(1)
          }}
          className="self-start"
        >
          <option value="">All actions</option>
          {['ban_create', 'ban_lift', 'auto_ban', 'rule_create', 'rule_update', 'rule_delete', 'user_update', 'event_state', 'instance_stop', 'event_export', 'event_import', 'backup_download', 'backup_create', 'ops_cleanup'].map((a) => (
            <option key={a} value={a}>
              {a.replace(/_/g, ' ')}
            </option>
          ))}
        </Select>
      </Field>
      {q.isPending ? (
        <Loading />
      ) : q.data && q.data.entries.length > 0 ? (
        <>
          <DataTable caption="Moderation audit" head={['When', 'By', 'Action', 'Target', 'Detail']}>
            {q.data.entries.map((a) => (
              <tr key={a.id}>
                <td className={`${TD} whitespace-nowrap text-muted`} title={new Date(a.at * 1000).toLocaleString()}>
                  {timeAgo(a.at)}
                </td>
                <td className={TD}>{a.actor_name ?? a.actor_role}</td>
                <td className={`${TD} font-mono text-xs`}>{a.action}</td>
                <td className={`${TD} font-mono text-xs`}>
                  {a.target_kind} {a.target_id}
                </td>
                <td className={`${TD} break-words text-muted`}>{a.detail}</td>
              </tr>
            ))}
          </DataTable>
          <Pager page={page} pages={pages} from={(page - 1) * q.data.per_page + 1} to={Math.min(q.data.total, page * q.data.per_page)} total={q.data.total} onChange={setPage} noun="entries" />
        </>
      ) : (
        <Empty>No entries.</Empty>
      )}
      <ErrorText error={q.error} />
    </div>
  )
}

function Body() {
  const [view, setView] = useState<View>('traffic')
  const admin = useIsAdmin()
  const who = useWho()
  // Monitors may change rules and bans too (they hold the moderation permission).
  const canEdit = admin || who?.role === 'monitor'
  const tabs: TabItem<View>[] = [
    { id: 'traffic', label: 'Traffic' },
    { id: 'rules', label: 'Rate limits' },
    { id: 'bans', label: 'Bans' },
    { id: 'audit', label: 'Audit' },
  ]
  return (
    <Tabs<View> items={tabs} value={view} label="Monitor" onChange={setView}>
      {view === 'traffic' && <Traffic />}
      {view === 'rules' && <Rules canEdit={canEdit} />}
      {view === 'bans' && <Bans canEdit={canEdit} />}
      {view === 'audit' && <Audit />}
    </Tabs>
  )
}

export function Monitor() {
  usePageTitle('Monitor')
  return (
    <div className="flex flex-col gap-6">
      <PageTitle>Monitor</PageTitle>
      <Gate need="monitor">
        <Body />
      </Gate>
    </div>
  )
}
