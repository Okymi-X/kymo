import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Paperclip, Trash2 } from 'lucide-react'
import { useRef, useState, type ReactNode } from 'react'
import { platform } from '../../api/platform'
import type { AdminChallenge, Category, ChallengeBody, FlagSpec, Scoring } from '../../api/platformTypes'
import { Modal, ModalBody, ModalCancel, ModalFooter, ModalHeader, modalForm } from '../../components/Modal'
import { Pill, Toggle } from '../../components/platform/bits'
import { Button, ErrorText, Field, Input, Select, Textarea } from '../../components/ui'
import { fileSize, flagHint, fromLocalInput, toLocalInput, verdictText } from '../../lib/platform'

export interface Draft {
  title: string
  description: string
  category_id: number
  scoring: 'static' | 'dynamic'
  points: string
  initial: string
  minimum: string
  decay: string
  curve: 'linear' | 'parabolic' | 'logarithmic'
  flagKind: FlagSpec['kind']
  flagValue: string
  prefix: string
  suffix: string
  caseSensitive: boolean
  maxAttempts: string
  release: string
  retire: string
  position: string
  requiresInstance: boolean
  image: string
  port: string
  cpu: string
  memory: string
  pids: string
  ttl: string
}

export const blank = (category: number): Draft => ({
  title: '',
  description: '',
  category_id: category,
  scoring: 'static',
  points: '100',
  initial: '500',
  minimum: '100',
  decay: '20',
  curve: 'parabolic',
  flagKind: 'static',
  flagValue: '',
  prefix: '',
  suffix: '',
  caseSensitive: true,
  maxAttempts: '',
  release: '',
  retire: '',
  position: '0',
  requiresInstance: false,
  image: '',
  port: '8000',
  cpu: '500',
  memory: '256',
  pids: '128',
  ttl: '1800',
})

function flagText(f: FlagSpec): string {
  return f.kind === 'static' ? f.value : f.kind === 'regex' ? f.pattern : f.expected
}

export function fromChallenge(c: AdminChallenge): Draft {
  const cfg = c.instance_config
  return {
    ...blank(c.category_id),
    title: c.title,
    description: c.description,
    scoring: c.scoring.kind,
    points: c.scoring.kind === 'static' ? String(c.scoring.points) : '100',
    initial: c.scoring.kind === 'dynamic' ? String(c.scoring.initial) : '500',
    minimum: c.scoring.kind === 'dynamic' ? String(c.scoring.minimum) : '100',
    decay: c.scoring.kind === 'dynamic' ? String(c.scoring.decay) : '20',
    curve: c.scoring.kind === 'dynamic' ? c.scoring.curve : 'parabolic',
    flagKind: c.flag.kind,
    flagValue: flagText(c.flag),
    prefix: c.flag_prefix,
    suffix: c.flag_suffix,
    caseSensitive: c.case_sensitive,
    maxAttempts: c.max_attempts === null ? '' : String(c.max_attempts),
    release: toLocalInput(c.release_at),
    retire: toLocalInput(c.retire_at),
    position: String(c.position),
    requiresInstance: c.requires_instance,
    image: cfg?.image ?? '',
    port: String(cfg?.port ?? 8000),
    cpu: String(cfg?.cpu_millis ?? 500),
    memory: String(cfg?.memory_mb ?? 256),
    pids: String(cfg?.pids ?? 128),
    ttl: String(cfg?.ttl_secs ?? 1800),
  }
}

const int = (s: string) => Math.trunc(Number(s))

/** Turns the form into the API body; the server validates every field again. */
export function toBody(d: Draft): ChallengeBody {
  const scoring: Scoring =
    d.scoring === 'static'
      ? { kind: 'static', points: int(d.points) }
      : { kind: 'dynamic', initial: int(d.initial), minimum: int(d.minimum), decay: int(d.decay), curve: d.curve }
  const flag: FlagSpec =
    d.flagKind === 'static' ? { kind: 'static', value: d.flagValue } : d.flagKind === 'regex' ? { kind: 'regex', pattern: d.flagValue } : { kind: 'per_team', expected: d.flagValue }
  return {
    category_id: d.category_id,
    title: d.title.trim(),
    description: d.description,
    scoring,
    flag,
    flag_prefix: d.prefix,
    flag_suffix: d.suffix,
    case_sensitive: d.caseSensitive,
    max_attempts: d.maxAttempts.trim() === '' ? null : int(d.maxAttempts),
    release_at: fromLocalInput(d.release),
    retire_at: fromLocalInput(d.retire),
    position: int(d.position) || 0,
    requires_instance: d.requiresInstance,
    instance_config: d.requiresInstance
      ? { image: d.image.trim(), port: int(d.port), cpu_millis: int(d.cpu), memory_mb: int(d.memory), pids: int(d.pids), ttl_secs: int(d.ttl) }
      : null,
  }
}

export type Problems = Partial<Record<keyof Draft, string>>

const inRange = (value: string, min: number, max: number) => {
  const n = Number(value)
  return value.trim() !== '' && Number.isInteger(n) && n >= min && n <= max
}

/** What would be refused, found before the request is sent so the form can say why Save is off. */
export function validate(d: Draft): Problems {
  const p: Problems = {}
  if (d.title.trim() === '') p.title = 'Give the challenge a title.'
  if (d.category_id === 0) p.category_id = 'Add a category first.'
  if (d.scoring === 'static') {
    if (!inRange(d.points, 0, 100000)) p.points = 'Points are a whole number, 0 or more.'
  } else {
    if (!inRange(d.initial, 1, 100000)) p.initial = 'A whole number, 1 or more.'
    if (!inRange(d.minimum, 0, 100000)) p.minimum = 'A whole number, 0 or more.'
    else if (inRange(d.initial, 1, 100000) && Number(d.minimum) > Number(d.initial)) p.minimum = 'The floor cannot be above the start.'
    if (!inRange(d.decay, 1, 100000)) p.decay = 'A whole number, 1 or more.'
  }
  if (d.flagValue === '') {
    p.flagValue = 'Enter the flag.'
  } else if (d.flagKind === 'static' || d.flagKind === 'per_team') {
    // A flag that does not match the accepted format could never be submitted.
    const shown = d.caseSensitive ? d.flagValue : d.flagValue.toLowerCase()
    const pre = d.caseSensitive ? d.prefix : d.prefix.toLowerCase()
    const suf = d.caseSensitive ? d.suffix : d.suffix.toLowerCase()
    if (d.prefix && !shown.startsWith(pre)) p.flagValue = `The flag must start with the accepted prefix ${d.prefix}, or nobody can submit it.`
    else if (d.suffix && !shown.endsWith(suf)) p.flagValue = `The flag must end with the accepted suffix ${d.suffix}, or nobody can submit it.`
  } else {
    try {
      new RegExp(d.flagValue)
    } catch {
      p.flagValue = 'This is not a valid pattern.'
    }
  }
  if (d.maxAttempts.trim() !== '' && !inRange(d.maxAttempts, 1, 10000)) p.maxAttempts = 'A whole number from 1, or empty for no limit.'
  const rel = fromLocalInput(d.release)
  const ret = fromLocalInput(d.retire)
  if (rel !== null && ret !== null && rel >= ret) p.retire = 'It must retire after it is released.'
  if (d.requiresInstance) {
    if (d.image.trim() === '') p.image = 'Name the container image.'
    if (!inRange(d.port, 1, 65535)) p.port = '1 to 65535.'
    if (!inRange(d.cpu, 1, 4000)) p.cpu = '1 to 4000.'
    if (!inRange(d.memory, 1, 2048)) p.memory = '1 to 2048.'
    if (!inRange(d.pids, 1, 512)) p.pids = '1 to 512.'
    if (!inRange(d.ttl, 1, 14400)) p.ttl = '1 to 14400 (4 hours).'
  }
  return p
}

function Section({ title, help, children }: { title: string; help?: ReactNode; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-4 rounded-card border border-line p-4">
      <div>
        <h3 className="text-sm font-semibold">{title}</h3>
        {help && <p className="mt-0.5 text-xs text-muted">{help}</p>}
      </div>
      {children}
    </section>
  )
}

function Num({ label, value, onChange, error, hint, min = 0 }: { label: string; value: string; onChange: (v: string) => void; error?: string; hint?: ReactNode; min?: number }) {
  return (
    <Field label={label} error={error} hint={hint}>
      <Input type="number" min={min} value={value} onChange={(e) => onChange(e.target.value)} aria-invalid={error ? true : undefined} className="font-mono" />
    </Field>
  )
}

/** A date and time with a clear button, so an optional limit can be taken away again. */
function When({ label, value, onChange, error, hint }: { label: string; value: string; onChange: (v: string) => void; error?: string; hint?: ReactNode }) {
  return (
    <Field label={label} error={error} hint={hint}>
      <span className="flex gap-2">
        <Input type="datetime-local" value={value} onChange={(e) => onChange(e.target.value)} aria-invalid={error ? true : undefined} className="min-w-0 flex-1" />
        {value && (
          <Button variant="ghost" onClick={() => onChange('')} aria-label={`Clear ${label.toLowerCase()}`}>
            Clear
          </Button>
        )}
      </span>
    </Field>
  )
}

/** One sentence that says what the dynamic settings mean in practice. */
function dynamicSummary(d: Draft): string {
  const start = d.initial || '?'
  const floor = d.minimum || '?'
  const n = d.decay || '?'
  if (d.curve === 'linear') return `The first solver gets ${start}. Each solve after that takes ${n} points off, down to ${floor}.`
  return `The first solver gets ${start}, falling ${d.curve === 'parabolic' ? 'faster at first' : 'quickly then slowly'} to ${floor} after about ${n} solves. Everyone's points update as more teams solve it.`
}

function Extras({ challenge, onChanged }: { challenge: AdminChallenge; onChanged: () => void }) {
  const [hint, setHint] = useState('')
  const [cost, setCost] = useState('10')
  const addHint = useMutation({
    mutationFn: () => platform.addHint(challenge.id, hint.trim(), int(cost)),
    onSuccess: () => {
      setHint('')
      onChanged()
    },
  })
  const delHint = useMutation({ mutationFn: (id: number) => platform.deleteHint(id), onSuccess: onChanged })
  const file = useRef<HTMLInputElement>(null)
  const upload = useMutation({ mutationFn: (f: File) => platform.uploadAttachment(challenge.id, f), onSuccess: onChanged })
  const delFile = useMutation({ mutationFn: (id: number) => platform.deleteAttachment(id), onSuccess: onChanged })
  const [probe, setProbe] = useState('')
  const test = useMutation({ mutationFn: () => platform.testFlag(challenge.id, probe) })
  const costOk = inRange(cost, 0, 100000)
  return (
    <>
      <Section title="Hints" help="Players can unlock a hint for its cost, which comes off the challenge when they solve it. Changes here save at once.">
        {challenge.hints.length === 0 && <p className="text-sm text-muted">No hints yet.</p>}
        {challenge.hints.map((h) => (
          <div key={h.id} className="flex items-start gap-2 rounded-control border border-line p-2.5 text-sm">
            <span className="min-w-0 flex-1 whitespace-pre-wrap break-words">{h.content}</span>
            <span className="font-mono text-xs text-muted">{h.cost} pts</span>
            <button type="button" aria-label="Delete hint" onClick={() => delHint.mutate(h.id)} className="text-muted hover:text-danger">
              <Trash2 size={15} aria-hidden="true" />
            </button>
          </div>
        ))}
        <div className="grid grid-cols-[1fr_6rem_auto] items-end gap-2 max-sm:grid-cols-1">
          <Field label="New hint">
            <Input value={hint} onChange={(e) => setHint(e.target.value)} placeholder="What should a stuck player try?" />
          </Field>
          <Field label="Cost">
            <Input type="number" min={0} value={cost} onChange={(e) => setCost(e.target.value)} aria-label="Hint cost" className="font-mono" />
          </Field>
          <Button onClick={() => addHint.mutate()} disabled={!hint.trim() || !costOk || addHint.isPending}>
            Add hint
          </Button>
        </div>
        <ErrorText error={addHint.error ?? delHint.error} />
      </Section>

      <Section title="Attachments" help="Files players download from the challenge, up to 10 MB each. Changes here save at once.">
        {challenge.attachments.length === 0 && <p className="text-sm text-muted">No files yet.</p>}
        {challenge.attachments.map((a) => (
          <div key={a.id} className="flex items-center gap-2 rounded-control border border-line p-2.5 text-sm">
            <Paperclip size={14} aria-hidden="true" className="text-muted" />
            <span className="min-w-0 flex-1 truncate">{a.name}</span>
            <span className="font-mono text-xs text-muted">{fileSize(a.size)}</span>
            <button type="button" aria-label={`Delete ${a.name}`} onClick={() => delFile.mutate(a.id)} className="text-muted hover:text-danger">
              <Trash2 size={15} aria-hidden="true" />
            </button>
          </div>
        ))}
        <input
          ref={file}
          type="file"
          className="sr-only"
          aria-label="Attachment file"
          onChange={(e) => {
            const f = e.target.files?.[0]
            if (f) upload.mutate(f)
            e.target.value = ''
          }}
        />
        <Button className="self-start" onClick={() => file.current?.click()} disabled={upload.isPending}>
          {upload.isPending ? 'Uploading...' : 'Upload a file'}
        </Button>
        <ErrorText error={upload.error ?? delFile.error} />
      </Section>

      <Section title="Test the flag" help="Checks a flag the way a player's submission is checked, without recording a submission or a solve.">
        <div className="flex gap-2">
          <Input value={probe} onChange={(e) => setProbe(e.target.value)} placeholder="Try a flag" aria-label="Flag to test" className="min-w-0 flex-1 font-mono" autoComplete="off" />
          <Button onClick={() => test.mutate()} disabled={!probe || test.isPending}>
            Test
          </Button>
        </div>
        {test.data && <p className={test.data.verdict === 'correct' ? 'text-sm text-win' : 'text-sm text-danger'}>{verdictText[test.data.verdict]}</p>}
        <ErrorText error={test.error} />
      </Section>
    </>
  )
}

interface Props {
  eventId: number
  categories: Category[]
  challenge: AdminChallenge | null
  onClose: () => void
}

export function ChallengeEditor({ eventId, categories, challenge, onClose }: Props) {
  const client = useQueryClient()
  const [start] = useState<Draft>(() => (challenge ? fromChallenge(challenge) : blank(categories[0]?.id ?? 0)))
  const [d, setD] = useState<Draft>(start)
  const set = <K extends keyof Draft>(key: K, value: Draft[K]) => setD((prev) => ({ ...prev, [key]: value }))
  const dirty = JSON.stringify(d) !== JSON.stringify(start)
  const problems = validate(d)
  const first = Object.values(problems)[0]
  // Show a field's problem only after it has been touched, so a new form does not open in red.
  const [touched, setTouched] = useState(false)
  const shown: Problems = touched || dirty ? problems : {}

  const refresh = () => void client.invalidateQueries({ queryKey: ['admin-challenges', eventId] })
  const save = useMutation({
    mutationFn: () => (challenge ? platform.updateChallenge(challenge.id, toBody(d)) : platform.createChallenge(eventId, toBody(d))),
    onSuccess: () => {
      refresh()
      onClose()
    },
  })
  const state = useMutation({
    mutationFn: (action: 'publish' | 'retire') => (action === 'publish' ? platform.publishChallenge(challenge!.id) : platform.retireChallenge(challenge!.id)),
    onSuccess: refresh,
  })

  return (
    <Modal label={challenge ? 'Edit challenge' : 'New challenge'} onClose={onClose} width="52rem" dirty={dirty} backdropCloses={false}>
      <form
        className={modalForm}
        onSubmit={(e) => {
          e.preventDefault()
          setTouched(true)
          if (!first) save.mutate()
        }}
      >
        <ModalHeader
          title={challenge ? 'Edit challenge' : 'New challenge'}
          subtitle={challenge ? `${challenge.solves} ${challenge.solves === 1 ? 'solve' : 'solves'}` : 'Players see it once you publish it.'}
          actions={
            challenge && (
              <>
                <Pill tone={challenge.state === 'published' ? 'text-win' : 'text-muted'}>{challenge.state}</Pill>
                {challenge.state !== 'published' && (
                  <Button onClick={() => state.mutate('publish')} disabled={state.isPending || dirty} title={dirty ? 'Save your changes first' : undefined}>
                    Publish
                  </Button>
                )}
                {challenge.state === 'published' && (
                  <Button variant="danger" onClick={() => state.mutate('retire')} disabled={state.isPending}>
                    Retire
                  </Button>
                )}
              </>
            )
          }
        />
        <ModalBody>
          <ErrorText error={state.error} />

          <Section title="Basics">
            <div className="grid gap-4 sm:grid-cols-[1fr_14rem]">
              <Field label="Title" error={shown.title}>
                <Input value={d.title} onChange={(e) => set('title', e.target.value)} maxLength={100} aria-invalid={shown.title ? true : undefined} />
              </Field>
              <Field label="Category" error={shown.category_id}>
                <Select value={d.category_id} onChange={(e) => set('category_id', int(e.target.value))}>
                  {categories.map((c) => (
                    <option key={c.id} value={c.id}>
                      {c.name}
                    </option>
                  ))}
                </Select>
              </Field>
            </div>
            <Field label="Description" hint="Shown to players as plain text. Say what to do, not how to solve it.">
              <Textarea value={d.description} onChange={(e) => set('description', e.target.value)} rows={4} />
            </Field>
          </Section>

          <Section title="Points">
            <div className="grid gap-4 sm:grid-cols-[14rem_1fr] sm:items-start">
              <Field label="Scoring">
                <Select value={d.scoring} onChange={(e) => set('scoring', e.target.value as Draft['scoring'])} aria-label="Scoring type">
                  <option value="static">Fixed points</option>
                  <option value="dynamic">Dynamic points</option>
                </Select>
              </Field>
              {d.scoring === 'static' ? (
                <Num label="Points" value={d.points} onChange={(v) => set('points', v)} error={shown.points} />
              ) : (
                <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
                  <Num label="Start at" value={d.initial} onChange={(v) => set('initial', v)} error={shown.initial} />
                  <Num label="Floor" value={d.minimum} onChange={(v) => set('minimum', v)} error={shown.minimum} />
                  <Num label={d.curve === 'linear' ? 'Lost per solve' : 'Solves to floor'} value={d.decay} onChange={(v) => set('decay', v)} error={shown.decay} />
                  <Field label="Curve">
                    <Select value={d.curve} onChange={(e) => set('curve', e.target.value as Draft['curve'])}>
                      <option value="parabolic">Parabolic</option>
                      <option value="linear">Linear</option>
                      <option value="logarithmic">Logarithmic</option>
                    </Select>
                  </Field>
                </div>
              )}
            </div>
            {d.scoring === 'dynamic' && <p className="text-xs text-muted">{dynamicSummary(d)}</p>}
          </Section>

          <Section title="Flag" help="What a player must submit, and the shape that is accepted before it is even compared.">
            <div className="grid gap-4 sm:grid-cols-[14rem_1fr]">
              <Field label="Kind">
                <Select value={d.flagKind} onChange={(e) => set('flagKind', e.target.value as FlagSpec['kind'])}>
                  <option value="static">Exact text</option>
                  <option value="regex">Pattern (regex)</option>
                  <option value="per_team">Per team</option>
                </Select>
              </Field>
              <Field label={d.flagKind === 'regex' ? 'Pattern' : 'Flag'} error={shown.flagValue} hint={d.flagKind === 'regex' ? 'Must match the whole submission.' : undefined}>
                <Input value={d.flagValue} onChange={(e) => set('flagValue', e.target.value)} className="font-mono" autoComplete="off" spellCheck={false} aria-invalid={shown.flagValue ? true : undefined} />
              </Field>
            </div>
            <div className="grid gap-4 sm:grid-cols-2">
              <Field label="Accepted prefix">
                <Input value={d.prefix} onChange={(e) => set('prefix', e.target.value)} placeholder="KYMO{" className="font-mono" />
              </Field>
              <Field label="Accepted suffix">
                <Input value={d.suffix} onChange={(e) => set('suffix', e.target.value)} placeholder="}" className="font-mono" />
              </Field>
            </div>
            <div className="flex flex-wrap items-center justify-between gap-3">
              <Toggle label="Case sensitive" checked={d.caseSensitive} onChange={(v) => set('caseSensitive', v)} />
              {flagHint(d.prefix, d.suffix) && <span className="text-xs text-muted">Players see the shape: <code className="font-mono text-ink">{flagHint(d.prefix, d.suffix)}</code></span>}
            </div>
          </Section>

          <Section title="Availability" help="When players can see and solve it, and how many wrong tries they get. Times use your browser's time zone.">
            <div className="grid gap-4 sm:grid-cols-2">
              <When label="Release at" value={d.release} onChange={(v) => set('release', v)} hint="Empty: visible as soon as it is published." />
              <When label="Retire at" value={d.retire} onChange={(v) => set('retire', v)} error={shown.retire} hint="Empty: stays open until the event ends." />
            </div>
            <div className="grid gap-4 sm:grid-cols-2">
              <Num label="Attempt limit" value={d.maxAttempts} onChange={(v) => set('maxAttempts', v)} min={1} error={shown.maxAttempts} hint="Empty means unlimited. Wrong-format answers never count." />
              <Num label="Order" value={d.position} onChange={(v) => set('position', v)} min={-1000} hint="Lower numbers come first in the category." />
            </div>
          </Section>

          <Section title="Instance" help="Give every player or team their own copy of a service to attack. Turn it off for a plain flag.">
            <Toggle label="Each player or team starts its own container" checked={d.requiresInstance} onChange={(v) => set('requiresInstance', v)} />
            {d.requiresInstance && (
              <>
                <Field label="Container image" error={shown.image} hint="A name the container platform can already run, for example registry.example.org/ctf/web:1.0.">
                  <Input value={d.image} onChange={(e) => set('image', e.target.value)} placeholder="registry.example.org/ctf/web-1:1.0" className="font-mono" aria-invalid={shown.image ? true : undefined} />
                </Field>
                <div className="grid grid-cols-2 gap-4 sm:grid-cols-5">
                  <Num label="Port" value={d.port} onChange={(v) => set('port', v)} min={1} error={shown.port} hint="The service's port." />
                  <Num label="CPU" value={d.cpu} onChange={(v) => set('cpu', v)} min={1} error={shown.cpu} hint="Millicores." />
                  <Num label="Memory" value={d.memory} onChange={(v) => set('memory', v)} min={1} error={shown.memory} hint="MB." />
                  <Num label="Processes" value={d.pids} onChange={(v) => set('pids', v)} min={1} error={shown.pids} hint="At most." />
                  <Num label="Lifetime" value={d.ttl} onChange={(v) => set('ttl', v)} min={1} error={shown.ttl} hint="Seconds." />
                </div>
              </>
            )}
          </Section>

          {challenge ? (
            <Extras challenge={challenge} onChanged={refresh} />
          ) : (
            <p className="rounded-card border border-dashed border-line p-4 text-sm text-muted">Hints, attachments and a flag test appear here once the challenge is created.</p>
          )}
          <ErrorText error={save.error} />
        </ModalBody>
        <ModalFooter note={first && (touched || dirty) ? <span className="text-danger">{first}</span> : dirty ? 'Unsaved changes' : null}>
          <ModalCancel />
          <Button type="submit" variant="primary" disabled={save.isPending || (challenge !== null && !dirty) || (dirty && first !== undefined)}>
            {challenge ? 'Save changes' : 'Create challenge'}
          </Button>
        </ModalFooter>
      </form>
    </Modal>
  )
}
