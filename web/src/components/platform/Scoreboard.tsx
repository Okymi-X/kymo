import { useQuery } from '@tanstack/react-query'
import { Check } from 'lucide-react'
import { platform } from '../../api/platform'
import { useEventStream } from '../../api/platformHooks'
import { timeAgo } from '../../lib/time'
import { Avatar } from '../Avatar'
import { ErrorText } from '../ui'
import { Empty, Loading } from './bits'

export function Scoreboard({ slug }: { slug: string }) {
  useEventStream(slug, true)
  const q = useQuery({ queryKey: ['scoreboard', slug], queryFn: () => platform.scoreboard(slug), refetchInterval: 30_000 })
  if (q.isPending) return <Loading label="Loading scoreboard" />
  if (q.error) return <ErrorText error={q.error} />
  const board = q.data
  if (board.rows.length === 0) return <Empty>Nobody has registered yet.</Empty>
  const matrix = board.challenges && board.challenges.length > 0 && board.challenges.length <= 40 ? board.challenges : null
  return (
    <div className="flex flex-col gap-3">
      {!board.public && <p className="text-sm text-muted">The scoreboard is not public. Only registered players and staff see it.</p>}
      <div className="overflow-x-auto rounded-card border border-line bg-surface">
        <table className="w-full min-w-[32rem] text-sm">
          <caption className="sr-only">Scoreboard</caption>
          <thead>
            <tr className="border-b border-line text-left text-xs text-muted">
              <th scope="col" className="w-14 px-4 py-3 font-medium">
                Rank
              </th>
              <th scope="col" className="px-4 py-3 font-medium">
                {board.mode === 'team' ? 'Team' : 'Player'}
              </th>
              <th scope="col" className="px-4 py-3 text-right font-medium">
                Solves
              </th>
              <th scope="col" className="px-4 py-3 text-right font-medium">
                Points
              </th>
              <th scope="col" className="hidden px-4 py-3 text-right font-medium sm:table-cell">
                Last solve
              </th>
              {matrix?.map((c) => (
                <th key={c.id} scope="col" title={c.title} className="hidden w-8 px-1 py-3 text-center font-mono text-[11px] font-medium lg:table-cell">
                  {c.id}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {board.rows.map((row) => (
              <tr key={row.participant_id} className="border-b border-line last:border-0">
                <td className="px-4 py-3 font-mono tabular-nums text-muted">{row.rank}</td>
                <td className="px-4 py-3">
                  <span className="flex items-center gap-2.5">
                    <Avatar name={row.avatar_seed || row.name} size={24} />
                    <span className="truncate font-medium">{row.name}</span>
                  </span>
                </td>
                <td className="px-4 py-3 text-right font-mono tabular-nums">{row.solves}</td>
                <td className="px-4 py-3 text-right font-mono tabular-nums font-semibold">{row.points}</td>
                <td className="hidden px-4 py-3 text-right text-muted sm:table-cell">{row.last_solve_at ? timeAgo(row.last_solve_at) : 'none'}</td>
                {matrix?.map((c) => (
                  <td key={c.id} className="hidden px-1 py-3 text-center lg:table-cell">
                    {row.solved?.includes(c.id) ? <Check size={14} aria-label={`Solved ${c.title}`} className="mx-auto text-win" /> : null}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}
