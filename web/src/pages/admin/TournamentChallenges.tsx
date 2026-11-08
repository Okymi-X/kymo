import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Plus, Trash2 } from 'lucide-react'
import { useState } from 'react'
import { platform } from '../../api/platform'
import type { AdminChallenge, Category, DfirItemBody, EventInfo, FlagSpec } from '../../api/platformTypes'
import { useTournament } from '../../api/hooks'
import type { ChallengeAssignment, ChallengeSettings } from '../../api/types'
import { ConfirmDialog } from '../../components/ConfirmDialog'
import { Modal, ModalBody, ModalCancel, ModalFooter, ModalHeader, modalForm } from '../../components/Modal'
import { Panel, Pill } from '../../components/platform/bits'
import { Button, ErrorText, Field, Input, Select, Textarea } from '../../components/ui'

const MODES: { value: ChallengeAssignment; label: string; help: string }[] = [
  { value: 'none', label: 'No challenges', help: 'Scores are typed in by an organizer or a referee, as in any tournament.' },
  { value: 'manual', label: 'Organizer picks', help: 'You choose the challenges of every match from the match dialog.' },
  { value: 'per_match', label: 'Random per match', help: 'Kymo draws different challenges for each match, avoiding ones a team already played.' },
  { value: 'per_round', label: 'Random per round', help: 'Kymo draws once per round; every match of that round gets the same challenges.' },
]

/** How this tournament decides its matches, and who plays for each team. */
export function TournamentChallengeSettings({ event }: { event: EventInfo }) {
  const client = useQueryClient()
  const q = useTournament(event.slug)
  const t = q.data
  const [draft, setDraft] = useState<(ChallengeSettings & { minutes: string }) | null>(null)
  const [filling, setFilling] = useState(false)
  const save = useMutation({
    mutationFn: (s: ChallengeSettings) => platform.saveChallengeSettings(t!.id, s),
    onSuccess: (next) => {
      client.setQueryData(['tournament', event.slug], next)
      setDraft(null)
    },
  })
  const fill = useMutation({
    mutationFn: () => platform.fillRoster(t!.id),
    onSuccess: (next) => {
      client.setQueryData(['tournament', event.slug], next)
      setFilling(false)
    },
  })
  if (!t) return null
  const s = draft ?? { ...t.challenges, minutes: t.challenges.time_limit ? String(Math.round(t.challenges.time_limit / 60)) : '' }
  const set = (patch: Partial<ChallengeSettings & { minutes: string }>) => setDraft({ ...s, ...patch })
  const mode = MODES.find((m) => m.value === s.assignment)
  const linked = t.teams.filter((team) => team.user_id || team.player_team_id).length
  const count = event.mode === 'solo' ? s.solo : s.team
  return (
    <Panel className="flex flex-col gap-4">
      <div>
        <h2 className="text-lg font-semibold tracking-tight">Challenges decide the matches</h2>
        <p className="mt-1 text-sm text-muted">
          Each side answers the same challenges and its points become the match score. The format then decides wins, draws and standings as usual. Author the challenges in the Challenges tab; they are worth fixed points.
        </p>
      </div>
      <div className="grid gap-4 sm:grid-cols-2">
        <Field label="How challenges reach matches" hint={mode?.help}>
          <Select value={s.assignment} onChange={(e) => set({ assignment: e.target.value as ChallengeAssignment })}>
            {MODES.map((m) => (
              <option key={m.value} value={m.value}>
                {m.label}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Time limit (minutes)" hint="Optional. A match closes this long after its first answer. Empty means no limit.">
          <Input type="number" min={1} value={s.minutes} onChange={(e) => set({ minutes: e.target.value })} className="font-mono" />
        </Field>
        <Field label="Challenges per match, solo" hint={event.mode === 'solo' ? 'Used by this event.' : 'Used if this event is solo.'}>
          <Input type="number" min={1} max={10} value={s.solo} onChange={(e) => set({ solo: Number(e.target.value) })} className="font-mono" />
        </Field>
        <Field label="Challenges per match, team" hint={event.mode === 'team' ? 'Used by this event.' : 'Used if this event is in teams.'}>
          <Input type="number" min={1} max={10} value={s.team} onChange={(e) => set({ team: Number(e.target.value) })} className="font-mono" />
        </Field>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Button
          variant="primary"
          disabled={draft === null || save.isPending}
          onClick={() =>
            save.mutate({ assignment: s.assignment, solo: s.solo, team: s.team, seed: s.seed, time_limit: s.minutes.trim() === '' ? null : Math.round(Number(s.minutes) * 60) })
          }
        >
          Save
        </Button>
        {draft && (
          <Button variant="ghost" onClick={() => setDraft(null)}>
            Discard
          </Button>
        )}
        <span className="text-sm text-muted">
          Each match carries {count} {count === 1 ? 'challenge' : 'challenges'}.
        </span>
      </div>
      <ErrorText error={save.error} />
      {s.assignment !== 'none' && (
        <div className="flex flex-col gap-2 border-t border-line pt-4">
          <h3 className="text-sm font-semibold">Who plays for each team</h3>
          <p className="text-sm text-muted">
            {event.mode === 'solo' ? 'Each team here is one player.' : 'Each team here is a registered team.'} {linked} of {t.teams.length} roster entries are linked to registered{' '}
            {event.mode === 'solo' ? 'players' : 'teams'}. Only linked entries can receive challenges.
          </p>
          {t.status === 'draft' ? (
            <Button className="self-start" onClick={() => setFilling(true)}>
              Fill the roster from registrations
            </Button>
          ) : (
            <p className="text-sm text-muted">The roster is fixed once the tournament has started.</p>
          )}
        </div>
      )}
      {filling && (
        <ConfirmDialog
          title="Replace the roster?"
          message="The roster becomes everyone registered for this event, in the order they registered. Teams you added by hand are removed."
          confirmLabel="Fill the roster"
          pending={fill.isPending}
          onConfirm={() => fill.mutate()}
          onClose={() => setFilling(false)}
        />
      )}
      <ErrorText error={fill.error} />
    </Panel>
  )
}

interface ItemDraft {
  question: string
  answer: string
  caseSensitive: boolean
  points: string
  attempts: string
}

const toFlag = (answer: string): FlagSpec => ({ kind: 'static', value: answer })

function DfirItems({ challenge, onChanged }: { challenge: AdminChallenge; onChanged: () => void }) {
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

/** Authoring for a tournament challenge: fixed points, a flag or DFIR questions. */
export function TournamentChallengeDialog({ eventId, categories, challenge, onClose }: { eventId: number; categories: Category[]; challenge: AdminChallenge | null; onClose: () => void }) {
  const client = useQueryClient()
  const [title, setTitle] = useState(challenge?.title ?? '')
  const [categoryId, setCategoryId] = useState(challenge?.category_id ?? categories[0]?.id ?? 0)
  const [kind, setKind] = useState<'flag' | 'dfir'>(challenge?.kind ?? 'flag')
  const [description, setDescription] = useState(challenge?.description ?? '')
  const [points, setPoints] = useState(challenge?.scoring.kind === 'static' ? String(challenge.scoring.points) : '100')
  const [flag, setFlag] = useState(challenge?.flag.kind === 'static' ? challenge.flag.value : '')
  const [attempts, setAttempts] = useState(challenge?.max_attempts ? String(challenge.max_attempts) : '')
  const refresh = () => void client.invalidateQueries({ queryKey: ['admin-challenges', eventId] })
  const body = () => ({
    category_id: categoryId,
    title: title.trim(),
    description,
    scoring: { kind: 'static' as const, points: Math.trunc(Number(points)) },
    ...(kind === 'flag' ? { flag: { kind: 'static' as const, value: flag } } : {}),
    kind,
    flag_prefix: '',
    flag_suffix: '',
    case_sensitive: true,
    max_attempts: attempts.trim() === '' ? null : Math.trunc(Number(attempts)),
    release_at: null,
    retire_at: null,
    position: 0,
    requires_instance: false,
    instance_config: null,
  })
  const save = useMutation({
    mutationFn: () => (challenge ? platform.updateChallenge(challenge.id, body()) : platform.createChallenge(eventId, body())),
    onSuccess: () => {
      refresh()
      onClose()
    },
  })
  const state = useMutation({
    mutationFn: (action: 'publish' | 'retire') => (action === 'publish' ? platform.publishChallenge(challenge!.id) : platform.retireChallenge(challenge!.id)),
    onSuccess: refresh,
  })
  const valid = title.trim() !== '' && categoryId !== 0 && Number(points) >= 0 && (kind === 'dfir' || flag.trim() !== '')
  return (
    <Modal label={challenge ? 'Edit challenge' : 'New challenge'} onClose={onClose} width="44rem" backdropCloses={false}>
      <form
        className={modalForm}
        onSubmit={(e) => {
          e.preventDefault()
          if (valid) save.mutate()
        }}
      >
        <ModalHeader
          title={challenge ? 'Edit challenge' : 'New challenge'}
          subtitle="Worth a fixed number of points in a match."
          actions={
            challenge && (
              <>
                <Pill tone={challenge.state === 'published' ? 'text-win' : 'text-muted'}>{challenge.state}</Pill>
                {challenge.state !== 'published' ? (
                  <Button onClick={() => state.mutate('publish')} disabled={state.isPending}>
                    Publish
                  </Button>
                ) : (
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
          <div className="grid gap-4 sm:grid-cols-2">
            <Field label="Title">
              <Input value={title} onChange={(e) => setTitle(e.target.value)} maxLength={120} />
            </Field>
            <Field label="Category">
              <Select value={categoryId} onChange={(e) => setCategoryId(Number(e.target.value))}>
                {categories.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label="Type" hint={challenge ? 'The type is fixed once created.' : 'A flag is one answer. DFIR has several questions with partial credit.'}>
              <Select value={kind} onChange={(e) => setKind(e.target.value as 'flag' | 'dfir')} disabled={challenge !== null}>
                <option value="flag">Flag</option>
                <option value="dfir">DFIR questions</option>
              </Select>
            </Field>
            <Field label="Points">
              <Input type="number" min={0} value={points} onChange={(e) => setPoints(e.target.value)} className="font-mono" />
            </Field>
            {kind === 'flag' && (
              <Field label="Flag">
                <Input value={flag} onChange={(e) => setFlag(e.target.value)} className="font-mono" autoComplete="off" />
              </Field>
            )}
            <Field label="Attempt limit" hint={kind === 'flag' ? 'Empty means unlimited.' : 'Questions have their own limits.'}>
              <Input type="number" min={1} value={attempts} onChange={(e) => setAttempts(e.target.value)} disabled={kind === 'dfir'} className="font-mono" />
            </Field>
          </div>
          <Field label="What the players see">
            <Textarea value={description} onChange={(e) => setDescription(e.target.value)} rows={5} />
          </Field>
          {kind === 'dfir' && challenge && <DfirItems challenge={challenge} onChanged={refresh} />}
          {kind === 'dfir' && !challenge && <p className="rounded-card border border-dashed border-line p-4 text-sm text-muted">Create the challenge, then add its questions.</p>}
          <ErrorText error={save.error} />
        </ModalBody>
        <ModalFooter>
          <ModalCancel />
          <Button type="submit" variant="primary" disabled={!valid || save.isPending}>
            {challenge ? 'Save changes' : 'Create challenge'}
          </Button>
        </ModalFooter>
      </form>
    </Modal>
  )
}
