import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Check, Download, Lock } from 'lucide-react'
import { useMemo, useState, type FormEvent } from 'react'
import { useParams } from 'react-router'
import { platform } from '../api/platform'
import type { AnswerResult, DuelChallenge, MatchView } from '../api/platformTypes'
import { BackLink } from '../components/BackLink'
import { Modal, ModalBody, ModalHeader } from '../components/Modal'
import { Empty, Gate, Loading, Pill, useNow } from '../components/platform/bits'
import { Button, ErrorText, Input, PageTitle } from '../components/ui'
import { InstancePanel } from '../components/platform/InstancePanel'
import { fileSize, flagHint } from '../lib/platform'
import { usePageTitle } from '../lib/usePageTitle'

const STATUS: Record<MatchView['status'], string> = {
  waiting: 'The organizer has not assigned challenges to this match yet.',
  open: 'Both sides are answering. Anyone on your team can submit.',
  needs_decision: 'Everything is answered. An organizer will confirm the result.',
  final: 'This match is over.',
}

const useAnswer = (slug: string, matchId: number, challenge: number, itemId: number | undefined, onDone: () => void) =>
  useMutation({
    mutationFn: (value: string) => platform.answerMatch(slug, matchId, challenge, value.trim(), itemId),
    onSuccess: onDone,
  })

const clock = (seconds: number) => `${Math.floor(Math.max(0, seconds) / 60)}:${String(Math.max(0, seconds) % 60).padStart(2, '0')}`

/** The question or flag box, with the verdict under it. */
function AnswerBox({ slug, matchId, challenge, itemId, label, placeholder, locked }: { slug: string; matchId: number; challenge: number; itemId?: number; label: string; placeholder: string; locked: boolean }) {
  const client = useQueryClient()
  const [value, setValue] = useState('')
  const [result, setResult] = useState<AnswerResult | null>(null)
  const send = useAnswer(slug, matchId, challenge, itemId, () => {
    void client.invalidateQueries({ queryKey: ['play', slug, matchId] })
    void client.invalidateQueries({ queryKey: ['tournament', slug] })
  })
  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (!value.trim()) return
    send.mutate(value, {
      onSuccess: (r) => {
        setResult(r)
        if (r.correct) setValue('')
      },
    })
  }
  return (
    <form onSubmit={submit} className="flex flex-col gap-2">
      <div className="flex gap-2">
        <Input value={value} onChange={(e) => setValue(e.target.value)} aria-label={label} placeholder={placeholder} disabled={locked} autoComplete="off" spellCheck={false} className="min-w-0 flex-1 font-mono" />
        <Button type="submit" variant="primary" disabled={locked || !value.trim() || send.isPending}>
          Submit
        </Button>
      </div>
      <div aria-live="polite" className="min-h-5 text-sm">
        {result && (
          <span className={result.correct ? 'text-win' : 'text-danger'}>
            {result.correct ? `Correct, ${result.points} points.` : 'Not correct.'}
            {!result.correct && result.attempts_left !== null && ` ${result.attempts_left} ${result.attempts_left === 1 ? 'attempt' : 'attempts'} left.`}
          </span>
        )}
      </div>
      <ErrorText error={send.error} />
    </form>
  )
}

function ChallengeDialog({ slug, view, challenge, vpn, labHost, onClose }: { slug: string; view: MatchView; challenge: DuelChallenge; vpn: boolean; labHost: number | null; onClose: () => void }) {
  const download = useMutation({ mutationFn: (a: { id: number; name: string }) => platform.download(slug, challenge.id, a.id, a.name) })
  const open = view.status === 'open'
  const me = view.you ?? 0
  const placeholder = challenge.flag_format ? flagHint(challenge.flag_format.prefix, challenge.flag_format.suffix) : 'Enter the flag'
  return (
    <Modal label={challenge.title} onClose={onClose} width="40rem">
      <ModalHeader
        title={challenge.title}
        subtitle={`${challenge.category} · ${challenge.kind === 'dfir' ? 'Multi-question' : challenge.kind === 'instance' ? 'Instance' : 'Single flag'}${open ? '' : ' · closed'}`}
        actions={<Pill tone="text-ink">{challenge.earned[me]} / {challenge.points} pts</Pill>}
      />
      <ModalBody>
        {challenge.description && <p className="whitespace-pre-wrap break-words text-sm leading-relaxed">{challenge.description}</p>}
        {challenge.attachments.length > 0 && (
          <div className="flex flex-col gap-2">
            <h3 className="text-sm font-semibold">Files</h3>
            <ul className="flex flex-col gap-1.5">
              {challenge.attachments.map((a) => (
                <li key={a.id}>
                  <Button className="w-full justify-start" onClick={() => download.mutate({ id: a.id, name: a.name })}>
                    <Download size={15} aria-hidden="true" />
                    <span className="truncate">{a.name}</span>
                    <span className="ml-auto text-xs text-muted">{fileSize(a.size)}</span>
                  </Button>
                </li>
              ))}
            </ul>
            <ErrorText error={download.error} />
          </div>
        )}
        {challenge.kind === 'instance' && <InstancePanel slug={slug} challengeId={challenge.id} open={open && !challenge.solved} vpn={vpn} labHost={labHost} />}
        {challenge.kind === 'dfir' && (
          <ol className="flex flex-col gap-4">
            {challenge.items.map((item, i) => (
              <li key={item.id} className="flex flex-col gap-2 rounded-card border border-line p-3">
                <p className="flex items-start gap-2 text-sm">
                  <span className="min-w-0 flex-1 whitespace-pre-wrap break-words">
                    <strong>{i + 1}.</strong> {item.question}
                  </span>
                  <span className="font-mono text-xs text-muted">{item.points} pts</span>
                  {item.solved && <Check size={16} aria-label="Answered" className="shrink-0 text-win" />}
                </p>
                {!item.solved && (
                  <>
                    <AnswerBox slug={slug} matchId={view.match_id} challenge={challenge.id} itemId={item.id} label={`Answer to question ${i + 1}`} placeholder="Your answer" locked={!open || item.attempts_left === 0} />
                    {item.attempts_left !== null && <p className="text-xs text-muted">{item.attempts_left} attempts left on this question.</p>}
                  </>
                )}
              </li>
            ))}
          </ol>
        )}
      </ModalBody>
      {challenge.kind !== 'dfir' && (
        <div className="flex flex-col gap-2 border-t border-line bg-surface px-6 py-4">
          {challenge.solved ? (
            <p className="flex items-center gap-2 text-win" aria-live="polite">
              <Check size={18} aria-hidden="true" /> Solved for your team.
            </p>
          ) : (
            <>
              <span className="text-sm font-semibold">Flag</span>
              <AnswerBox slug={slug} matchId={view.match_id} challenge={challenge.id} label="Flag" placeholder={placeholder} locked={!open || challenge.attempts_left === 0} />
              {challenge.attempts_left !== null && <p className="text-xs text-muted">{challenge.attempts_left} attempts left on this challenge.</p>}
            </>
          )}
        </div>
      )}
    </Modal>
  )
}

/** The match board. On its own page it has a back link and a title; inside the Play tab it does not. */
export function MatchBoard({ slug, matchId: id, embedded = false }: { slug: string; matchId: number; embedded?: boolean }) {
  const q = useQuery({ queryKey: ['play', slug, id], queryFn: () => platform.playMatch(slug, id), refetchInterval: 4000 })
  const now = useNow()
  const event = useQuery({ queryKey: ['event-info', slug], queryFn: () => platform.event(slug) })
  const [openId, setOpenId] = useState<number | null>(null)
  const view = q.data
  usePageTitle(view ? `${view.sides[0].name} versus ${view.sides[1].name}` : 'Match')
  const groups = useMemo(() => {
    const map = new Map<string, DuelChallenge[]>()
    for (const c of view?.challenges ?? []) map.set(c.category, [...(map.get(c.category) ?? []), c])
    return [...map.entries()]
  }, [view])
  if (q.isPending) return <Loading label="Loading the match" />
  if (!view) return <ErrorText error={q.error} />
  const me = view.you ?? 0
  const left = view.closes_at ? view.closes_at - Math.floor(now / 1000) : null
  const selected = view.challenges.find((c) => c.id === openId)
  return (
    <div className="flex flex-col gap-6">
      {embedded ? (
        <h2 className="text-lg font-semibold tracking-tight">
          {view.sides[0].name} <span className="text-muted">versus</span> {view.sides[1].name}
        </h2>
      ) : (
        <div>
          <BackLink to={`/t/${slug}`}>Tournament</BackLink>
          <PageTitle>
            {view.sides[0].name} <span className="text-muted">versus</span> {view.sides[1].name}
          </PageTitle>
        </div>
      )}

      <div className="grid grid-cols-2 gap-3" aria-label="Score">
        {view.sides.map((side, i) => (
          <div key={side.id} className={`rounded-card border p-4 ${i === me ? 'border-accent/60 bg-accent/5' : 'border-line bg-surface'}`}>
            <p className="truncate text-sm text-muted">{i === me ? `${side.name} (your team)` : side.name}</p>
            <p className="font-mono text-3xl font-semibold tabular-nums">{side.score}</p>
          </div>
        ))}
      </div>
      <p className="text-sm text-muted">
        {STATUS[view.status]}
        {left !== null && view.status === 'open' && <span className="ml-2 font-mono tabular-nums text-ink">{clock(left)} left</span>}
      </p>

      {groups.length === 0 ? (
        <Empty>No challenges yet.</Empty>
      ) : (
        groups.map(([category, challenges]) => (
          <section key={category} aria-label={category} className="flex flex-col gap-3">
            <h2 className="text-sm font-semibold uppercase tracking-wide text-muted">{category}</h2>
            <ul className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
              {challenges.map((c) => {
                const done = c.kind === 'dfir' ? c.items.length > 0 && c.items.every((i) => i.solved) : c.solved
                const answered = c.items.filter((i) => i.solved).length
                return (
                  <li key={c.id}>
                    <button
                      type="button"
                      onClick={() => setOpenId(c.id)}
                      className={`flex h-full w-full flex-col gap-2 rounded-card border p-4 text-left transition-[transform,box-shadow] hover:shadow-[var(--shadow-lift)] active:scale-[0.99] ${done ? 'border-win/50 bg-win/5' : 'border-line bg-surface'} ${view.status === 'open' ? '' : 'opacity-70'}`}
                    >
                      <span className="flex items-start justify-between gap-2">
                        <span className="font-medium">{c.title}</span>
                        {done ? <Check size={18} aria-label="Solved" className="shrink-0 text-win" /> : view.status !== 'open' ? <Lock size={16} aria-label="Closed" className="shrink-0 text-muted" /> : null}
                      </span>
                      <span className="mt-auto flex items-center justify-between text-sm text-muted">
                        <span className="font-mono tabular-nums text-ink">
                          {c.earned[me]} / {c.points} pts
                        </span>
                        <span>{c.kind === 'dfir' ? `${answered} of ${c.items.length} answered` : done ? 'Solved' : 'Open'}</span>
                      </span>
                    </button>
                  </li>
                )
              })}
            </ul>
          </section>
        ))
      )}
      {selected && <ChallengeDialog slug={slug} view={view} challenge={selected} vpn={event.data?.network_mode === 'vpn'} labHost={event.data?.lab_host_id ?? null} onClose={() => setOpenId(null)} />}
    </div>
  )
}

export function MatchPage() {
  const { slug = '', matchId = '' } = useParams()
  return (
    <Gate need="player">
      <MatchBoard slug={slug} matchId={Number(matchId)} />
    </Gate>
  )
}
