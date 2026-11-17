import { useMutation } from '@tanstack/react-query'
import { Plus, Trash2 } from 'lucide-react'
import { useState } from 'react'
import { platform } from '../../../api/platform'
import type { AdminChallenge, CodeCase, LabCatalog } from '../../../api/platformTypes'
import { CodeResult } from '../../../components/platform/CodeResult'
import { Toggle } from '../../../components/platform/bits'
import { Button, ErrorText, Field, Select } from '../../../components/ui'
import { uniqueIds } from '../../../lib/code'
import type { Draft, Problems } from './draft'
import { Num, Section } from './parts'

interface Props {
  d: Draft
  set: <K extends keyof Draft>(key: K, value: Draft[K]) => void
  /** What the event's lab host runs; null while it is read or when the event has no lab host. */
  catalog: LabCatalog | null
  hasLab: boolean
  shown: Problems
  challenge: AdminChallenge | null
}

const area = 'min-h-16 w-full resize-y rounded-control border border-field-line bg-field p-2 font-mono text-xs outline-none focus:border-accent'

/** Languages, tests and limits of a coding challenge, and a place to try a reference solution. */
export function CodeSection({ d, set, catalog, hasLab, shown, challenge }: Props) {
  const c = d.code
  const patch = (next: Partial<Draft['code']>) => set('code', { ...c, ...next })
  const offered = catalog?.ok ? (catalog.artifacts ?? []).filter((a) => a.judge) : []
  const chosen = new Set(c.languages.map((l) => l.artifact))
  const toggle = (artifact: string, label: string) => {
    if (chosen.has(artifact)) {
      patch({ languages: c.languages.filter((l) => l.artifact !== artifact) })
      return
    }
    const next = [...c.languages, { id: '', label, artifact }]
    const ids = uniqueIds(next.map((l) => l.label))
    patch({ languages: next.map((l, i) => ({ ...l, id: ids[i] ?? l.id })) })
  }
  const editCase = (i: number, change: Partial<CodeCase>) => patch({ cases: c.cases.map((x, j) => (j === i ? { ...x, ...change } : x)) })
  const stale = c.languages.filter((l) => !offered.some((a) => a.artifact === l.artifact))

  return (
    <Section title="Coding" help="Players write a program that reads standard input and prints standard output. It runs on the event's lab host, once per test.">
      <Field label="Languages" error={shown.code && c.languages.length === 0 ? shown.code : undefined}>
        <div className="flex flex-col gap-2">
          {!hasLab && <p className="text-sm text-danger">The event has no lab host. Choose one in its Infrastructure tab: code only runs there.</p>}
          {hasLab && catalog && !catalog.ok && <p className="text-sm text-danger">Could not read the lab host: {catalog.error ?? 'no answer'}.</p>}
          {hasLab && catalog?.ok && offered.length === 0 && <p className="text-sm text-muted">This lab host has no language. Add a manifest with a <code className="font-mono">judge</code> block.</p>}
          <div className="flex flex-wrap gap-2">
            {offered.map((a) => (
              <label key={a.artifact} className={`flex cursor-pointer items-center gap-2 rounded-control border px-3 py-1.5 text-sm ${chosen.has(a.artifact) ? 'border-accent bg-accent/5' : 'border-line'}`}>
                <input type="checkbox" checked={chosen.has(a.artifact)} onChange={() => toggle(a.artifact, a.judge_label ?? a.artifact)} />
                {a.judge_label ?? a.artifact}
              </label>
            ))}
            {stale.map((l) => (
              <span key={l.artifact} className="flex items-center gap-2 rounded-control border border-danger/50 px-3 py-1.5 text-sm" title="The lab host does not list this language any more">
                {l.label} <span className="text-xs text-danger">missing</span>
                <button type="button" aria-label={`Remove ${l.label}`} onClick={() => patch({ languages: c.languages.filter((x) => x.artifact !== l.artifact) })} className="text-muted hover:text-danger">
                  <Trash2 size={13} aria-hidden="true" />
                </button>
              </span>
            ))}
          </div>
        </div>
      </Field>

      <div className="flex flex-col gap-3">
        <div className="flex items-baseline justify-between">
          <h4 className="text-sm font-medium">Tests ({c.cases.length})</h4>
          <span className="text-xs text-muted">Shown tests are examples; the others stay hidden.</span>
        </div>
        {c.cases.map((x, i) => (
          <div key={i} className="flex flex-col gap-2 rounded-control border border-line p-3" role="group" aria-label={`Test ${i + 1}`}>
            <div className="grid gap-2 sm:grid-cols-2">
              <Field label="Input">
                <textarea value={x.input} onChange={(e) => editCase(i, { input: e.target.value })} rows={2} spellCheck={false} className={area} />
              </Field>
              <Field label="Expected output">
                <textarea value={x.expected} onChange={(e) => editCase(i, { expected: e.target.value })} rows={2} spellCheck={false} className={area} />
              </Field>
            </div>
            <div className="flex items-center justify-between">
              <Toggle label="Show to players" checked={x.sample} onChange={(v) => editCase(i, { sample: v })} />
              {c.cases.length > 1 && (
                <button type="button" aria-label={`Remove test ${i + 1}`} onClick={() => patch({ cases: c.cases.filter((_, j) => j !== i) })} className="text-muted hover:text-danger">
                  <Trash2 size={15} aria-hidden="true" />
                </button>
              )}
            </div>
          </div>
        ))}
        <Button className="self-start" onClick={() => patch({ cases: [...c.cases, { input: '', expected: '', sample: false }] })} disabled={c.cases.length >= 50}>
          <Plus size={15} aria-hidden="true" /> Add a test
        </Button>
        {shown.code && c.languages.length > 0 && <p className="text-sm text-danger">{shown.code}</p>}
      </div>

      <div className="grid grid-cols-2 gap-4 sm:grid-cols-3">
        <Num label="Time per test" value={c.time} onChange={(v) => patch({ time: v })} min={100} hint="Milliseconds." />
        <Num label="Memory" value={c.memory} onChange={(v) => patch({ memory: v })} min={16} hint="MB." />
        <Field label="Output match">
          <Select value={c.compare} onChange={(e) => patch({ compare: e.target.value as Draft['code']['compare'] })}>
            <option value="lines">By line, ignore trailing spaces</option>
            <option value="tokens">By word, ignore all spacing</option>
            <option value="exact">Exactly</option>
          </Select>
        </Field>
      </div>

      {challenge?.code ? <Tester challenge={challenge} /> : <p className="text-xs text-muted">Save the challenge to try a reference solution against the tests.</p>}
    </Section>
  )
}

/** Runs a reference solution on the lab host against the saved tests, showing every test. Nothing is recorded. */
function Tester({ challenge }: { challenge: AdminChallenge }) {
  const languages = challenge.code?.languages ?? []
  const [language, setLanguage] = useState(languages[0]?.id ?? '')
  const [source, setSource] = useState('')
  const run = useMutation({ mutationFn: () => platform.testCode(challenge.id, language, source) })
  return (
    <details className="rounded-control border border-line p-3 text-sm">
      <summary className="cursor-pointer font-medium">Try a reference solution</summary>
      <div className="mt-3 flex flex-col gap-3">
        <Select value={language} onChange={(e) => setLanguage(e.target.value)} aria-label="Language of the solution" className="w-48">
          {languages.map((l) => (
            <option key={l.id} value={l.id}>
              {l.label}
            </option>
          ))}
        </Select>
        <textarea value={source} onChange={(e) => setSource(e.target.value)} rows={8} spellCheck={false} aria-label="Reference solution" className={`${area} text-sm`} placeholder="Runs against the saved tests, hidden ones included. Nothing is recorded." />
        <Button className="self-start" onClick={() => run.mutate()} disabled={!source.trim() || run.isPending}>
          {run.isPending ? 'Running...' : 'Run it'}
        </Button>
        <ErrorText error={run.error} />
        {run.data && <CodeResult result={run.data} />}
      </div>
    </details>
  )
}
