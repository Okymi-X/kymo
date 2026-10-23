import type { TeamRow } from '../lib/stats'
import { Avatar } from './Avatar'
import { StatusBadge } from './StatusBadge'

export function TeamsGrid({ rows, onSelect }: { rows: TeamRow[]; onSelect: (teamId: number) => void }) {
  return (
    <ul className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
      {rows.map((r) => (
        <li key={r.team.id} className="min-w-0">
          <button
            type="button"
            onClick={() => onSelect(r.team.id)}
            className="flex w-full items-center gap-3 rounded-card border border-line bg-surface p-4 text-left shadow-[var(--shadow-card)] transition-[border-color,box-shadow,transform] duration-150 hover:border-field-line hover:shadow-[var(--shadow-lift)] active:scale-[0.99]"
          >
            <Avatar name={r.team.name} size={40} />
            <span className="min-w-0 flex-1">
              <span className="block truncate font-medium">{r.team.name}</span>
              <span className="block font-mono text-xs tabular-nums text-muted">
                Seed {r.team.seed} · {r.wins}-{r.losses}
              </span>
            </span>
            <StatusBadge status={r.status} />
          </button>
        </li>
      ))}
    </ul>
  )
}
