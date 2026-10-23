import { Link } from 'react-router'
import type { Tournament } from '../api/types'
import { formatLabel } from '../lib/labels'
import { Avatar } from './Avatar'
import { StatusPill } from './ui'

const STACK = 4

function AvatarStack({ names, total }: { names: string[]; total: number }) {
  const extra = total - names.length
  return (
    <div className="flex items-center" aria-hidden="true">
      {names.map((name) => (
        <span key={name} className="-ml-1.5 rounded-full ring-2 ring-surface first:ml-0">
          <Avatar name={name} size={24} />
        </span>
      ))}
      {extra > 0 && <span className="ml-2 font-mono text-xs tabular-nums text-muted">+{extra}</span>}
    </div>
  )
}

function Progress({ played, total }: { played: number; total: number }) {
  const pct = total === 0 ? 0 : Math.round((played / total) * 100)
  return (
    <div className="flex items-center gap-3">
      <div
        role="progressbar"
        aria-label="Matches played"
        aria-valuemin={0}
        aria-valuemax={total}
        aria-valuenow={played}
        className="h-1 flex-1 overflow-hidden rounded-full bg-line"
      >
        <div className="h-full rounded-full bg-accent transition-[width] duration-300" style={{ width: `${pct}%` }} />
      </div>
      <span className="font-mono text-xs tabular-nums text-muted">
        {played}/{total}
      </span>
    </div>
  )
}

export function TournamentCard({ t }: { t: Tournament }) {
  const names = t.teams.slice(0, STACK).map((team) => team.name)
  return (
    <Link
      to={`/t/${t.slug}`}
      className="group flex h-full flex-col gap-4 rounded-card border border-line bg-surface p-5 transition-[border-color,transform] duration-150 hover:border-field-line active:scale-[0.99]"
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="truncate text-base font-semibold tracking-tight">{t.name}</p>
          <p className="text-sm text-muted">
            {formatLabel[t.format]}
            {t.third_place ? ' with third place' : ''}
          </p>
        </div>
        <StatusPill status={t.status} />
      </div>
      <div className="mt-auto flex flex-col gap-3">
        {t.progress.total > 0 && <Progress played={t.progress.played} total={t.progress.total} />}
        <div className="flex items-center justify-between">
          {t.teams.length > 0 ? <AvatarStack names={names} total={t.teams.length} /> : <span className="text-sm text-muted">No teams yet</span>}
          <span className="text-sm text-muted">{t.teams.length} teams</span>
        </div>
      </div>
    </Link>
  )
}

export function CardSkeleton() {
  return <div className="h-36 animate-pulse rounded-card border border-line bg-surface" aria-hidden="true" />
}
