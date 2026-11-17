import { useQuery } from '@tanstack/react-query'
import { Check, Snowflake } from 'lucide-react'
import { useState } from 'react'
import { useWho } from '../../api/client'
import { platform } from '../../api/platform'
import { useEventStream } from '../../api/platformHooks'
import type { BoardRow } from '../../api/platformTypes'
import { timeAgo } from '../../lib/time'
import { Avatar } from '../Avatar'
import { Button, ErrorText } from '../ui'
import { Empty, Loading } from './bits'
import { ScoreChart } from './ScoreChart'

const FIRST_ROWS = 25

/** The three in front, large: who they are and what they scored. */
function Podium({ rows, mine }: { rows: BoardRow[]; mine: number | null }) {
  return (
    <ol aria-label="Top three" className="grid gap-3 sm:grid-cols-3">
      {rows.map((row) => (
        <li key={row.participant_id} className={`flex items-center gap-3 rounded-card border bg-surface p-4 ${row.rank === 1 ? 'border-accent/60 shadow-[var(--shadow-card)]' : 'border-line'}`}>
          <span className={`flex size-9 shrink-0 items-center justify-center rounded-full font-mono text-sm font-semibold ${row.rank === 1 ? 'bg-accent text-bg' : 'border border-line text-muted'}`} aria-label={`Rank ${row.rank}`}>
            {row.rank}
          </span>
          <Avatar name={row.avatar_seed || row.name} size={32} />
          <span className="min-w-0 flex-1">
            <span className="block truncate font-medium">
              {row.name}
              {row.participant_id === mine && <span className="ml-2 text-xs font-normal text-accent">you</span>}
            </span>
            <span className="text-xs text-muted">{row.solves} solved</span>
          </span>
          <span className="font-mono text-lg font-semibold tabular-nums">{row.points}</span>
        </li>
      ))}
    </ol>
  )
}

export function Scoreboard({ slug }: { slug: string }) {
  useEventStream(slug, true)
  const who = useWho()
  const q = useQuery({ queryKey: ['scoreboard', slug], queryFn: () => platform.scoreboard(slug), refetchInterval: 30_000 })
  // Who "you" are on this board: the player's own participant (a user, or their team).
  const me = useQuery({ queryKey: ['progress', slug], queryFn: () => platform.progress(slug), enabled: who !== null, retry: false })
  const [grid, setGrid] = useState(false)
  const [all, setAll] = useState(false)
  if (q.isPending) return <Loading label="Loading scoreboard" />
  if (q.error) return <ErrorText error={q.error} />
  const board = q.data
  if (board.rows.length === 0) return <Empty>Nobody has registered yet.</Empty>
  const challenges = board.challenges && board.challenges.length > 0 && board.challenges.length <= 40 ? board.challenges : null
  const mine = me.data?.participant_id ?? null
  // The bar fills against the challenges there are, or against the best score when they are not listed.
  const total = challenges?.length ?? Math.max(1, ...board.rows.map((r) => r.solves))
  const rows = all ? board.rows : board.rows.slice(0, FIRST_ROWS)
  const podium = board.rows.length >= 3 ? board.rows.slice(0, 3) : []
  return (
    <div className="flex flex-col gap-5">
      {board.frozen_at && (
        <p role="status" className="flex items-center gap-2 rounded-control border border-line bg-surface px-3 py-2 text-sm text-muted">
          <Snowflake size={15} aria-hidden="true" className="text-accent" />
          Frozen at {new Date(board.frozen_at * 1000).toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' })}. The final board appears when the event ends.
        </p>
      )}
      {podium.length > 0 && <Podium rows={podium} mine={mine} />}
      {!board.public && <p className="text-sm text-muted">Only registered players and staff see this board.</p>}
      <div className="flex flex-col gap-3">
        {challenges && (
          <div className="flex justify-end">
            <Button variant="ghost" aria-pressed={grid} onClick={() => setGrid((v) => !v)}>
              {grid ? 'Hide challenge grid' : 'Show challenge grid'}
            </Button>
          </div>
        )}
        <div className="overflow-x-auto rounded-card border border-line bg-surface">
          <table className="w-full min-w-[26rem] text-sm">
            <caption className="sr-only">Scoreboard</caption>
            <thead>
              <tr className="border-b border-line text-left text-xs text-muted">
                <th scope="col" className="w-14 px-4 py-3 font-medium">#</th>
                <th scope="col" className="px-2 py-3 font-medium">{board.mode === 'team' ? 'Team' : 'Player'}</th>
                <th scope="col" className="hidden px-4 py-3 font-medium sm:table-cell">Solved</th>
                <th scope="col" className="px-4 py-3 text-right font-medium">Points</th>
                <th scope="col" className="hidden px-4 py-3 text-right font-medium md:table-cell">Last solve</th>
                {grid &&
                  challenges?.map((c, i) => (
                    <th key={c.id} scope="col" title={c.title} className="hidden w-8 px-1 py-3 text-center font-mono text-[11px] font-medium lg:table-cell">
                      {i + 1}
                    </th>
                  ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => {
                const you = row.participant_id === mine
                return (
                  <tr key={row.participant_id} className={`border-b border-line last:border-0 ${you ? 'bg-accent/5' : ''}`}>
                    <td className={`px-4 py-3 font-mono tabular-nums ${row.rank <= 3 ? 'font-semibold text-ink' : 'text-muted'}`}>{row.rank}</td>
                    <td className="px-2 py-2.5">
                      <span className="flex items-center gap-2.5">
                        <Avatar name={row.avatar_seed || row.name} size={24} />
                        <span className="truncate font-medium">{row.name}</span>
                        {you && <span className="text-xs text-accent">you</span>}
                      </span>
                    </td>
                    <td className="hidden w-2/5 px-4 py-3 sm:table-cell">
                      <span className="flex items-center gap-3">
                        <span aria-hidden="true" className="h-1.5 flex-1 overflow-hidden rounded-full bg-line">
                          <span className="block h-full rounded-full bg-accent" style={{ width: `${Math.min(100, (row.solves / total) * 100)}%` }} />
                        </span>
                        <span className="w-12 text-right font-mono text-xs tabular-nums text-muted">
                          {row.solves}
                          {challenges ? `/${challenges.length}` : ''}
                        </span>
                      </span>
                    </td>
                    <td className="px-4 py-3 text-right font-mono font-semibold tabular-nums">{row.points}</td>
                    <td className="hidden px-4 py-3 text-right text-muted md:table-cell">{row.last_solve_at ? timeAgo(row.last_solve_at) : '·'}</td>
                    {grid &&
                      challenges?.map((c) => (
                        <td key={c.id} className="hidden px-1 py-3 text-center lg:table-cell">
                          {row.solved?.includes(c.id) ? <Check size={14} aria-label={`Solved ${c.title}`} className="mx-auto text-win" /> : null}
                        </td>
                      ))}
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
        {board.rows.length > FIRST_ROWS && (
          <Button variant="ghost" className="self-center" onClick={() => setAll((v) => !v)}>
            {all ? 'Show fewer' : `Show all ${board.rows.length}`}
          </Button>
        )}
      </div>
      <ScoreChart slug={slug} />
    </div>
  )
}
