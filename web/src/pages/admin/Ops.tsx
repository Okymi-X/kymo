import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Archive, Download, RefreshCw, Trash2 } from 'lucide-react'
import { platform } from '../../api/platform'
import type { Health, RunInfo } from '../../api/platformTypes'
import { DataTable, Empty, Gate, Loading, Panel, Pill, Stat, TD } from '../../components/platform/bits'
import { Button, ErrorText, Field, Input, PageTitle } from '../../components/ui'
import { useState } from 'react'
import { fileSize } from '../../lib/platform'
import { timeAgo } from '../../lib/time'
import { usePageTitle } from '../../lib/usePageTitle'

function uptime(secs: number): string {
  const d = Math.floor(secs / 86400)
  const h = Math.floor((secs % 86400) / 3600)
  const m = Math.floor((secs % 3600) / 60)
  return d > 0 ? `${d}d ${h}h` : h > 0 ? `${h}h ${m}m` : `${m}m`
}

function Job({ name, info }: { name: string; info: RunInfo | null }) {
  return (
    <tr>
      <td className={`${TD} font-medium`}>{name}</td>
      <td className={`${TD} text-muted`}>{info ? timeAgo(info.at) : 'not run yet'}</td>
      <td className={TD}>{info?.error ? <span className="text-danger">{info.error}</span> : (info?.detail ?? '')}</td>
    </tr>
  )
}

function HealthView({ h }: { h: Health }) {
  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-center gap-3">
        <Pill tone={h.status === 'ok' ? 'text-win' : 'text-danger'}>{h.status === 'ok' ? 'All systems normal' : 'Degraded'}</Pill>
        <span className="text-sm text-muted">
          Version {h.version}, up {uptime(h.uptime_secs)}
        </span>
      </div>
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <Stat label="Database" value={h.database.ok ? `${h.database.ping_ms} ms` : 'down'} tone={h.database.ok ? '' : 'text-danger'} />
        <Stat label="Database size" value={fileSize(h.database.size_bytes)} />
        <Stat label={`Requests, last ${Math.round(h.traffic.window_secs / 60)} min`} value={h.traffic.requests} />
        <Stat label="Average response" value={`${h.traffic.avg_ms} ms`} />
        <Stat label="Server errors" value={h.traffic.server_errors} tone={h.traffic.server_errors > 0 ? 'text-danger' : ''} />
        <Stat label="Rate limited" value={h.traffic.throttled} />
        <Stat label="Live instances" value={h.instances.enabled ? h.instances.live : 'off'} />
        <Stat label="Orphaned containers" value={h.instances.enabled ? h.instances.orphans : 'off'} tone={h.instances.orphans > 0 ? 'text-danger' : ''} />
      </div>
      {h.instances.enabled && h.instances.runtime_ok === false && <p className="text-sm text-danger">The container platform cannot be reached.</p>}

      <section className="flex flex-col gap-2">
        <h2 className="text-lg font-semibold tracking-tight">Background jobs</h2>
        <DataTable caption="Background jobs" head={['Job', 'Last run', 'Result']}>
          <Job name="Instance reaper" info={h.jobs.reaper} />
          <Job name="Maintenance" info={h.jobs.maintenance} />
          <Job name="Backup" info={h.jobs.backup} />
        </DataTable>
      </section>

      <section className="flex flex-col gap-2">
        <h2 className="text-lg font-semibold tracking-tight">Events</h2>
        {h.events.length === 0 ? (
          <Empty>No open events.</Empty>
        ) : (
          <DataTable caption="Events" head={['Event', 'State', 'Players', 'Instances', 'Submissions, 10 min']}>
            {h.events.map((e) => (
              <tr key={e.id}>
                <td className={`${TD} font-medium`}>{e.name}</td>
                <td className={TD}>{e.effective_state}</td>
                <td className={`${TD} font-mono tabular-nums`}>{e.participants}</td>
                <td className={`${TD} font-mono tabular-nums`}>{e.live_instances}</td>
                <td className={`${TD} font-mono tabular-nums`}>{e.submissions_last_10m}</td>
              </tr>
            ))}
          </DataTable>
        )}
      </section>

      <section className="flex flex-col gap-2">
        <h2 className="text-lg font-semibold tracking-tight">Recent server errors</h2>
        {h.recent_errors.length === 0 ? (
          <Empty>No server errors recorded.</Empty>
        ) : (
          <DataTable caption="Recent server errors" head={['When', 'Request', 'Status', 'Time']}>
            {h.recent_errors.map((e, i) => (
              <tr key={i}>
                <td className={`${TD} text-muted`}>{timeAgo(e.at)}</td>
                <td className={`${TD} font-mono text-xs`}>
                  {e.method} {e.route}
                </td>
                <td className={`${TD} font-mono text-danger`}>{e.status}</td>
                <td className={`${TD} font-mono`}>{e.ms} ms</td>
              </tr>
            ))}
          </DataTable>
        )}
      </section>
    </div>
  )
}

function VpnSection() {
  const client = useQueryClient()
  const q = useQuery({ queryKey: ['admin-vpn'], queryFn: platform.adminVpn, refetchInterval: 15_000 })
  const sync = useMutation({ mutationFn: platform.syncVpn, onSuccess: () => void client.invalidateQueries({ queryKey: ['admin-vpn'] }) })
  const [user, setUser] = useState('')
  const revoke = useMutation({
    mutationFn: () => platform.revokeVpnClient(Number(user)),
    onSuccess: () => {
      setUser('')
      void client.invalidateQueries({ queryKey: ['admin-vpn'] })
    },
  })
  const v = q.data
  if (!v) return q.error ? <ErrorText error={q.error} /> : null
  if (!v.enabled) {
    return (
      <Panel className="flex flex-col gap-2">
        <h2 className="text-lg font-semibold tracking-tight">VPN gateway</h2>
        <p className="text-sm text-muted">Not enabled. Set KYMO_VPN=1 and KYMO_VPN_HOST on the server to give each player their own private network. The README explains the setup.</p>
      </Panel>
    )
  }
  const ok = v.gateway?.running === true
  return (
    <Panel className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center gap-3">
        <h2 className="text-lg font-semibold tracking-tight">VPN gateway</h2>
        <Pill tone={ok ? 'text-win' : 'text-danger'}>{ok ? 'Running' : 'Not running'}</Pill>
        <span className="text-sm text-muted">
          Players connect to {v.host}:{v.port}
        </span>
      </div>
      {v.gateway?.error && <p className="text-sm text-danger">{v.gateway.error}</p>}
      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <Stat label="Players with a profile" value={`${v.tunnel?.used ?? 0}/${v.tunnel?.capacity ?? 0}`} />
        <Stat label="Private networks" value={`${v.lab_pool?.used ?? 0}/${v.lab_pool?.capacity ?? 0}`} />
        <Stat label="Tunnel range" value={<span className="text-base">{v.tunnel?.net}</span>} />
        <Stat label="Network range" value={<span className="text-base">{v.lab_pool?.net}</span>} />
      </div>
      {(v.labs ?? []).length > 0 && (
        <DataTable caption="Private networks" head={['Network', 'Range', 'Owner', 'Event', 'Instances']}>
          {(v.labs ?? []).map((l) => (
            <tr key={l.id}>
              <td className={`${TD} font-mono text-xs`}>{l.network}</td>
              <td className={`${TD} font-mono text-xs`}>{l.subnet}</td>
              <td className={TD}>
                {l.participant.kind} {l.participant.id}
              </td>
              <td className={`${TD} font-mono`}>#{l.event_id}</td>
              <td className={`${TD} font-mono tabular-nums`}>{l.live_instances}</td>
            </tr>
          ))}
        </DataTable>
      )}
      <div className="flex flex-wrap items-end gap-3">
        <Button onClick={() => sync.mutate()} disabled={sync.isPending}>
          <RefreshCw size={15} aria-hidden="true" /> Sync the gateway now
        </Button>
        <Field label="Revoke a player's VPN (user id)">
          <Input value={user} onChange={(e) => setUser(e.target.value.replace(/\D/g, ''))} inputMode="numeric" className="w-32 font-mono" />
        </Field>
        <Button variant="danger" onClick={() => revoke.mutate()} disabled={!user || revoke.isPending}>
          Revoke
        </Button>
      </div>
      {sync.data && (
        <p className="text-sm text-win">
          Synced: {sync.data.labs} networks, {sync.data.clients} players, {sync.data.allows} allowed pairs, {sync.data.disconnected} disconnected.
        </p>
      )}
      <ErrorText error={sync.error ?? revoke.error} />
    </Panel>
  )
}

function Backups() {
  const client = useQueryClient()
  const q = useQuery({ queryKey: ['backups'], queryFn: platform.backups })
  const make = useMutation({ mutationFn: platform.createBackup, onSuccess: () => void client.invalidateQueries({ queryKey: ['backups'] }) })
  const download = useMutation({ mutationFn: platform.downloadBackup })
  const cleanup = useMutation({ mutationFn: platform.cleanup })
  return (
    <Panel className="flex flex-col gap-4">
      <h2 className="text-lg font-semibold tracking-tight">Backups and cleanup</h2>
      <p className="text-sm text-muted">
        A download is a full copy of the database, including password hashes. Keep it private. To restore, stop the server and run <code className="font-mono">kymo-server restore backup.db</code>.
      </p>
      <div className="flex flex-wrap gap-2">
        <Button onClick={() => download.mutate()} disabled={download.isPending}>
          <Download size={15} aria-hidden="true" /> Download a backup
        </Button>
        <Button onClick={() => make.mutate()} disabled={make.isPending || !q.data?.configured}>
          <Archive size={15} aria-hidden="true" /> Save one on the server
        </Button>
        <Button onClick={() => cleanup.mutate()} disabled={cleanup.isPending}>
          <Trash2 size={15} aria-hidden="true" /> Clean up now
        </Button>
      </div>
      <ErrorText error={download.error ?? make.error ?? cleanup.error} />
      {cleanup.data && (
        <p className="text-sm text-win">
          Cleaned up: {cleanup.data.instances_ended} instances stopped, {cleanup.data.log_rows} old log rows and {cleanup.data.expired_sessions} expired sessions removed.
        </p>
      )}
      {q.data && !q.data.configured && <p className="text-sm text-muted">Scheduled backups are off. Set KYMO_BACKUP_DIR on the server to turn them on.</p>}
      {q.data && q.data.configured && (
        <>
          <p className="text-sm text-muted">
            Every {Math.round((q.data.every_secs ?? 0) / 3600)} hours, keeping the newest {q.data.keep}.
          </p>
          {q.data.backups.length === 0 ? (
            <p className="text-sm text-muted">No backups yet.</p>
          ) : (
            <ul className="flex flex-col divide-y divide-line text-sm">
              {q.data.backups.map((b) => (
                <li key={b.name} className="flex justify-between py-2 first:pt-0 last:pb-0">
                  <span className="font-mono">{b.name}</span>
                  <span className="text-muted">{fileSize(b.size)}</span>
                </li>
              ))}
            </ul>
          )}
        </>
      )}
    </Panel>
  )
}

function Body() {
  const q = useQuery({ queryKey: ['health'], queryFn: platform.health, refetchInterval: 15_000 })
  return (
    <div className="flex flex-col gap-6">
      {q.isPending ? <Loading /> : q.data ? <HealthView h={q.data} /> : <ErrorText error={q.error} />}
      <VpnSection />
      <Backups />
    </div>
  )
}

export function Ops() {
  usePageTitle('Operations')
  return (
    <div className="flex flex-col gap-6">
      <PageTitle>Operations</PageTitle>
      <Gate need="admin">
        <Body />
      </Gate>
    </div>
  )
}
