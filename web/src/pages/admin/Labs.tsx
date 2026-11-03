import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Server } from 'lucide-react'
import { useState } from 'react'
import { platform } from '../../api/platform'
import type { LabCheck, LabHost } from '../../api/platformTypes'
import { ConfirmDialog } from '../../components/ConfirmDialog'
import { ConsoleDialog } from '../../components/platform/ConsoleDialog'
import { LabDialog } from '../../components/platform/LabDialog'
import { CopyButton, DataTable, Empty, Gate, Loading, Panel, Pill, TD } from '../../components/platform/bits'
import { Button, ErrorText, PageTitle } from '../../components/ui'
import { timeAgo } from '../../lib/time'
import { usePageTitle } from '../../lib/usePageTitle'

const INSTALL = 'sudo ./install-lab.sh --public-host YOUR_ADDRESS --allow-from PLATFORM_ADDRESS'

function status(lab: LabHost): { label: string; tone: string } {
  if (!lab.enabled) return { label: 'Switched off', tone: 'text-muted' }
  if (lab.last_checked_at === null) return { label: 'Not checked yet', tone: 'text-muted' }
  return lab.last_error ? { label: 'Unreachable', tone: 'text-danger' } : { label: 'Online', tone: 'text-win' }
}

function encryption(lab: LabHost): string {
  if (lab.api_url.startsWith('http://')) return 'Not encrypted'
  return lab.fingerprint ? 'Encrypted, certificate pinned' : 'Encrypted, certificate authority'
}

function Setup() {
  return (
    <Panel className="flex flex-col gap-3">
      <div className="flex items-start gap-3">
        <Server size={20} aria-hidden="true" className="mt-0.5 shrink-0 text-accent" />
        <div className="flex flex-col gap-1">
          <h2 className="text-lg font-semibold tracking-tight">Run challenges on a separate machine</h2>
          <p className="text-sm text-muted">
            A lab host runs only challenge containers. Keeping them off this server means a challenge that is broken into cannot reach the accounts, flags or database here. Each event picks where its instances run.
          </p>
        </div>
      </div>
      <ol className="ml-5 list-decimal space-y-2 text-sm">
        <li>
          On the new machine (Linux with Docker and systemd), download <code className="font-mono text-xs">kymo-lab-…-linux-x86_64.tar.gz</code> from the Kymo releases page, unpack it and run the installer:{' '}
          <span className="inline-flex flex-wrap items-center gap-2 align-middle">
            <code className="rounded-control border border-line bg-bg px-2 py-1 font-mono text-xs">{INSTALL}</code>
            <CopyButton text={INSTALL} />
          </span>
        </li>
        <li>It prints an API address, a token and a certificate fingerprint. Add them here with Add a lab host.</li>
        <li>Put the images your challenges use on the lab host and describe them in its manifest folder. Then choose the lab host in the event's settings.</li>
      </ol>
    </Panel>
  )
}

function CheckView({ check }: { check: LabCheck }) {
  if (!check.ok) {
    return (
      <p role="alert" className="break-words text-sm text-danger">
        {check.error}
      </p>
    )
  }
  return (
    <div className="flex flex-col gap-2 text-sm" role="status">
      <p className="text-win">
        Answered in {check.ms} ms: version {check.health?.version}, {(check.health?.runtimes ?? [check.health?.runtime]).join(' and ')}, {check.health?.mode === 'manifest' ? 'runs only listed images' : 'runs any valid image'}, {check.health?.running} of {check.health?.max_instances} instances in use.
      </p>
      {check.health?.runtimes?.includes('vm') && (
        <p className={check.health.accel === 'kvm' ? 'text-win' : 'text-danger'}>
          {check.health.accel === 'kvm' ? 'Virtual machines run with KVM acceleration.' : 'Virtual machines run in plain emulation, which is very slow. Give the lab host KVM (the service user must be in the kvm group).'}
        </p>
      )}
      {(check.artifacts ?? []).length > 0 ? (
        <ul className="flex flex-wrap gap-1.5">
          {check.artifacts?.map((a) => (
            <li
              key={a.artifact}
              className={`flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 font-mono text-xs ${a.ready === false ? 'border-danger/40' : 'border-line'}`}
              title={`${(a.ports ?? [{ name: 'main', port: a.port }]).map((p) => `${p.name} ${p.port}`).join(', ')}, up to ${Math.round(a.max_ttl_secs / 60)} minutes${a.note ? `. ${a.note}` : ''}`}
            >
              {a.artifact}
              {a.runtime === 'vm' && <span className="rounded bg-line/60 px-1 font-sans text-[10px] uppercase tracking-wide text-muted">vm</span>}
              {a.ready === false && <span className="font-sans text-[10px] text-danger">{a.note?.includes('unpacked') ? 'preparing' : 'not ready'}</span>}
            </li>
          ))}
        </ul>
      ) : (
        check.health?.mode === 'manifest' && <p className="text-danger">No manifests: this lab host will refuse every start.</p>
      )}
      {(check.notes ?? []).map((n) => (
        <p key={n} className="text-danger">
          {n}
        </p>
      ))}
    </div>
  )
}

function Running({ lab }: { lab: LabHost }) {
  const [console_, setConsole] = useState<string | null>(null)
  const q = useQuery({ queryKey: ['lab-instances', lab.id], queryFn: () => platform.labInstances(lab.id), refetchInterval: 10_000 })
  if (q.isPending) return <Loading />
  if (q.error) return <ErrorText error={q.error} />
  const rows = q.data.instances
  if (rows.length === 0) return <p className="text-sm text-muted">Nothing is running on this lab host.</p>
  return (
    <>
      <DataTable caption={`Instances on ${lab.name}`} head={['Name', 'State', 'Port', 'Expires', '']}>
      {rows.map((i) => (
        <tr key={i.name}>
          <td className={`${TD} font-mono text-xs`}>{i.name}</td>
          <td className={TD}>{i.status}</td>
          <td className={`${TD} font-mono`}>{i.host_port}</td>
          <td className={`${TD} text-muted`}>{new Date(i.expires_at * 1000).toLocaleTimeString()}</td>
          <td className={TD}>
            <span className="flex flex-wrap items-center gap-2">
              {!i.known && <Pill tone="text-danger">Not known here</Pill>}
              {i.runtime === 'vm' && (
                <Button onClick={() => setConsole(i.name)} className="h-8 px-2.5 text-xs">
                  Console
                </Button>
              )}
            </span>
          </td>
        </tr>
      ))}
      </DataTable>
      {console_ && <ConsoleDialog labId={lab.id} name={console_} onClose={() => setConsole(null)} />}
    </>
  )
}

function LabCard({ lab, onEdit }: { lab: LabHost; onEdit: () => void }) {
  const client = useQueryClient()
  const [check, setCheck] = useState<LabCheck | null>(null)
  const [showing, setShowing] = useState(false)
  const [cleaning, setCleaning] = useState(false)
  const [removing, setRemoving] = useState(false)
  const [note, setNote] = useState('')
  const refresh = () => void client.invalidateQueries({ queryKey: ['labs'] })
  const test = useMutation({
    mutationFn: () => platform.testLab(lab.id),
    onSuccess: (c) => {
      setCheck(c)
      refresh()
    },
  })
  const toggle = useMutation({ mutationFn: () => platform.updateLab(lab.id, { enabled: !lab.enabled }), onSuccess: refresh })
  const clean = useMutation({
    mutationFn: () => platform.reconcileLab(lab.id),
    onSuccess: (r) => {
      setNote(r.removed.length === 0 ? 'Nothing to clean up.' : `Removed ${r.removed.length} container${r.removed.length === 1 ? '' : 's'} the platform no longer lists.`)
      setCleaning(false)
      void client.invalidateQueries({ queryKey: ['lab-instances', lab.id] })
    },
  })
  const remove = useMutation({
    mutationFn: () => platform.deleteLab(lab.id),
    onSuccess: () => {
      setRemoving(false)
      refresh()
    },
  })
  const s = status(lab)
  return (
    <Panel className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <h3 className="text-base font-semibold tracking-tight">{lab.name}</h3>
        <Pill tone={s.tone}>{s.label}</Pill>
        <span className="text-xs text-muted">{lab.last_checked_at ? `checked ${timeAgo(lab.last_checked_at)}` : ''}</span>
      </div>
      <dl className="grid gap-x-6 gap-y-1 text-sm sm:grid-cols-2">
        <div className="min-w-0">
          <dt className="text-xs text-muted">API address</dt>
          <dd className="truncate font-mono text-xs" title={lab.api_url}>
            {lab.api_url}
          </dd>
        </div>
        <div>
          <dt className="text-xs text-muted">Players connect to</dt>
          <dd className="font-mono text-xs">{lab.public_host}</dd>
        </div>
        <div>
          <dt className="text-xs text-muted">Connection</dt>
          <dd className={lab.api_url.startsWith('http://') ? 'text-danger' : ''}>{encryption(lab)}</dd>
        </div>
        <div>
          <dt className="text-xs text-muted">In use</dt>
          <dd>
            {lab.events} event{lab.events === 1 ? '' : 's'}, {lab.live_instances} running instance{lab.live_instances === 1 ? '' : 's'}
          </dd>
        </div>
      </dl>
      {lab.last_error && lab.enabled && <p className="break-words text-sm text-danger">{lab.last_error}</p>}
      <div className="flex flex-wrap gap-2">
        <Button onClick={() => test.mutate()} disabled={test.isPending}>
          {test.isPending ? 'Testing…' : 'Test connection'}
        </Button>
        <Button onClick={() => setShowing((v) => !v)} aria-expanded={showing}>
          What is running
        </Button>
        <Button onClick={onEdit}>Edit</Button>
        <Button onClick={() => toggle.mutate()} disabled={toggle.isPending}>
          {lab.enabled ? 'Switch off' : 'Switch on'}
        </Button>
        <Button onClick={() => setCleaning(true)}>Clean up</Button>
        <Button variant="danger" onClick={() => setRemoving(true)}>
          Remove
        </Button>
      </div>
      {check && <CheckView check={check} />}
      {note && (
        <p role="status" className="text-sm text-muted">
          {note}
        </p>
      )}
      {showing && <Running lab={lab} />}
      <ErrorText error={test.error ?? toggle.error ?? clean.error} />
      {cleaning && (
        <ConfirmDialog
          title={`Clean up ${lab.name}?`}
          message="Containers on the lab host that the platform does not list are removed. Running instances that players are using are kept."
          confirmLabel="Clean up"
          pending={clean.isPending}
          onConfirm={() => clean.mutate()}
          onClose={() => setCleaning(false)}
        />
      )}
      {removing && (
        <ConfirmDialog
          title={`Remove ${lab.name}?`}
          message="The platform forgets this lab host. Nothing on the lab host itself is deleted. It cannot be removed while an event uses it or instances run on it."
          confirmLabel="Remove"
          danger
          pending={remove.isPending}
          onConfirm={() => remove.mutate()}
          onClose={() => {
            setRemoving(false)
            remove.reset()
          }}
        />
      )}
      {removing && <ErrorText error={remove.error} />}
    </Panel>
  )
}

function Body() {
  const q = useQuery({ queryKey: ['labs'], queryFn: platform.labs, refetchInterval: 20_000 })
  const [editing, setEditing] = useState<LabHost | 'new' | null>(null)
  return (
    <div className="flex flex-col gap-6">
      <Setup />
      <div className="flex items-center justify-between gap-3">
        <h2 className="text-lg font-semibold tracking-tight">Lab hosts</h2>
        <Button variant="primary" onClick={() => setEditing('new')}>
          Add a lab host
        </Button>
      </div>
      {q.isPending ? <Loading /> : q.error ? <ErrorText error={q.error} /> : q.data.labs.length === 0 ? <Empty>No lab host yet. Events run their instances on this server until you add one.</Empty> : q.data.labs.map((lab) => <LabCard key={lab.id} lab={lab} onEdit={() => setEditing(lab)} />)}
      {editing && <LabDialog key={editing === 'new' ? 'new' : editing.id} lab={editing === 'new' ? undefined : editing} onClose={() => setEditing(null)} />}
    </div>
  )
}

export function Labs() {
  usePageTitle('Lab hosts')
  return (
    <div className="flex flex-col gap-6">
      <PageTitle>Lab hosts</PageTitle>
      <Gate need="admin">
        <Body />
      </Gate>
    </div>
  )
}
