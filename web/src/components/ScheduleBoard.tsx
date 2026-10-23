import { useMemo } from 'react'
import type { Match, Slot, Tournament } from '../api/types'
import { MatchCard } from './MatchCard'

interface Props {
  tournament: Tournament
  teamName: (slot: Slot) => string
  onEdit?: (match: Match) => void
}

/** Round robin fixtures grouped by round. Every team plays once per round or rests. */
export function ScheduleBoard({ tournament, teamName, onEdit }: Props) {
  const rounds = useMemo(() => {
    const grouped = new Map<number, Match[]>()
    for (const m of tournament.bracket?.matches ?? []) grouped.set(m.round, [...(grouped.get(m.round) ?? []), m])
    return [...grouped.entries()].sort((a, b) => a[0] - b[0])
  }, [tournament.bracket])
  return (
    <div className="flex flex-col gap-8">
      {rounds.map(([round, matches]) => {
        const played = matches.filter((m) => m.score !== null || m.forfeit).length
        return (
          <section key={round} aria-label={`Round ${round}`} className="flex flex-col gap-3">
            <h2 className="text-sm font-medium text-muted">
              Round {round} <span className="font-mono text-xs tabular-nums">{played}/{matches.length}</span>
            </h2>
            <ul className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
              {matches.map((m) => (
                <li key={m.id} className="min-w-0">
                  <MatchCard match={m} teamName={teamName} onEdit={onEdit} quiet />
                </li>
              ))}
            </ul>
          </section>
        )
      })}
    </div>
  )
}
