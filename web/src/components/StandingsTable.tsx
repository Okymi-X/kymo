import type { Tournament } from '../api/types'
import { standingsCsv, type TeamRow } from '../lib/stats'
import { Avatar } from './Avatar'
import { ExportButton } from './ExportButton'
import { StatusBadge } from './StatusBadge'

interface Props {
  tournament: Tournament
  rows: TeamRow[]
  onSelect: (teamId: number) => void
}

const num = 'text-right font-mono tabular-nums'

export function StandingsTable({ tournament, rows, onSelect }: Props) {
  const league = tournament.format === 'round_robin'
  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center justify-between gap-3">
        <p className="text-sm text-muted">{league ? 'Ranked by points (3 for a win, 1 for a draw), then goal difference, then goals scored.' : 'Active teams first, then by how far they went, wins and point difference.'}</p>
        <ExportButton filename={`${tournament.slug}-standings.csv`} build={() => standingsCsv(rows)} />
      </div>
      <div className="overflow-x-auto rounded-card border border-line bg-surface">
        <table className="w-full min-w-[40rem] text-sm">
          <caption className="sr-only">Standings for {tournament.name}</caption>
          <thead>
            <tr className="border-b border-line text-left text-xs text-muted">
              <th scope="col" className="w-10 px-4 py-3 font-medium">#</th>
              <th scope="col" className="px-2 py-3 font-medium">Team</th>
              <th scope="col" className={`px-3 py-3 font-medium ${num}`}>Seed</th>
              <th scope="col" className={`px-3 py-3 font-medium ${num}`}>Qualifier</th>
              <th scope="col" className={`px-3 py-3 font-medium ${num}`}>P</th>
              <th scope="col" className={`px-3 py-3 font-medium ${num}`}>W</th>
              {league && <th scope="col" className={`px-3 py-3 font-medium ${num}`}>D</th>}
              <th scope="col" className={`px-3 py-3 font-medium ${num}`}>L</th>
              <th scope="col" className={`px-3 py-3 font-medium ${num}`}>+/-</th>
              {league && <th scope="col" className={`px-3 py-3 font-medium ${num}`}>Pts</th>}
              <th scope="col" className="px-4 py-3 font-medium">Status</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r, i) => {
              const diff = r.pointsFor - r.pointsAgainst
              return (
                <tr key={r.team.id} className="border-b border-line last:border-0 hover:bg-line/30">
                  <td className="px-4 py-2.5 font-mono tabular-nums text-muted">{i + 1}</td>
                  <td className="px-2 py-1.5">
                    <button type="button" onClick={() => onSelect(r.team.id)} className="flex items-center gap-2.5 rounded-control px-1.5 py-1 text-left font-medium hover:text-accent">
                      <Avatar name={r.team.name} size={24} />
                      <span className="max-w-[16rem] truncate">{r.team.name}</span>
                    </button>
                  </td>
                  <td className={`px-3 py-2.5 text-muted ${num}`}>{r.team.seed}</td>
                  <td className={`px-3 py-2.5 text-muted ${num}`}>{r.team.score}</td>
                  <td className={`px-3 py-2.5 ${num}`}>{r.played}</td>
                  <td className={`px-3 py-2.5 ${num}`}>{r.wins}</td>
                  {league && <td className={`px-3 py-2.5 ${num}`}>{r.draws}</td>}
                  <td className={`px-3 py-2.5 ${num}`}>{r.losses}</td>
                  <td className={`px-3 py-2.5 ${num} ${diff > 0 ? 'text-win' : diff < 0 ? 'text-danger' : 'text-muted'}`}>{diff > 0 ? `+${diff}` : diff}</td>
                  {league && <td className={`px-3 py-2.5 font-semibold ${num}`}>{r.points}</td>}
                  <td className="px-4 py-2.5">
                    <StatusBadge status={r.status} />
                  </td>
                </tr>
              )
            })}
          </tbody>
        </table>
      </div>
    </div>
  )
}
