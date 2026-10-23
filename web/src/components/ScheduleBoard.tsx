import { useMemo, useState } from 'react'
import type { Match, Slot, Tournament } from '../api/types'
import { MatchCard } from './MatchCard'

interface Props {
  tournament: Tournament
  teamName: (slot: Slot) => string
  onEdit?: (match: Match) => void
  /** Only this group's fixtures. */
  group?: number
}

/** Round robin fixtures grouped by round. Every team plays once per round or rests. */
export function ScheduleBoard({ tournament, teamName, onEdit, group }: Props) {
  const rounds = useMemo(() => {
    const grouped = new Map<number, Match[]>()
    for (const m of tournament.bracket?.matches ?? []) {
      if (m.stage !== 'league' || (group !== undefined && m.group !== group)) continue
      grouped.set(m.round, [...(grouped.get(m.round) ?? []), m])
    }
    return [...grouped.entries()].sort((a, b) => a[0] - b[0])
  }, [tournament.bracket, group])
  const open = (r: [number, Match[]]) => r[1].some((m) => m.score === null && !m.forfeit)
  const current = (rounds.find(open) ?? rounds[rounds.length - 1])?.[0] ?? 1
  const [picked, setPicked] = useState<number | 'all' | null>(null)
  // A group's fixtures are few, so only the full schedule is split by round.
  const chosen = group !== undefined || rounds.length < 2 ? 'all' : (picked ?? current)
  const shown = chosen === 'all' ? rounds : rounds.filter(([round]) => round === chosen)
  return (
    <div className="flex flex-col gap-6">
      {group === undefined && rounds.length > 1 && (
        <nav aria-label="Rounds" className="flex gap-1.5 overflow-x-auto pb-1">
          {([...rounds.map(([round]) => round), 'all'] as const).map((value) => (
            <button
              key={value}
              type="button"
              aria-pressed={chosen === value}
              onClick={() => setPicked(value)}
              className={`h-9 shrink-0 rounded-full border px-3.5 text-sm font-medium transition-colors ${chosen === value ? 'border-ink bg-ink text-bg' : 'border-line text-muted hover:text-ink'}`}
            >
              {value === 'all' ? 'All rounds' : `Round ${value}`}
            </button>
          ))}
        </nav>
      )}
      {shown.map(([round, matches]) => {
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
