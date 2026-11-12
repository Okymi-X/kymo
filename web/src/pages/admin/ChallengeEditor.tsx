import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Paperclip, Trash2 } from 'lucide-react'
import { useRef, useState, type ReactNode } from 'react'
import { platform } from '../../api/platform'
import type { AdminChallenge, Category, ChallengeBody, FlagInput, FlagSpec, Scoring } from '../../api/platformTypes'
import { Modal, ModalBody, ModalCancel, ModalFooter, ModalHeader, modalForm } from '../../components/Modal'
import { Pill, Toggle } from '../../components/platform/bits'
import { Button, ErrorText, Field, Input, Select, Textarea } from '../../components/ui'
import { blankRow, exampleFlag, manifestSkeleton, MAX_FLAGS, resizeRows, rowProblems, splitPoints, type FlagRowDraft } from '../../lib/flags'
import { fileSize, flagHint, fromLocalInput, toLocalInput, verdictText } from '../../lib/platform'
import { DfirItems } from './DfirItems'

export interface Draft {
  /** `flag` (also on a container) or `dfir`, answered question by question. */
  kind: 'flag' | 'dfir'
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
  /** `static` (the flag above) or `dynamic` (generated for every instance run). */
  flagMode: 'static' | 'dynamic'
  /** How many flags; more than one makes a multi flag challenge. */
  flagCount: string
  /** The flags of a dynamic or multi flag challenge. */
  rows: FlagRowDraft[]
  /** Players fix the machine and a Verifier gives the flag. */
  responder: boolean
  requiresInstance: boolean
  image: string
  port: string
  cpu: string
  memory: string
  pids: string
  ttl: string
}

export const blank = (category: number): Draft => ({
  kind: 'flag',
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
  flagMode: 'static',
  flagCount: '1',
  rows: [blankRow('flag')],
  responder: false,
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
    kind: c.kind,
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
    flagMode: c.flag_mode ?? 'static',
    flagCount: String(Math.max(1, c.flags?.length ?? 0)),
    rows:
      c.flags && c.flags.length > 0
        ? c.flags.map((f) => ({ name: f.name, label: f.label, value: f.value, randomLen: String(f.random_len), closing: f.closing, weight: String(f.weight) }))
        : [blankRow('flag', c.flag_suffix)],
    responder: c.responder ?? false,
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

/** The flags come from the rows, not from the single flag field. */
export const usesRows = (d: Draft): boolean => d.kind === 'flag' && (d.flagMode === 'dynamic' || int(d.flagCount) > 1)
/** Several flags: static points, split by weight. */
export const isMulti = (d: Draft): boolean => d.kind === 'flag' && int(d.flagCount) > 1

/** Turns the form into the API body; the server validates every field again. */
export function toBody(d: Draft, fixedPoints = false): ChallengeBody {
  const dfir = d.kind === 'dfir'
  const scoring: Scoring =
    d.scoring === 'static' || fixedPoints || dfir || isMulti(d)
      ? { kind: 'static', points: int(d.points) }
      : { kind: 'dynamic', initial: int(d.initial), minimum: int(d.minimum), decay: int(d.decay), curve: d.curve }
  const flag: FlagSpec =
    d.flagKind === 'static' ? { kind: 'static', value: d.flagValue } : d.flagKind === 'regex' ? { kind: 'regex', pattern: d.flagValue } : { kind: 'per_team', expected: d.flagValue }
  const rows = usesRows(d)
  const flags: FlagInput[] = rows
    ? d.rows.slice(0, Math.max(1, int(d.flagCount))).map((r) => ({
        name: r.name.trim(),
        label: r.label.trim(),
        value: r.value,
        ...(d.flagMode === 'dynamic' ? { random_len: int(r.randomLen), closing: r.closing } : {}),
        weight: int(r.weight) || 1,
      }))
    : []
  return {
    category_id: d.category_id,
    title: d.title.trim(),
    description: d.description,
    scoring,
    kind: d.kind,
    ...(dfir || rows ? {} : { flag }),
    ...(dfir ? {} : { flag_mode: d.flagMode, responder: d.responder && d.requiresInstance, flags }),
    flag_prefix: d.prefix,
    flag_suffix: d.suffix,
    case_sensitive: d.caseSensitive,
    max_attempts: d.maxAttempts.trim() === '' ? null : int(d.maxAttempts),
    release_at: fromLocalInput(d.release),
    retire_at: fromLocalInput(d.retire),
    position: int(d.position) || 0,
    requires_instance: !dfir && d.requiresInstance,
    instance_config: !dfir && d.requiresInstance
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
export function validate(d: Draft, fixedPoints = false): Problems {
  const p: Problems = {}
  const dfir = d.kind === 'dfir'
  if (d.title.trim() === '') p.title = 'Give the challenge a title.'
  if (d.category_id === 0) p.category_id = 'Add a category first.'
  if (d.scoring === 'static' || fixedPoints || dfir || isMulti(d)) {
    if (!inRange(d.points, 0, 100000)) p.points = 'Points are a whole number, 0 or more.'
  } else {
    if (!inRange(d.initial, 1, 100000)) p.initial = 'A whole number, 1 or more.'
    if (!inRange(d.minimum, 0, 100000)) p.minimum = 'A whole number, 0 or more.'
    else if (inRange(d.initial, 1, 100000) && Number(d.minimum) > Number(d.initial)) p.minimum = 'The floor cannot be above the start.'
    if (!inRange(d.decay, 1, 100000)) p.decay = 'A whole number, 1 or more.'
  }
  const rows = usesRows(d)
  if (!dfir && (d.flagMode === 'dynamic' || d.responder) && !d.requiresInstance) {
    p.requiresInstance = d.responder ? 'A Responder needs an instance.' : 'Dynamic flags need an instance.'
  }
  const count = int(d.flagCount)
  if (!dfir && (!Number.isInteger(count) || count < 1 || count > MAX_FLAGS)) p.flagCount = `1 to ${MAX_FLAGS}.`
  if (d.responder && count > 1) p.flagCount = 'A Responder has exactly one flag.'
  if (dfir) {
    // The answers live in the questions.
  } else if (rows) {
    const used = d.rows.slice(0, Math.max(1, Math.min(count, MAX_FLAGS)))
    const names = used.map((r) => r.name)
    const bad = used.map((r) => rowProblems(r, d.flagMode, d.prefix, d.suffix, d.caseSensitive, names))
    const first = bad.map((b) => Object.values(b)[0]).find(Boolean)
    if (first) p.rows = first
  } else if (d.flagValue === '') {
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
  if (d.requiresInstance && !dfir) {
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

/** The flags of a dynamic or multi flag challenge: one card each, with the share of the points a weight gives. */
function FlagRows({ d, set, shown, locked }: { d: Draft; set: <K extends keyof Draft>(key: K, value: Draft[K]) => void; shown?: string; locked: boolean }) {
  const dynamic = d.flagMode === 'dynamic'
  const count = Math.max(1, Math.min(MAX_FLAGS, int(d.flagCount) || 1))
  const rows = d.rows.slice(0, count)
  const names = rows.map((r) => r.name)
  const several = count > 1
  const shares = splitPoints(int(d.points), rows.map((r) => int(r.weight) || 1))
  const update = (i: number, patch: Partial<FlagRowDraft>) => set('rows', d.rows.map((r, j) => (j === i ? { ...r, ...patch } : r)))
  const skeleton = manifestSkeleton(rows, d.responder)
  const [copied, setCopied] = useState(false)
  return (
    <div className="flex flex-col gap-3">
      {rows.map((r, i) => {
        const bad = shown === undefined ? {} : rowProblems(r, d.flagMode, d.prefix, d.suffix, d.caseSensitive, names)
        return (
          <div key={i} className="flex flex-col gap-3 rounded-control border border-line p-3" role="group" aria-label={`Flag ${i + 1}`}>
            <div className="grid gap-3 sm:grid-cols-[10rem_1fr_6rem]">
              <Field label="Name" error={bad.name} hint="Matches the lab manifest.">
                <Input value={r.name} onChange={(e) => update(i, { name: e.target.value })} disabled={locked} className="font-mono" autoComplete="off" spellCheck={false} aria-invalid={bad.name ? true : undefined} />
              </Field>
              <Field label="Label" hint="Only you see it.">
                <Input value={r.label} onChange={(e) => update(i, { label: e.target.value })} placeholder={i === 0 ? 'User flag' : ''} maxLength={60} />
              </Field>
              {several && (
                <Field label="Weight" error={bad.weight} hint={Number.isFinite(shares[i]) && d.points !== '' ? `${shares[i]} pts` : undefined}>
                  <Input type="number" min={1} value={r.weight} onChange={(e) => update(i, { weight: e.target.value })} disabled={locked} className="font-mono" aria-invalid={bad.weight ? true : undefined} />
                </Field>
              )}
            </div>
            <div className={dynamic ? 'grid gap-3 sm:grid-cols-[1fr_7rem_8rem]' : ''}>
              <Field label={dynamic ? 'Flag prefix' : 'Flag'} error={bad.value} hint={dynamic ? 'The readable start, for example KYMO{box_.' : undefined}>
                <Input value={r.value} onChange={(e) => update(i, { value: e.target.value })} disabled={locked} className="font-mono" autoComplete="off" spellCheck={false} aria-invalid={bad.value ? true : undefined} />
              </Field>
              {dynamic && (
                <>
                  <Field label="Random length" error={bad.randomLen} hint="Hex characters.">
                    <Input type="number" min={4} max={32} value={r.randomLen} onChange={(e) => update(i, { randomLen: e.target.value })} disabled={locked} className="font-mono" aria-invalid={bad.randomLen ? true : undefined} />
                  </Field>
                  <Field label="Closing" error={bad.closing} hint="After the random part.">
                    <Input value={r.closing} onChange={(e) => update(i, { closing: e.target.value })} disabled={locked} className="font-mono" autoComplete="off" spellCheck={false} aria-invalid={bad.closing ? true : undefined} />
                  </Field>
                </>
              )}
            </div>
            {dynamic && r.value && (
              <p className="text-xs text-muted">
                A player gets, for example: <code className="font-mono text-ink">{exampleFlag(r.value, int(r.randomLen), r.closing)}</code>
              </p>
            )}
          </div>
        )
      })}
      {d.requiresInstance && (
        <details className="rounded-control border border-line p-3 text-sm">
          <summary className="cursor-pointer font-medium">Lab manifest block for {d.responder ? 'the Verifier' : 'these flags'}</summary>
          <p className="mt-2 text-xs text-muted">
            {d.responder ? 'Add this to the lab host manifest of the image. The definition names a directory of Verifier checks.' : 'Add this to the lab host manifest of the image, with the right variable names or file paths for your image.'}
          </p>
          <pre className="mt-2 overflow-x-auto rounded-control border border-line bg-bg p-2 font-mono text-xs">{skeleton}</pre>
          <Button
            className="mt-2"
            onClick={() => {
              void navigator.clipboard?.writeText(skeleton).then(() => {
                setCopied(true)
                window.setTimeout(() => setCopied(false), 1500)
              })
            }}
          >
            {copied ? 'Copied' : 'Copy block'}
          </Button>
        </details>
      )}
      {shown && <p className="text-sm text-danger">{shown}</p>}
    </div>
  )
}

function Extras({ challenge, tournament, onChanged }: { challenge: AdminChallenge; tournament: boolean; onChanged: () => void }) {
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
      {!tournament && (
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
      )}

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

      {challenge.kind !== 'dfir' && (challenge.flag_mode === 'dynamic' || challenge.multi_flag) && (
      <Section title="Test the flag" help="Dynamic flags are checked against a player's own instance, so there is nothing to test here. Start an instance as a player, or read a run's flag on the Instances page.">
        <p className="text-sm text-muted">Not available for this challenge.</p>
      </Section>
      )}
      {challenge.kind !== 'dfir' && challenge.flag_mode !== 'dynamic' && !challenge.multi_flag && (
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
      )}
    </>
  )
}

interface Props {
  eventId: number
  /** A tournament's challenges are worth fixed points and have no hints. */
  eventKind: 'jeopardy' | 'tournament'
  categories: Category[]
  challenge: AdminChallenge | null
  onClose: () => void
}

export function ChallengeEditor({ eventId, eventKind, categories, challenge, onClose }: Props) {
  const tournament = eventKind === 'tournament'
  const client = useQueryClient()
  const [start] = useState<Draft>(() => (challenge ? fromChallenge(challenge) : blank(categories[0]?.id ?? 0)))
  const [d, setD] = useState<Draft>(start)
  const set = <K extends keyof Draft>(key: K, value: Draft[K]) => setD((prev) => ({ ...prev, [key]: value }))
  const dirty = JSON.stringify(d) !== JSON.stringify(start)
  const dfir = d.kind === 'dfir'
  const multi = isMulti(d)
  const rowsMode = usesRows(d)
  const fixedPoints = tournament || dfir || multi
  // The flags are fixed once a run or a solve exists (the server enforces it for runs too).
  const flagsLocked = challenge !== null && challenge.solves > 0
  const closing = d.suffix
  const setFlagMode = (mode: Draft['flagMode']) =>
    setD((prev) => ({
      ...prev,
      flagMode: mode,
      // A dynamic flag needs its own closing text: start from the accepted suffix.
      rows: mode === 'dynamic' ? prev.rows.map((r) => (r.closing === '' ? { ...r, closing } : r)) : prev.rows,
    }))
  const setFlagCount = (value: string) =>
    setD((prev) => ({ ...prev, flagCount: value, rows: resizeRows(prev.rows, Math.trunc(Number(value)) || 1, prev.suffix) }))
  const setResponder = (on: boolean) =>
    setD((prev) => ({ ...prev, responder: on, flagCount: on ? '1' : prev.flagCount, rows: on ? resizeRows(prev.rows, 1, prev.suffix) : prev.rows }))
  const problems = validate(d, tournament)
  const first = Object.values(problems)[0]
  // Show a field's problem only after it has been touched, so a new form does not open in red.
  const [touched, setTouched] = useState(false)
  const shown: Problems = touched || dirty ? problems : {}

  const refresh = () => void client.invalidateQueries({ queryKey: ['admin-challenges', eventId] })
  const save = useMutation({
    mutationFn: () => (challenge ? platform.updateChallenge(challenge.id, toBody(d, tournament)) : platform.createChallenge(eventId, toBody(d, tournament))),
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
          subtitle={challenge && !tournament ? `${challenge.solves} ${challenge.solves === 1 ? 'solve' : 'solves'}` : 'Players see it once you publish it.'}
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
            <div className="grid gap-4 sm:grid-cols-[1fr_14rem_12rem]">
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
              <Field label="Type" hint={challenge ? 'Fixed once created.' : dfir ? 'Several questions, each with its own answer and points.' : undefined}>
                <Select value={d.kind} onChange={(e) => set('kind', e.target.value as Draft['kind'])} disabled={challenge !== null}>
                  <option value="flag">Single flag</option>
                  <option value="dfir">Multi-question</option>
                </Select>
              </Field>
            </div>
            <Field label="Description" hint="Shown to players as plain text. Say what to do, not how to solve it.">
              <Textarea value={d.description} onChange={(e) => set('description', e.target.value)} rows={4} />
            </Field>
          </Section>

          {!dfir && (
          <Section title="Instance" help="Give every player or team their own copy of a service to attack. Turn it off for a plain flag.">
            <Toggle label="Each player or team starts its own container" checked={d.requiresInstance} onChange={(v) => set('requiresInstance', v)} />
            {shown.requiresInstance && <p className="text-sm text-danger">{shown.requiresInstance}</p>}
            {d.requiresInstance && (
              <>
                <Field label="Instance image" error={shown.image} hint={d.responder ? "The artifact name of the lab host's manifest for this image. That manifest must define a verifier." : 'A name the container platform can already run, for example registry.example.org/ctf/web:1.0. On a lab host, the artifact name of its manifest.'}>
                  <Input value={d.image} onChange={(e) => set('image', e.target.value)} placeholder="registry.example.org/ctf/web-1:1.0" className="font-mono" aria-invalid={shown.image ? true : undefined} />
                </Field>
                <div className="grid grid-cols-2 gap-4 sm:grid-cols-5">
                  <Num label="Port" value={d.port} onChange={(v) => set('port', v)} min={1} error={shown.port} hint="The service's port." />
                  <Num label="CPU" value={d.cpu} onChange={(v) => set('cpu', v)} min={1} error={shown.cpu} hint="Millicores." />
                  <Num label="Memory" value={d.memory} onChange={(v) => set('memory', v)} min={1} error={shown.memory} hint="MB." />
                  <Num label="Processes" value={d.pids} onChange={(v) => set('pids', v)} min={1} error={shown.pids} hint="At most." />
                  <Num label="Lifetime" value={d.ttl} onChange={(v) => set('ttl', v)} min={1} error={shown.ttl} hint="Seconds." />
                </div>
                {!tournament && (
                  <div className="flex flex-col gap-1">
                    <Toggle label="Responder: players fix the machine and a Verifier gives the flag" checked={d.responder} onChange={setResponder} disabled={flagsLocked} />
                    {d.responder && <p className="text-xs text-muted">The machine and the Verifier each get their own address on the player's private network. The flag is held by the lab host and shown by the Verifier only when every check passes. A Responder has exactly one flag.</p>}
                  </div>
                )}
              </>
            )}
          </Section>
          )}

          {!dfir && (
          <Section title={multi ? 'Flags' : 'Flag'} help="What a player must submit, and the shape that is accepted before it is even compared.">
            {!tournament && (
              <>
                <div className="grid gap-4 sm:grid-cols-[14rem_10rem_1fr] sm:items-start">
                  <Field label="Flag mode" hint={d.requiresInstance ? undefined : 'Dynamic needs an instance.'}>
                    <Select value={d.flagMode} onChange={(e) => setFlagMode(e.target.value as Draft['flagMode'])} aria-label="Flag mode" disabled={flagsLocked || (!d.requiresInstance && d.flagMode === 'static')}>
                      <option value="static">Static</option>
                      <option value="dynamic">Dynamic</option>
                    </Select>
                  </Field>
                  <Num label="Number of flags" value={d.flagCount} onChange={setFlagCount} min={1} error={shown.flagCount} />
                  <p className="text-xs text-muted sm:pt-6">
                    {d.flagMode === 'dynamic'
                      ? 'Kymo generates a different flag for every instance run: your prefix, a random part, your closing text. A flag from another player never works.'
                      : multi
                        ? 'Players find each flag once. Points are split between the flags by weight.'
                        : 'One flag for everybody.'}
                  </p>
                </div>
                {flagsLocked && <p className="text-xs text-muted">The flags can no longer change once the challenge has a run or a solve, only their labels.</p>}
              </>
            )}
            {rowsMode ? (
              <FlagRows d={d} set={set} shown={shown.rows} locked={flagsLocked} />
            ) : (
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
            )}
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
          )}
          {dfir && (challenge ? <DfirItems challenge={challenge} onChanged={refresh} /> : <p className="rounded-card border border-dashed border-line p-4 text-sm text-muted">Create the challenge first, then add its questions here. Each question has its own answer, points and attempt limit.</p>)}

          <Section title="Points">
            <div className="grid gap-4 sm:grid-cols-[14rem_1fr] sm:items-start">
              <Field label="Scoring" hint={fixedPoints ? (dfir ? 'Split between the questions.' : multi ? 'The total, split between the flags by weight.' : 'A match score is the points earned.') : undefined}>
                <Select value={fixedPoints ? 'static' : d.scoring} onChange={(e) => set('scoring', e.target.value as Draft['scoring'])} aria-label="Scoring type" disabled={fixedPoints}>
                  <option value="static">Fixed points</option>
                  <option value="dynamic">Dynamic points</option>
                </Select>
              </Field>
              {fixedPoints || d.scoring === 'static' ? (
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
            {!fixedPoints && d.scoring === 'dynamic' && <p className="text-xs text-muted">{dynamicSummary(d)}</p>}
          </Section>

          <Section title="Availability" help="When players can see and solve it, and how many wrong tries they get. Times use your browser's time zone.">
            <div className="grid gap-4 sm:grid-cols-2">
              <When label="Release at" value={d.release} onChange={(v) => set('release', v)} hint="Empty: visible as soon as it is published." />
              <When label="Retire at" value={d.retire} onChange={(v) => set('retire', v)} error={shown.retire} hint="Empty: stays open until the event ends." />
            </div>
            <div className="grid gap-4 sm:grid-cols-2">
              {!dfir && <Num label="Attempt limit" value={d.maxAttempts} onChange={(v) => set('maxAttempts', v)} min={1} error={shown.maxAttempts} hint="Empty means unlimited. Wrong-format answers never count." />}
              <Num label="Order" value={d.position} onChange={(v) => set('position', v)} min={-1000} hint="Lower numbers come first in the category." />
            </div>
          </Section>

          {challenge ? (
            <Extras challenge={challenge} tournament={tournament} onChanged={refresh} />
          ) : (
            <p className="rounded-card border border-dashed border-line p-4 text-sm text-muted">{tournament ? 'Files' : 'Hints, files'} {dfir ? '' : 'and a flag test '}appear here once the challenge is created.</p>
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
