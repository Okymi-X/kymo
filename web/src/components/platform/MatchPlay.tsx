import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { Link } from 'react-router'
import { useWho } from '../../api/client'
import { platform } from '../../api/platform'
import type { Tournament } from '../../api/types'
import { MatchBoard } from '../../pages/MatchPage'
import { Button } from '../ui'

/** The Play tab: the signed-in player's current match, with its challenges, right on the tournament page. */
export function PlayTab({ tournament, teamName }: { tournament: Tournament; teamName: (id: number) => string }) {
  const who = useWho()
  const [picked, setPicked] = useState<number | null>(null)
  const live = tournament.status === 'live' && !tournament.paused
  const q = useQuery({ queryKey: ['my-matches', tournament.slug, who?.id], queryFn: () => platform.myMatches(tournament.slug), enabled: who !== null && live, refetchInterval: 15_000 })
  const note = (text: string) => <p className="rounded-card border border-dashed border-line p-8 text-center text-muted">{text}</p>
  if (who === null) {
    return (
      <div className="rounded-card border border-dashed border-line p-8 text-center">
        <p className="text-muted">Sign in with your player account to play your matches here.</p>
        <Link to="/signin" className="mt-2 inline-block font-medium text-accent hover:underline">
          Sign in
        </Link>
      </div>
    )
  }
  if (tournament.paused) return note('The tournament is paused. Your match continues when it resumes.')
  if (tournament.status === 'draft') return note('The tournament has not started yet.')
  if (tournament.status === 'done') return note('The tournament is over.')
  if (q.isPending) return <p className="text-muted">Loading your matches</p>
  const ids = q.data?.matches ?? []
  if (ids.length === 0) {
    return note('No match is waiting for you. A match appears here as soon as both teams are known. If you expected one, ask an organizer to check that your team is on the roster.')
  }
  const current = picked !== null && ids.includes(picked) ? picked : ids[0]
  const name = (slot: { kind: string; team?: number }) => (slot.kind === 'team' && slot.team !== undefined ? teamName(slot.team) : '')
  return (
    <div className="flex flex-col gap-6">
      {ids.length > 1 && (
        <div role="tablist" aria-label="Your matches" className="flex flex-wrap gap-2">
          {ids.map((id) => {
            const m = tournament.bracket?.matches[id]
            return (
              <Button key={id} role="tab" aria-selected={id === current} variant={id === current ? 'primary' : 'secondary'} onClick={() => setPicked(id)}>
                {m ? `${name(m.slots[0])} versus ${name(m.slots[1])}` : `Match ${id}`}
              </Button>
            )
          })}
        </div>
      )}
      <MatchBoard slug={tournament.slug} matchId={current} embedded />
      <p className="text-sm text-muted">
        <Link to={`/t/${tournament.slug}/match/${current}`} className="font-medium text-accent hover:underline">
          Open this match on its own page
        </Link>
      </p>
    </div>
  )
}
