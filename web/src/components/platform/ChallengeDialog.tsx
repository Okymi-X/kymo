import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Check, Download, Lightbulb, Lock } from 'lucide-react'
import { useState, type FormEvent } from 'react'
import { platform } from '../../api/platform'
import type { PlayerChallenge, SubmitResult } from '../../api/platformTypes'
import { flagProgress } from '../../lib/flags'
import { fileSize, flagHint, verdictText } from '../../lib/platform'
import { Modal, ModalBody, ModalHeader } from '../Modal'
import { Button, ErrorText, Input } from '../ui'
import { Pill } from './bits'
import { InstancePanel } from './InstancePanel'

interface Props {
  slug: string
  challenge: PlayerChallenge
  category: string
  /** Instances of this event are reached through the player's VPN. */
  vpn?: boolean
  labHost?: number | null
  onClose: () => void
}

/** One answer box for a DFIR question. */
function QuestionForm({ slug, challengeId, itemId, label, locked, refresh }: { slug: string; challengeId: number; itemId: number; label: string; locked: boolean; refresh: () => void }) {
  const [value, setValue] = useState('')
  const [result, setResult] = useState<SubmitResult | null>(null)
  const send = useMutation({
    mutationFn: () => platform.submitItem(slug, challengeId, itemId, value.trim()),
    onSuccess: (r) => {
      setResult(r)
      if (r.correct) setValue('')
      refresh()
    },
  })
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault()
        if (value.trim()) send.mutate()
      }}
      className="flex flex-col gap-1.5"
    >
      <div className="flex gap-2">
        <Input value={value} onChange={(e) => setValue(e.target.value)} aria-label={label} placeholder="Your answer" disabled={locked} autoComplete="off" spellCheck={false} className="min-w-0 flex-1 font-mono" />
        <Button type="submit" variant="primary" disabled={locked || !value.trim() || send.isPending}>
          Submit
        </Button>
      </div>
      <div aria-live="polite" className="min-h-5 text-sm">
        {result && (
          <span className={result.correct ? 'text-win' : 'text-danger'}>
            {result.correct ? `Correct, ${result.points} points.` : 'Not correct.'}
            {!result.correct && result.attempts_remaining !== null && ` ${result.attempts_remaining} ${result.attempts_remaining === 1 ? 'attempt' : 'attempts'} left.`}
          </span>
        )}
      </div>
      <ErrorText error={send.error} />
    </form>
  )
}

export function ChallengeDialog({ slug, challenge, category, vpn = false, labHost = null, onClose }: Props) {
  const client = useQueryClient()
  const refresh = () => {
    void client.invalidateQueries({ queryKey: ['challenges', slug] })
    void client.invalidateQueries({ queryKey: ['scoreboard', slug] })
    void client.invalidateQueries({ queryKey: ['progress', slug] })
  }
  const [flag, setFlag] = useState('')
  const [result, setResult] = useState<SubmitResult | null>(null)
  const submit = useMutation({
    mutationFn: () => platform.submit(slug, challenge.id, flag.trim()),
    onSuccess: (r) => {
      setResult(r)
      if (r.correct) setFlag('')
      refresh()
    },
  })
  const unlock = useMutation({ mutationFn: (hint: number) => platform.unlockHint(slug, challenge.id, hint), onSuccess: refresh })
  const [confirming, setConfirming] = useState<number | null>(null)
  const download = useMutation({ mutationFn: (a: { id: number; name: string }) => platform.download(slug, challenge.id, a.id, a.name) })

  const open = challenge.status === 'open'
  const onSubmit = (e: FormEvent) => {
    e.preventDefault()
    if (flag.trim()) submit.mutate()
  }
  const hintText = flagHint(challenge.flag_format.prefix, challenge.flag_format.suffix)
  // Several flags: how many are found, shown next to the one input every flag goes into.
  const found = result?.flags_found ?? challenge.flags_found ?? 0
  const total = result?.flags_total ?? challenge.flags_total ?? 1
  const progress = flagProgress(found, total)

  return (
    <Modal label={challenge.title} onClose={onClose} width="40rem">
      <ModalHeader
        title={challenge.title}
        subtitle={
          <>
            {category} · {challenge.solved_by} {challenge.solved_by === 1 ? 'solve' : 'solves'}
            {challenge.solved && ' · solved by you'}
            {!open && ' · closed'}
          </>
        }
        actions={<Pill tone="text-ink">{challenge.points} pts</Pill>}
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
            {download.isSuccess && download.variables && (
              <p role="status" className="text-sm font-medium text-win">
                Saved {download.variables.name}. Look in your browser's downloads.
              </p>
            )}
            <ErrorText error={download.error} />
          </div>
        )}

        {challenge.kind === 'dfir' && (
          <ol className="flex flex-col gap-3">
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
                    <QuestionForm slug={slug} challengeId={challenge.id} itemId={item.id} label={`Answer to question ${i + 1}`} locked={!open || item.attempts_remaining === 0} refresh={refresh} />
                    {item.attempts_remaining !== null && <p className="text-xs text-muted">{item.attempts_remaining} attempts left on this question.</p>}
                  </>
                )}
              </li>
            ))}
          </ol>
        )}

        {challenge.requires_instance && <InstancePanel slug={slug} challengeId={challenge.id} open={open && !challenge.solved} vpn={vpn} labHost={labHost} />}
        {challenge.requires_instance && challenge.solved && (
          <p className="text-sm text-muted">You solved this challenge, so a new instance cannot be started for it.</p>
        )}

        {challenge.hints.length > 0 && (
          <div className="flex flex-col gap-2">
            <h3 className="text-sm font-semibold">Hints</h3>
            {challenge.hints.map((h, i) =>
              h.unlocked ? (
                <p key={h.id} className="whitespace-pre-wrap break-words rounded-control border border-line bg-bg p-3 text-sm">
                  <Lightbulb size={14} aria-hidden="true" className="mr-1.5 inline text-accent" />
                  {h.content}
                </p>
              ) : confirming === h.id ? (
                <div key={h.id} className="flex flex-wrap items-center gap-2 rounded-control border border-line p-3 text-sm">
                  <span>
                    Unlock hint {i + 1} for <strong>{h.cost}</strong> {h.cost === 1 ? 'point' : 'points'}? The cost comes off this challenge when you solve it.
                  </span>
                  <span className="ml-auto flex gap-2">
                    <Button variant="ghost" onClick={() => setConfirming(null)}>
                      Cancel
                    </Button>
                    <Button variant="primary" disabled={unlock.isPending} onClick={() => unlock.mutate(h.id, { onSettled: () => setConfirming(null) })}>
                      Unlock
                    </Button>
                  </span>
                </div>
              ) : (
                <Button key={h.id} className="justify-start" disabled={!open || challenge.solved} onClick={() => setConfirming(h.id)}>
                  <Lock size={15} aria-hidden="true" /> Hint {i + 1}
                  <span className="ml-auto text-xs text-muted">costs {h.cost}</span>
                </Button>
              ),
            )}
            <ErrorText error={unlock.error} />
          </div>
        )}
      </ModalBody>

      {/* The flag form stays in view while the rest scrolls: it is what the player came here to do. */}
      {challenge.kind !== 'dfir' && (
      <form onSubmit={onSubmit} className="flex flex-col gap-2 border-t border-line bg-surface px-6 py-4">
        {challenge.solved ? (
          <p className="flex items-center gap-2 text-win" aria-live="polite">
            <Check size={18} aria-hidden="true" />
            {result?.correct ? `Correct, ${result.points} points.` : 'You solved this challenge.'}
          </p>
        ) : (
          <>
            <div className="flex items-center gap-2">
              <label htmlFor="flag" className="text-sm font-semibold">
                Flag
              </label>
              {progress && (
                <span aria-label={`${found} of ${total} flags found`} className={`rounded-full border border-line px-2 py-0.5 font-mono text-xs ${found === total ? 'text-win' : 'text-accent'}`}>
                  {progress}
                </span>
              )}
            </div>
            <div className="flex gap-2">
              <Input
                id="flag"
                value={flag}
                onChange={(e) => setFlag(e.target.value)}
                placeholder={hintText || 'Enter the flag'}
                disabled={!open}
                autoComplete="off"
                spellCheck={false}
                className="min-w-0 flex-1 font-mono"
              />
              <Button type="submit" variant="primary" disabled={!open || !flag.trim() || submit.isPending}>
                Submit
              </Button>
            </div>
            <div aria-live="polite" className="min-h-5 text-sm">
              {result?.correct && !challenge.solved && result.flags_total && result.flags_total > 1 && (
                <span className="text-win">
                  Flag {result.flags_found} of {result.flags_total} found, {result.points} points.
                </span>
              )}
              {result && !result.correct && (
                <span className="text-danger">
                  {verdictText[result.verdict]}.
                  {result.attempts_remaining !== null && ` ${result.attempts_remaining} ${result.attempts_remaining === 1 ? 'attempt' : 'attempts'} left.`}
                </span>
              )}
              {result?.sharing && (
                <p className="mt-1 text-danger">
                  {result.sharing.status === 'warning'
                    ? 'That flag belongs to another player. Sharing flags is against the rules; doing it again can get you banned.'
                    : result.sharing.status === 'banned'
                      ? 'That flag belongs to another player. Your account has been banned for sharing flags.'
                      : 'That flag belongs to another player. The organizers have been told.'}
                </p>
              )}
              {!result && challenge.max_attempts !== null && challenge.attempts_remaining !== null && (
                <span className="text-muted">{challenge.attempts_remaining} attempts left on this challenge.</span>
              )}
              {!open && <span className="text-muted">This challenge is closed.</span>}
            </div>
          </>
        )}
        <ErrorText error={submit.error} />
      </form>
      )}
    </Modal>
  )
}
