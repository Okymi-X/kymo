import { useMutation, useQuery } from '@tanstack/react-query'
import { Plus, Trash2 } from 'lucide-react'
import { useState } from 'react'
import { platform } from '../../api/platform'
import type { AdminChallenge, DfirItemBody, FlagSpec } from '../../api/platformTypes'
import { Button, ErrorText, Field, Input, Textarea } from '../../components/ui'

interface ItemDraft {
  question: string
  answer: string
  caseSensitive: boolean
  points: string
  attempts: string
}

const toFlag = (answer: string): FlagSpec => ({ kind: 'static', value: answer })

/** The questions of a DFIR challenge: each has its own answer, points and attempt limit. */
export function DfirItems({ challenge, onChanged }: { challenge: AdminChallenge; onChanged: () => void }) {
  const total = challenge.scoring.kind === 'static' ? challenge.scoring.points : 0
  const q = useQuery({ queryKey: ['dfir-items', challenge.id], queryFn: () => platform.dfirItems(challenge.id) })
  const [rows, setRows] = useState<ItemDraft[] | null>(null)
  const current: ItemDraft[] =
    rows ??
    (q.data ?? []).map((i) => ({
      question: i.question,
      answer: i.answer.kind === 'static' ? i.answer.value : i.answer.kind === 'regex' ? i.answer.pattern : i.answer.expected,
      caseSensitive: i.case_sensitive,
      points: String(i.points),
      attempts: i.max_attempts === null ? '' : String(i.max_attempts),
    }))
  const sum = current.reduce((n, r) => n + (Number(r.points) || 0), 0)
  const save = useMutation({
    mutationFn: () =>
      platform.saveDfirItems(
        challenge.id,
        current.map<DfirItemBody>((r) => ({
          question: r.question.trim(),
          answer: toFlag(r.answer),
          case_sensitive: r.caseSensitive,
          points: Math.trunc(Number(r.points)),
          max_attempts: r.attempts.trim() === '' ? null : Math.trunc(Number(r.attempts)),
        })),
      ),
    onSuccess: async () => {
      setRows(null)
      await q.refetch()
      onChanged()
    },
  })
  const edit = (index: number, patch: Partial<ItemDraft>) => setRows(current.map((r, i) => (i === index ? { ...r, ...patch } : r)))
  const ready = current.length > 0 && current.every((r) => r.question.trim() && r.answer.trim() && Number(r.points) >= 1) && sum === total
  return (
    <section className="flex flex-col gap-3 rounded-card border border-line p-4">
      <div>
        <h3 className="text-sm font-semibold">Questions</h3>
        <p className="mt-0.5 text-xs text-muted">
          Each question has its own answer and points. A team earns the points of every question it answers, so partial credit is automatic. The questions must add up to the challenge's {total} points.
        </p>
      </div>
      {current.map((r, i) => (
        <div key={i} className="flex flex-col gap-2 rounded-control border border-line p-3">
          <Field label={`Question ${i + 1}`}>
            <Textarea value={r.question} onChange={(e) => edit(i, { question: e.target.value })} rows={2} />
          </Field>
          <div className="grid gap-2 sm:grid-cols-[1fr_6rem_6rem_auto]">
            <Field label="Answer">
              <Input value={r.answer} onChange={(e) => edit(i, { answer: e.target.value })} className="font-mono" autoComplete="off" />
            </Field>
            <Field label="Points">
              <Input type="number" min={1} value={r.points} onChange={(e) => edit(i, { points: e.target.value })} className="font-mono" />
            </Field>
            <Field label="Attempts">
              <Input type="number" min={1} value={r.attempts} onChange={(e) => edit(i, { attempts: e.target.value })} placeholder="No limit" className="font-mono" />
            </Field>
            <Button variant="ghost" aria-label={`Remove question ${i + 1}`} onClick={() => setRows(current.filter((_, j) => j !== i))} className="self-end">
              <Trash2 size={15} aria-hidden="true" />
            </Button>
          </div>
          <label className="flex items-center gap-2 text-sm">
            <input type="checkbox" checked={r.caseSensitive} onChange={(e) => edit(i, { caseSensitive: e.target.checked })} className="size-4 accent-[var(--color-accent)]" />
            Capital letters matter
          </label>
        </div>
      ))}
      <div className="flex flex-wrap items-center gap-2">
        <Button onClick={() => setRows([...current, { question: '', answer: '', caseSensitive: false, points: '', attempts: '' }])} disabled={current.length >= 50}>
          <Plus size={15} aria-hidden="true" /> Add a question
        </Button>
        <Button variant="primary" onClick={() => save.mutate()} disabled={!ready || rows === null || save.isPending}>
          Save questions
        </Button>
        <span className={`text-sm ${sum === total ? 'text-muted' : 'text-danger'}`}>
          {sum} of {total} points
        </span>
      </div>
      <ErrorText error={save.error} />
    </section>
  )
}

