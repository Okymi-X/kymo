import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Paperclip, Trash2 } from 'lucide-react'
import { useRef, useState } from 'react'
import { platform } from '../../api/platform'
import type { AdminChallenge, Category, ChallengeBody, FlagSpec, Scoring } from '../../api/platformTypes'
import { Modal } from '../../components/Modal'
import { Toggle } from '../../components/platform/bits'
import { Button, ErrorText, Field, Input, Select, Textarea } from '../../components/ui'
import { fileSize, fromLocalInput, toLocalInput, verdictText } from '../../lib/platform'

interface Draft {
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

const blank = (category: number): Draft => ({
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

function fromChallenge(c: AdminChallenge): Draft {
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

function Num({ label, value, onChange, min = 0 }: { label: string; value: string; onChange: (v: string) => void; min?: number }) {
  return (
    <Field label={label}>
      <Input type="number" min={min} value={value} onChange={(e) => onChange(e.target.value)} className="font-mono" />
    </Field>
  )
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
  return (
    <div className="flex flex-col gap-5 border-t border-line pt-5">
      <div className="flex flex-col gap-2">
        <h3 className="text-sm font-semibold">Hints</h3>
        {challenge.hints.map((h) => (
          <div key={h.id} className="flex items-start gap-2 rounded-control border border-line p-2.5 text-sm">
            <span className="min-w-0 flex-1 whitespace-pre-wrap break-words">{h.content}</span>
            <span className="font-mono text-xs text-muted">{h.cost} pts</span>
            <button type="button" aria-label="Delete hint" onClick={() => delHint.mutate(h.id)} className="text-muted hover:text-danger">
              <Trash2 size={15} aria-hidden="true" />
            </button>
          </div>
        ))}
        <div className="flex gap-2">
          <Input value={hint} onChange={(e) => setHint(e.target.value)} placeholder="New hint" aria-label="New hint" className="min-w-0 flex-1" />
          <Input type="number" min={0} value={cost} onChange={(e) => setCost(e.target.value)} aria-label="Hint cost" className="w-20 font-mono" />
          <Button onClick={() => addHint.mutate()} disabled={!hint.trim() || addHint.isPending}>
            Add
          </Button>
        </div>
        <ErrorText error={addHint.error ?? delHint.error} />
      </div>

      <div className="flex flex-col gap-2">
        <h3 className="text-sm font-semibold">Attachments</h3>
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
          Upload a file (up to 10 MB)
        </Button>
        <ErrorText error={upload.error ?? delFile.error} />
      </div>

      <div className="flex flex-col gap-2">
        <h3 className="text-sm font-semibold">Test the flag</h3>
        <p className="text-xs text-muted">Checks a flag without recording a submission or a solve.</p>
        <div className="flex gap-2">
          <Input value={probe} onChange={(e) => setProbe(e.target.value)} placeholder="Try a flag" aria-label="Flag to test" className="min-w-0 flex-1 font-mono" autoComplete="off" />
          <Button onClick={() => test.mutate()} disabled={!probe || test.isPending}>
            Test
          </Button>
        </div>
        {test.data && <p className={test.data.verdict === 'correct' ? 'text-sm text-win' : 'text-sm text-danger'}>{verdictText[test.data.verdict]}</p>}
        <ErrorText error={test.error} />
      </div>
    </div>
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
  const [d, setD] = useState<Draft>(() => (challenge ? fromChallenge(challenge) : blank(categories[0]?.id ?? 0)))
  const set = <K extends keyof Draft>(key: K, value: Draft[K]) => setD((prev) => ({ ...prev, [key]: value }))
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
  const valid = d.title.trim() !== '' && d.flagValue !== '' && d.category_id !== 0 && (!d.requiresInstance || d.image.trim() !== '')
  // The parent resolves the challenge from the live list, so hint and file changes show up here.
  const live = challenge

  return (
    <Modal label={challenge ? 'Edit challenge' : 'New challenge'} onClose={onClose} width="44rem">
      <form
        className="flex max-h-[88vh] flex-col gap-5 overflow-y-auto p-6"
        onSubmit={(e) => {
          e.preventDefault()
          if (valid) save.mutate()
        }}
      >
        <div className="flex items-center justify-between gap-3">
          <h2 className="text-xl font-semibold tracking-tight">{challenge ? 'Edit challenge' : 'New challenge'}</h2>
          {live && (
            <div className="flex items-center gap-2">
              <span className="rounded-full border border-line px-2.5 py-0.5 text-xs font-medium capitalize">{live.state}</span>
              {live.state !== 'published' && (
                <Button onClick={() => state.mutate('publish')} disabled={state.isPending}>
                  Publish
                </Button>
              )}
              {live.state === 'published' && (
                <Button variant="danger" onClick={() => state.mutate('retire')} disabled={state.isPending}>
                  Retire
                </Button>
              )}
            </div>
          )}
        </div>
        <ErrorText error={state.error} />

        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="Title">
            <Input value={d.title} onChange={(e) => set('title', e.target.value)} maxLength={100} autoFocus />
          </Field>
          <Field label="Category">
            <Select value={d.category_id} onChange={(e) => set('category_id', int(e.target.value))}>
              {categories.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </Select>
          </Field>
        </div>
        <Field label="Description (shown as plain text)">
          <Textarea value={d.description} onChange={(e) => set('description', e.target.value)} />
        </Field>

        <fieldset className="flex flex-col gap-3 rounded-card border border-line p-4">
          <legend className="px-1 text-sm font-semibold">Points</legend>
          <Select value={d.scoring} onChange={(e) => set('scoring', e.target.value as Draft['scoring'])} aria-label="Scoring type" className="self-start">
            <option value="static">Fixed points</option>
            <option value="dynamic">Dynamic (drops as more teams solve)</option>
          </Select>
          {d.scoring === 'static' ? (
            <Num label="Points" value={d.points} onChange={(v) => set('points', v)} />
          ) : (
            <div className="grid gap-3 sm:grid-cols-4">
              <Num label="Start at" value={d.initial} onChange={(v) => set('initial', v)} />
              <Num label="Floor" value={d.minimum} onChange={(v) => set('minimum', v)} />
              <Num label="Decay" value={d.decay} onChange={(v) => set('decay', v)} />
              <Field label="Curve">
                <Select value={d.curve} onChange={(e) => set('curve', e.target.value as Draft['curve'])}>
                  <option value="parabolic">Parabolic</option>
                  <option value="linear">Linear</option>
                  <option value="logarithmic">Logarithmic</option>
                </Select>
              </Field>
            </div>
          )}
        </fieldset>

        <fieldset className="flex flex-col gap-3 rounded-card border border-line p-4">
          <legend className="px-1 text-sm font-semibold">Flag</legend>
          <div className="grid gap-3 sm:grid-cols-[10rem_1fr]">
            <Field label="Kind">
              <Select value={d.flagKind} onChange={(e) => set('flagKind', e.target.value as FlagSpec['kind'])}>
                <option value="static">Exact text</option>
                <option value="regex">Pattern (regex)</option>
                <option value="per_team">Per team</option>
              </Select>
            </Field>
            <Field label={d.flagKind === 'regex' ? 'Pattern (must match the whole flag)' : 'Flag'}>
              <Input value={d.flagValue} onChange={(e) => set('flagValue', e.target.value)} className="font-mono" autoComplete="off" spellCheck={false} />
            </Field>
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <Field label="Accepted prefix">
              <Input value={d.prefix} onChange={(e) => set('prefix', e.target.value)} placeholder="KYMO{" className="font-mono" />
            </Field>
            <Field label="Accepted suffix">
              <Input value={d.suffix} onChange={(e) => set('suffix', e.target.value)} placeholder="}" className="font-mono" />
            </Field>
          </div>
          <Toggle label="Case sensitive" checked={d.caseSensitive} onChange={(v) => set('caseSensitive', v)} />
        </fieldset>

        <div className="grid gap-4 sm:grid-cols-4">
          <Num label="Attempt limit (empty: none)" value={d.maxAttempts} onChange={(v) => set('maxAttempts', v)} min={1} />
          <Num label="Order" value={d.position} onChange={(v) => set('position', v)} min={-1000} />
          <Field label="Release at">
            <Input type="datetime-local" value={d.release} onChange={(e) => set('release', e.target.value)} />
          </Field>
          <Field label="Retire at">
            <Input type="datetime-local" value={d.retire} onChange={(e) => set('retire', e.target.value)} />
          </Field>
        </div>

        <fieldset className="flex flex-col gap-3 rounded-card border border-line p-4">
          <legend className="px-1 text-sm font-semibold">Instance</legend>
          <Toggle label="Each player or team starts its own container" checked={d.requiresInstance} onChange={(v) => set('requiresInstance', v)} />
          {d.requiresInstance && (
            <>
              <Field label="Container image">
                <Input value={d.image} onChange={(e) => set('image', e.target.value)} placeholder="registry.example.org/ctf/web-1:1.0" className="font-mono" />
              </Field>
              <div className="grid gap-3 sm:grid-cols-5">
                <Num label="Port" value={d.port} onChange={(v) => set('port', v)} min={1} />
                <Num label="CPU (millicores)" value={d.cpu} onChange={(v) => set('cpu', v)} min={1} />
                <Num label="Memory (MB)" value={d.memory} onChange={(v) => set('memory', v)} min={1} />
                <Num label="Processes" value={d.pids} onChange={(v) => set('pids', v)} min={1} />
                <Num label="Lifetime (s)" value={d.ttl} onChange={(v) => set('ttl', v)} min={1} />
              </div>
            </>
          )}
        </fieldset>

        <ErrorText error={save.error} />
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" variant="primary" disabled={!valid || save.isPending}>
            {challenge ? 'Save changes' : 'Create challenge'}
          </Button>
        </div>
        {live && <Extras challenge={live} onChanged={refresh} />}
      </form>
    </Modal>
  )
}
