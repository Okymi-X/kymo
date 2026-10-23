import { useMemo } from 'react'
import type { Match, Slot, Tournament } from '../api/types'
import { groupName } from '../lib/labels'
import { computeGroupTables } from '../lib/stats'
import { ScheduleBoard } from './ScheduleBoard'
import { StandingsTable } from './StandingsTable'

interface Props {
  tournament: Tournament
  teamName: (slot: Slot) => string
  onSelect: (teamId: number) => void
  onEdit?: (match: Match) => void
}

/** Group stage: one table per group, with that group's fixtures folded underneath. */
export function GroupsView({ tournament, teamName, onSelect, onEdit }: Props) {
  const tables = useMemo(() => computeGroupTables(tournament), [tournament])
  const fixtures = (group: number) => (tournament.bracket?.matches ?? []).filter((m) => m.stage === 'league' && m.group === group)
  return (
    <div className="flex flex-col gap-10">
      {tables.map(({ group, rows }) => {
        const all = fixtures(group)
        const played = all.filter((m) => m.score !== null || m.forfeit).length
        return (
          <section key={group} aria-label={groupName(group)} className="flex flex-col gap-4">
            <StandingsTable tournament={tournament} rows={rows} onSelect={onSelect} league title={groupName(group)} advancing={tournament.advance} hideExport />
            <details className="group rounded-card border border-line bg-surface">
              <summary className="cursor-pointer select-none px-4 py-3 text-sm font-medium">
                Fixtures <span className="font-mono text-xs tabular-nums text-muted">{played}/{all.length} played</span>
              </summary>
              <div className="border-t border-line p-4">
                <ScheduleBoard tournament={tournament} teamName={teamName} onEdit={onEdit} group={group} />
              </div>
            </details>
          </section>
        )
      })}
    </div>
  )
}
