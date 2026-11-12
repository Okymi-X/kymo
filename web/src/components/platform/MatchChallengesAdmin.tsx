import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { platform } from '../../api/platform'
import type { Match, Tournament } from '../../api/types'
import { Button, ErrorText } from '../ui'
import { Pill } from './bits'

const STATUS = {
  waiting: 'No challenges assigned yet',
  open: 'In play',
  needs_decision: 'Everything answered, waiting to be finalized',
  final: 'Decided',
} as const

/** The challenge side of a match, for organizers: what was assigned, what was earned, how to finish it. */
export function MatchChallengesAdmin({ tournament, match, names }: { tournament: Tournament; match: Match; names: [string, string] }) {
  const client = useQueryClient()
  const ready = match.slots.every((s) => s.kind === 'team')
  const played = match.score !== null || match.forfeit
  const eventId = tournament.event_id ?? 0
  const key = ['staff-match', tournament.id, match.id]
  const view = useQuery({ queryKey: key, queryFn: () => platform.staffMatch(tournament.id, match.id), enabled: ready, refetchInterval: 8000 })
  const pool = useQuery({ queryKey: ['admin-challenges', eventId], queryFn: () => platform.adminChallenges(eventId), enabled: ready && tournament.challenges.assignment === 'manual' })
  const [picked, setPicked] = useState<number[] | null>(null)
  const refresh = (t?: Tournament) => {
    if (t) client.setQueryData(['tournament', t.slug], t)
    void client.invalidateQueries({ queryKey: key })
    void client.invalidateQueries({ queryKey: ['tournament', tournament.slug] })
  }
  const assign = useMutation({
    mutationFn: (ids: number[]) => platform.assignMatch(tournament.id, match.id, ids),
    onSuccess: () => {
      setPicked(null)
      refresh()
    },
  })
  const finalize = useMutation({ mutationFn: (winner?: 'a' | 'b') => platform.finalizeMatch(tournament.id, match.id, winner), onSuccess: () => refresh() })
  const restart = useMutation({ mutationFn: () => platform.restartMatch(tournament.id, match.id), onSuccess: () => refresh() })

  if (tournament.challenges.assignment === 'none') return null
  if (!ready) return <p className="text-sm text-muted">Challenges are assigned once both teams are known.</p>
  const v = view.data
  const assignedIds = v?.challenges.map((c) => c.id) ?? []
  const selection = picked ?? assignedIds
  const manual = tournament.challenges.assignment === 'manual'
  const options = (pool.data ?? []).filter((c) => c.state === 'published' && !c.requires_instance)
  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <h3 className="text-sm font-semibold">Challenges</h3>
        {v && <Pill>{STATUS[v.status]}</Pill>}
        {v && (
          <span className="ml-auto font-mono text-sm tabular-nums">
            {v.sides[0].score} : {v.sides[1].score}
          </span>
        )}
      </div>
      {v?.challenges.map((c) => (
        <p key={c.id} className="flex items-center gap-2 text-sm">
          <span className="min-w-0 flex-1 truncate">{c.title}</span>
          <span className="font-mono text-xs text-muted">
            {c.earned[0]} / {c.earned[1]} of {c.points}
          </span>
        </p>
      ))}
      {manual && !played && v?.status !== 'final' && (
        <fieldset className="flex flex-col gap-1.5 rounded-control border border-line p-3">
          <legend className="px-1 text-xs text-muted">Pick the challenges for this match</legend>
          {options.length === 0 && <p className="text-sm text-muted">Publish some challenges first.</p>}
          {options.map((c) => (
            <label key={c.id} className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={selection.includes(c.id)}
                onChange={(e) => setPicked(e.target.checked ? [...selection, c.id] : selection.filter((id) => id !== c.id))}
                className="size-4 accent-[var(--color-accent)]"
              />
              {c.title} <span className="font-mono text-xs text-muted">{c.scoring.kind === 'static' ? c.scoring.points : ''} pts</span>
            </label>
          ))}
          <Button className="self-start" onClick={() => assign.mutate(selection)} disabled={assign.isPending || picked === null || selection.length === 0}>
            Save challenges
          </Button>
        </fieldset>
      )}
      {!played && v && v.status !== 'waiting' && v.status !== 'final' && (
        <div className="flex flex-wrap gap-2">
          <Button onClick={() => finalize.mutate(undefined)} disabled={finalize.isPending}>
            Finalize with the current score
          </Button>
          {v.status === 'needs_decision' && (
            <>
              <Button onClick={() => finalize.mutate('a')} disabled={finalize.isPending}>
                {names[0]} wins a tie
              </Button>
              <Button onClick={() => finalize.mutate('b')} disabled={finalize.isPending}>
                {names[1]} wins a tie
              </Button>
            </>
          )}
          <Button variant="ghost" onClick={() => restart.mutate()} disabled={restart.isPending}>
            Clear answers
          </Button>
        </div>
      )}
      {v?.answers && v.answers.length > 0 && <p className="text-xs text-muted">{v.answers.length} answers recorded. Every answer is kept in the audit trail with who sent it.</p>}
      <ErrorText error={view.error ?? assign.error ?? finalize.error ?? restart.error} />
    </div>
  )
}
