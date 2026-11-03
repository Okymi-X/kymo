import { useQuery } from '@tanstack/react-query'
import { platform } from '../../api/platform'
import { useMyTeam } from '../../api/platformHooks'
import type { EventInfo } from '../../api/platformTypes'
import { timeAgo } from '../../lib/time'
import { ErrorText } from '../ui'
import { Empty, Loading, Panel, Stat } from './bits'

function TeamBreakdown({ slug, teamId }: { slug: string; teamId: number }) {
  const q = useQuery({ queryKey: ['team-progress', slug, teamId], queryFn: () => platform.teamProgress(teamId, slug) })
  if (!q.data) return <ErrorText error={q.error} />
  return (
    <Panel>
      <h3 className="mb-3 text-sm font-semibold">Your teammates</h3>
      <ul className="flex flex-col divide-y divide-line">
        {q.data.members.map((m) => (
          <li key={m.user_id} className="flex flex-col gap-1 py-3 first:pt-0 last:pb-0">
            <div className="flex items-baseline justify-between">
              <span className="font-medium">{m.display_name || m.username}</span>
              <span className="font-mono text-sm tabular-nums">{m.points} pts</span>
            </div>
            {m.solves.length === 0 ? (
              <span className="text-sm text-muted">No solves yet</span>
            ) : (
              <ul className="flex flex-wrap gap-1.5 text-xs">
                {m.solves.map((s) => (
                  <li key={s.challenge_id} className="rounded-full border border-line px-2 py-0.5" title={timeAgo(s.at)}>
                    {s.title} <span className="font-mono text-muted">{s.points}</span>
                  </li>
                ))}
              </ul>
            )}
          </li>
        ))}
      </ul>
    </Panel>
  )
}

export function MyProgress({ event }: { event: EventInfo }) {
  const q = useQuery({ queryKey: ['progress', event.slug], queryFn: () => platform.progress(event.slug) })
  const team = useMyTeam().data
  if (q.isPending) return <Loading />
  if (q.error) return <ErrorText error={q.error} />
  const p = q.data
  return (
    <div className="flex flex-col gap-6">
      <div className="grid gap-3 sm:grid-cols-3">
        <Stat label={event.mode === 'team' ? `Team ${p.name}` : 'Rank'} value={p.rank ?? 'none'} />
        <Stat label="Points" value={p.points} />
        <Stat label="Solved" value={`${p.solved_count}/${p.total_challenges}`} />
      </div>
      {p.solves.length === 0 ? (
        <Empty>No solves yet. Open the Challenges tab and find your first flag.</Empty>
      ) : (
        <Panel>
          <h3 className="mb-3 text-sm font-semibold">Solves</h3>
          <ul className="flex flex-col divide-y divide-line text-sm">
            {p.solves.map((s) => (
              <li key={s.challenge_id} className="flex items-baseline justify-between gap-3 py-2.5 first:pt-0 last:pb-0">
                <span className="font-medium">{s.title}</span>
                <span className="text-muted">
                  {timeAgo(s.at)}
                  {s.hints_cost > 0 && `, hints ${s.hints_cost}`}
                </span>
                <span className="w-16 text-right font-mono tabular-nums">{s.points}</span>
              </li>
            ))}
          </ul>
        </Panel>
      )}
      {event.mode === 'team' && team && <TeamBreakdown slug={event.slug} teamId={team.id} />}
    </div>
  )
}
