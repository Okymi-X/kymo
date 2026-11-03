import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Check, Download, Lightbulb, Lock } from 'lucide-react'
import { useState, type FormEvent } from 'react'
import { platform } from '../../api/platform'
import type { PlayerChallenge, SubmitResult } from '../../api/platformTypes'
import { fileSize, flagHint, verdictText } from '../../lib/platform'
import { Modal } from '../Modal'
import { Button, ErrorText, Input } from '../ui'
import { InstancePanel } from './InstancePanel'

interface Props {
  slug: string
  challenge: PlayerChallenge
  category: string
  onClose: () => void
}

export function ChallengeDialog({ slug, challenge, category, onClose }: Props) {
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

  return (
    <Modal label={challenge.title} onClose={onClose} width="40rem">
      <div className="flex max-h-[85vh] flex-col gap-5 overflow-y-auto p-6">
        <div>
          <p className="text-xs font-medium text-muted">{category}</p>
          <div className="mt-1 flex items-start justify-between gap-4">
            <h2 className="text-2xl font-semibold tracking-tight">{challenge.title}</h2>
            <span className="shrink-0 rounded-full border border-line px-3 py-1 font-mono text-sm tabular-nums">{challenge.points} pts</span>
          </div>
          <p className="mt-1 text-xs text-muted">
            {challenge.solved_by} {challenge.solved_by === 1 ? 'solve' : 'solves'}
            {challenge.solved && ' · solved by you'}
            {!open && ' · closed'}
          </p>
        </div>

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
                    <Button
                      variant="primary"
                      disabled={unlock.isPending}
                      onClick={() => unlock.mutate(h.id, { onSettled: () => setConfirming(null) })}
                    >
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

        {challenge.requires_instance && <InstancePanel slug={slug} challengeId={challenge.id} open={open && !challenge.solved} />}

        <form onSubmit={onSubmit} className="flex flex-col gap-2">
          <label htmlFor="flag" className="text-sm font-semibold">
            Flag
          </label>
          {challenge.solved ? (
            <p className="flex items-center gap-2 text-win" aria-live="polite">
              <Check size={18} aria-hidden="true" />
              {result?.correct ? `Correct, ${result.points} points.` : 'You solved this challenge.'}
            </p>
          ) : (
            <>
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
                {result && !result.correct && (
                  <span className="text-danger">
                    {verdictText[result.verdict]}.
                    {result.attempts_remaining !== null && ` ${result.attempts_remaining} ${result.attempts_remaining === 1 ? 'attempt' : 'attempts'} left.`}
                  </span>
                )}
                {result?.correct && <span className="text-win">Correct, {result.points} points.</span>}
              </div>
            </>
          )}
          <ErrorText error={submit.error} />
          {!challenge.solved && challenge.max_attempts !== null && challenge.attempts_remaining !== null && !result && (
            <p className="text-xs text-muted">{challenge.attempts_remaining} attempts left on this challenge.</p>
          )}
        </form>

        <div className="flex justify-end">
          <Button variant="ghost" onClick={onClose}>
            Close
          </Button>
        </div>
      </div>
    </Modal>
  )
}
