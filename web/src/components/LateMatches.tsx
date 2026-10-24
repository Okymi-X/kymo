import { useEffect, useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from '../api/client'
import type { Match, Side, Slot, Tournament } from '../api/types'
import { LATE_AFTER_MINUTES, lateMinutes } from '../lib/schedule'
import { Avatar } from './Avatar'
import { Button, ErrorText } from './ui'

interface Props {
  tournament: Tournament
  teamName: (slot: Slot) => string
  label: (m: Match) => string
}

/** Planned matches that should have started: the organizer can award the match to the team that showed up. */
export function LateMatches({ tournament, teamName, label }: Props) {
  const client = useQueryClient()
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 30_000)
    return () => window.clearInterval(timer)
  }, [])
  const forfeit = useMutation({
    mutationFn: ({ match, winner }: { match: number; winner: Side }) => api.setForfeit(tournament.id, match, winner),
    onSuccess: (t) => client.setQueryData(['tournament', t.slug], t),
  })
  const late = (tournament.bracket?.matches ?? [])
    .map((m) => ({ m, minutes: lateMinutes(m, now) }))
    .filter((x): x is { m: Match; minutes: number } => x.minutes !== null)
    .sort((a, b) => b.minutes - a.minutes)
  if (late.length === 0) return null
  return (
    <section aria-label="Late matches" className="flex flex-col gap-3 rounded-card border border-danger/40 bg-danger/5 p-4">
      <div>
        <h2 className="text-sm font-semibold">Late matches {late.length}</h2>
        <p className="text-xs text-muted">Started more than {LATE_AFTER_MINUTES} minutes ago and not played. If a team did not show up, award the match to the other side.</p>
      </div>
      <ul className="flex flex-col gap-3">
        {late.map(({ m, minutes }) => (
          <li key={m.id} className="flex flex-wrap items-center justify-between gap-3">
            <div className="min-w-0">
              <p className="truncate text-sm font-medium">
                {teamName(m.slots[0])} versus {teamName(m.slots[1])}
              </p>
              <p className="text-xs text-danger">
                {label(m)}, {minutes} {minutes === 1 ? 'minute' : 'minutes'} late{m.venue ? `, ${m.venue}` : ''}
              </p>
            </div>
            <div className="flex flex-wrap gap-2">
              {([0, 1] as const).map((side) => (
                <Button key={side} onClick={() => forfeit.mutate({ match: m.id, winner: side === 0 ? 'A' : 'B' })} disabled={forfeit.isPending}>
                  <Avatar name={teamName(m.slots[side])} size={18} />
                  <span className="max-w-[10rem] truncate">{teamName(m.slots[side])} advances</span>
                </Button>
              ))}
            </div>
          </li>
        ))}
      </ul>
      <ErrorText error={forfeit.error} />
    </section>
  )
}
