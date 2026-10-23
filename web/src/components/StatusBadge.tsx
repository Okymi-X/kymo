import type { TeamStatus } from '../lib/stats'

const styles: Record<TeamStatus, string> = {
  champion: 'text-win border-win/40',
  active: 'text-accent border-accent/40',
  out: 'text-muted border-line',
  seeded: 'text-muted border-line',
}

const labels: Record<TeamStatus, string> = { champion: 'Champion', active: 'In', out: 'Out', seeded: 'Seeded' }

export function StatusBadge({ status }: { status: TeamStatus }) {
  return <span className={`rounded-full border px-2 py-0.5 text-xs font-medium ${styles[status]}`}>{labels[status]}</span>
}
