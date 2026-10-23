import type { Slot, Tournament } from '../api/types'
import { roundNamer } from '../lib/labels'
import { computeTeamRows, teamHistory } from '../lib/stats'
import { Avatar } from './Avatar'
import { Modal } from './Modal'
import { StatusBadge } from './StatusBadge'
import { Button } from './ui'

interface Props {
  tournament: Tournament
  teamId: number
  teamName: (slot: Slot) => string
  onClose: () => void
}

function Stat({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="rounded-control border border-line px-3 py-2">
      <p className="text-xs text-muted">{label}</p>
      <p className="font-mono text-lg font-semibold tabular-nums">{value}</p>
    </div>
  )
}

const resultLabel = { win: 'Won', loss: 'Lost', next: 'Up next', waiting: 'Waiting' } as const
const resultStyle = { win: 'text-win', loss: 'text-danger', next: 'text-accent', waiting: 'text-muted' } as const

export function TeamDialog({ tournament, teamId, teamName, onClose }: Props) {
  const row = computeTeamRows(tournament).find((r) => r.team.id === teamId)
  if (!row) return null
  const history = teamHistory(tournament, teamId)
  const label = tournament.bracket ? roundNamer(tournament.bracket) : () => ''
  const opponent = (id: number | null) => teamName(id === null ? { kind: 'empty' } : { kind: 'team', team: id })
  const diff = row.pointsFor - row.pointsAgainst
  return (
    <Modal label={`${row.team.name} details`} onClose={onClose} width="32rem">
      <div className="flex flex-col gap-5 p-6">
        <div className="flex items-center gap-4">
          <Avatar name={row.team.name} size={56} />
          <div className="min-w-0">
            <h2 className="truncate text-xl font-semibold tracking-tight">{row.team.name}</h2>
            <div className="mt-1 flex items-center gap-2 text-sm text-muted">
              Seed {row.team.seed} <StatusBadge status={row.status} />
            </div>
          </div>
        </div>
        <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
          <Stat label="Qualifier" value={row.team.score} />
          <Stat label="Record" value={`${row.wins}-${row.losses}`} />
          <Stat label="Points" value={`${row.pointsFor}:${row.pointsAgainst}`} />
          <Stat label="Difference" value={diff > 0 ? `+${diff}` : diff} />
        </div>
        <div>
          <h3 className="mb-2 text-sm font-medium text-muted">Path through the bracket</h3>
          {history.length === 0 ? (
            <p className="text-sm text-muted">No matches yet.</p>
          ) : (
            <ol className="divide-y divide-line overflow-hidden rounded-control border border-line">
              {history.map((h) => (
                <li key={h.match.id} className="grid grid-cols-[6.5rem_1fr_auto] items-center gap-3 px-3 py-2.5 text-sm">
                  <span className="text-xs text-muted">{label(h.match)}</span>
                  <span className="truncate">vs {opponent(h.opponent)}</span>
                  <span className={`font-medium ${resultStyle[h.result]}`}>
                    {resultLabel[h.result]}
                    {h.forfeit ? ' (forfeit)' : h.own !== null ? ` ${h.own}:${h.other}` : ''}
                  </span>
                </li>
              ))}
            </ol>
          )}
        </div>
        <div className="flex justify-end">
          <Button variant="primary" onClick={onClose}>
            Close
          </Button>
        </div>
      </div>
    </Modal>
  )
}
