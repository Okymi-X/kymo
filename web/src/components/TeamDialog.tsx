import { Star } from 'lucide-react'
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
  followed?: boolean
  onToggleFollow?: () => void
}

function Stat({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="rounded-control border border-line px-3 py-2">
      <p className="text-xs text-muted">{label}</p>
      <p className="font-mono text-lg font-semibold tabular-nums">{value}</p>
    </div>
  )
}

const resultLabel = { win: 'Won', loss: 'Lost', draw: 'Drew', next: 'Up next', waiting: 'Waiting' } as const
const resultStyle = { win: 'text-win', loss: 'text-danger', draw: 'text-muted', next: 'text-accent', waiting: 'text-muted' } as const

export function TeamDialog({ tournament, teamId, teamName, onClose, followed = false, onToggleFollow }: Props) {
  const row = computeTeamRows(tournament).find((r) => r.team.id === teamId)
  if (!row) return null
  const history = teamHistory(tournament, teamId)
  const label = tournament.bracket ? roundNamer(tournament.bracket) : () => ''
  const opponent = (id: number | null) => teamName(id === null ? { kind: 'empty' } : { kind: 'team', team: id })
  const diff = row.pointsFor - row.pointsAgainst
  const league = tournament.format !== 'single_elimination' && tournament.format !== 'double_elimination'
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
          <Stat label="Record" value={league ? `${row.wins}-${row.draws}-${row.losses}` : `${row.wins}-${row.losses}`} />
          <Stat label={league ? 'League points' : 'Points'} value={league ? row.points : `${row.pointsFor}:${row.pointsAgainst}`} />
          <Stat label="Difference" value={diff > 0 ? `+${diff}` : diff} />
        </div>
        <div>
          <h3 className="mb-2 text-sm font-medium text-muted">{league ? 'Fixtures' : 'Path through the bracket'}</h3>
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
        <div className="flex items-center justify-between gap-2">
          {onToggleFollow ? (
            <Button onClick={onToggleFollow} aria-pressed={followed}>
              <Star size={16} aria-hidden="true" className={followed ? 'fill-current text-accent' : ''} />
              {followed ? 'Following' : 'Follow this team'}
            </Button>
          ) : (
            <span />
          )}
          <Button variant="primary" onClick={onClose}>
            Close
          </Button>
        </div>
      </div>
    </Modal>
  )
}
