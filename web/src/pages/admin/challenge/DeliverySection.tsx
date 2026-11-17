import { Box, Cpu, Flag } from 'lucide-react'
import type { ReactNode } from 'react'
import type { LabArtifact, LabCatalog, LabChoice } from '../../../api/platformTypes'
import { Toggle } from '../../../components/platform/bits'
import { Field, Input, Select } from '../../../components/ui'
import { artifactsFor, displayText, findArtifact, runtimeLabel, type Delivery, type DeliveryProblems } from '../../../lib/delivery'
import type { Draft, Problems } from './draft'
import { Num, Section } from './parts'

interface Props {
  d: Draft
  set: <K extends keyof Draft>(key: K, value: Draft[K]) => void
  delivery: Delivery
  onDelivery: (next: Delivery) => void
  onArtifact: (artifact: LabArtifact) => void
  onResponder: (on: boolean) => void
  labs: LabChoice[]
  /** The lab host the event runs on; null is this server. */
  eventLab: LabChoice | null
  catalog: LabCatalog | null
  loading: boolean
  shown: Problems
  problems: DeliveryProblems
  tournament: boolean
  locked: boolean
}

const WAYS: { id: Delivery; title: string; text: string; icon: ReactNode }[] = [
  { id: 'none', title: 'Flag only', text: 'The flag is in the files or the description. Nothing is started.', icon: <Flag size={18} aria-hidden="true" /> },
  { id: 'container', title: 'Container', text: 'A Docker or Podman service, started for each player or team.', icon: <Box size={18} aria-hidden="true" /> },
  { id: 'vm', title: 'Virtual machine', text: 'A machine from an OVA, qcow2 or VMDK image, booted on a lab host for each player or team.', icon: <Cpu size={18} aria-hidden="true" /> },
]

/** How players get this challenge: nothing, a container or a machine, and everything that depends on it. */
export function DeliverySection(p: Props) {
  const { d, delivery } = p
  const vm = delivery === 'vm'
  const effective = d.labHost === '' ? p.eventLab : (p.labs.find((l) => String(l.id) === d.labHost) ?? null)
  const catalogOk = p.catalog?.ok === true
  const offered = catalogOk ? artifactsFor(delivery, p.catalog?.artifacts ?? []) : []
  const manifest = catalogOk && p.catalog?.mode === 'manifest'
  const artifact = catalogOk ? findArtifact(p.catalog?.artifacts ?? [], d.image) : undefined
  const eventName = p.eventLab ? p.eventLab.name : "this server's own container runtime"
  return (
    <Section title="Delivery" help="How players get it. The lab host's manifest sets the runtime.">
      <div role="radiogroup" aria-label="Delivery" className="grid gap-3 sm:grid-cols-3">
        {WAYS.map((w) => (
          <button
            key={w.id}
            type="button"
            role="radio"
            aria-checked={delivery === w.id}
            onClick={() => p.onDelivery(w.id)}
            disabled={p.locked && w.id !== delivery}
            className={`flex flex-col items-start gap-1.5 rounded-card border p-3 text-left transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${delivery === w.id ? 'border-accent bg-accent/5' : 'border-line hover:border-field-line'}`}
          >
            <span className="flex items-center gap-2 text-sm font-semibold">
              {w.icon}
              {w.title}
            </span>
            <span className="text-xs text-muted">{w.text}</span>
          </button>
        ))}
      </div>
      {p.locked && <p className="text-xs text-muted">Locked: it has runs or solves.</p>}

      {delivery !== 'none' && (
        <>
          <Field
            label="Runs on"
            error={p.problems.labHost}
            hint={vm ? 'Machines run on a lab host that has virtual machines set up.' : "Where this challenge's instances start. Leave it on the event's choice, or pick a lab host for this challenge alone."}
          >
            <Select value={d.labHost} onChange={(e) => p.set('labHost', e.target.value)} aria-invalid={p.problems.labHost ? true : undefined}>
              <option value="">Same as the event ({eventName})</option>
              {p.labs
                .filter((l) => l.enabled || String(l.id) === d.labHost)
                .map((l) => (
                  <option key={l.id} value={l.id}>
                    {l.name}
                    {l.enabled ? '' : ' (switched off)'}
                  </option>
                ))}
            </Select>
          </Field>

          {vm && p.labs.length === 0 && (
            <p className="text-sm text-muted">
              No lab host exists yet. <a href="/organizer/labs" target="_blank" rel="noopener" className="font-medium text-accent hover:underline">Add one</a> (it opens in a new tab), then come back.
            </p>
          )}
          {p.loading && <p role="status" className="text-sm text-muted">Reading what {effective?.name ?? 'the lab host'} can run...</p>}
          {p.catalog && !p.catalog.ok && <p className="text-sm text-danger">Could not read the lab host: {p.catalog.error ?? 'no answer'}. You can still type the name below.</p>}
          {p.catalog?.ok && p.catalog.accel === 'tcg' && vm && <p className="text-sm text-danger">This lab host emulates machines slowly (no KVM). Expect long boot times.</p>}

          {manifest ? (
            <Field
              label={vm ? 'Machine' : 'Container image'}
              error={p.shown.image ?? p.problems.image}
              hint={offered.length === 0 ? `This lab host lists no ${vm ? 'machines' : 'containers'}. Add a manifest on the lab host.` : 'Listed by the lab host. Only these will start.'}
            >
              <Select value={d.image} onChange={(e) => { const a = offered.find((x) => x.artifact === e.target.value); if (a) p.onArtifact(a); else p.set('image', e.target.value) }} aria-invalid={p.shown.image || p.problems.image ? true : undefined}>
                <option value="">Choose...</option>
                {d.image !== '' && !artifact && <option value={d.image}>{d.image} (not on this lab host)</option>}
                {offered.map((a) => (
                  <option key={a.artifact} value={a.artifact}>
                    {a.artifact} · {runtimeLabel(a.runtime)}
                    {a.ready === false ? ' (not ready)' : ''}
                  </option>
                ))}
              </Select>
            </Field>
          ) : (
            <Field
              label={vm ? 'Machine name' : 'Container image'}
              error={p.shown.image ?? p.problems.image}
              hint={d.responder ? "The artifact name of the lab host's manifest for this image. That manifest must define a verifier." : 'A name the runtime can already run, for example registry.example.org/ctf/web:1.0.'}
            >
              <Input value={d.image} onChange={(e) => p.set('image', e.target.value)} placeholder={vm ? 'kymera' : 'registry.example.org/ctf/web-1:1.0'} className="font-mono" autoComplete="off" spellCheck={false} aria-invalid={p.shown.image || p.problems.image ? true : undefined} />
            </Field>
          )}

          {artifact && <ArtifactSummary a={artifact} />}

          {vm ? (
            <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
              <Num label="Lifetime" value={d.ttl} onChange={(v) => p.set('ttl', v)} min={1} error={p.shown.ttl} hint="Seconds." />
              <Num label="CPU" value={d.cpu} onChange={(v) => p.set('cpu', v)} min={1} error={p.shown.cpu} hint={artifact ? "Millicores. The manifest's value." : 'Millicores. At most what the manifest gives.'} />
              <Num label="Memory" value={d.memory} onChange={(v) => p.set('memory', v)} min={1} error={p.shown.memory} hint={artifact ? "MB. The manifest's value." : 'MB. At most what the manifest gives.'} />
              <Num label="Port" value={d.port} onChange={(v) => p.set('port', v)} min={1} error={p.shown.port} hint={artifact ? 'From the manifest.' : 'The first service port.'} />
            </div>
          ) : (
            <div className="grid grid-cols-2 gap-4 sm:grid-cols-5">
              <Num label="Port" value={d.port} onChange={(v) => p.set('port', v)} min={1} error={p.shown.port} hint="The service's port." />
              <Num label="CPU" value={d.cpu} onChange={(v) => p.set('cpu', v)} min={1} error={p.shown.cpu} hint="Millicores." />
              <Num label="Memory" value={d.memory} onChange={(v) => p.set('memory', v)} min={1} error={p.shown.memory} hint="MB." />
              <Num label="Processes" value={d.pids} onChange={(v) => p.set('pids', v)} min={1} error={p.shown.pids} hint="At most." />
              <Num label="Lifetime" value={d.ttl} onChange={(v) => p.set('ttl', v)} min={1} error={p.shown.ttl} hint="Seconds." />
            </div>
          )}
          <p className="text-xs text-muted">The lab host runs the smaller of these numbers and what the manifest allows.</p>

          {!p.tournament && (
            <div className="flex flex-col gap-1">
              <Toggle label="Responder: players fix the machine and a Verifier gives the flag" checked={d.responder} onChange={p.onResponder} disabled={p.locked} />
              {d.responder && artifact && !artifact.verifier && <p className="text-xs text-danger">This manifest has no verifier block, so no Verifier would start.</p>}
              {d.responder && (
                <p className="text-xs text-muted">The event must run on the private VPN with a lab host. The Verifier shows the flag once every check passes. One flag only.</p>
              )}
              {d.responder && d.flagMode === 'static' && (
                <p className="text-xs text-danger">A static flag is the same for everyone: the first player to pass can share it. Make it dynamic so each player gets their own.</p>
              )}
            </div>
          )}
          {p.shown.requiresInstance && <p className="text-sm text-danger">{p.shown.requiresInstance}</p>}
        </>
      )}
    </Section>
  )
}

function ArtifactSummary({ a }: { a: LabArtifact }) {
  const ports = a.ports && a.ports.length > 0 ? a.ports : a.port > 0 ? [{ name: 'main', port: a.port }] : []
  return (
    <div className="flex flex-col gap-1.5 rounded-control border border-line bg-bg p-3 text-sm">
      <div className="flex flex-wrap items-center gap-2">
        <span className="rounded-full border border-line px-2.5 py-0.5 text-xs font-medium">{runtimeLabel(a.runtime)}</span>
        {a.ready === false && <span className="text-xs text-danger">{a.note ?? 'Not ready yet.'}</span>}
        <span className="text-xs text-muted">Up to {Math.round(a.max_ttl_secs / 60)} min</span>
      </div>
      {ports.length > 0 && (
        <p className="text-xs text-muted">
          Services: {ports.map((x) => `${x.name} ${x.port}`).join(' · ')}
        </p>
      )}
      <p className="text-xs text-muted">{displayText(a)}</p>
      {a.flags && a.flags.length > 0 && <p className="text-xs text-muted">Delivers flags named: <span className="font-mono text-ink">{a.flags.join(', ')}</span></p>}
    </div>
  )
}
