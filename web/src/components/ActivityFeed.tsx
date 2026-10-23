import { ArrowRight, CalendarClock, CircleSlash, Flag, History, Pencil, Play, RotateCcw, Trophy } from 'lucide-react'
import type { ReactNode } from 'react'
import type { EventKind, LogEvent, Slot, Tournament } from '../api/types'
import { useActivity } from '../api/hooks'
import { roundNamer } from '../lib/labels'
import { formatSlot } from '../lib/schedule'
import { timeAgo } from '../lib/time'
import { Avatar } from './Avatar'

const ICONS: Record<EventKind, ReactNode> = {
  started: <Play size={16} />,
  reset: <RotateCcw size={16} />,
  result: <Trophy size={16} />,
  corrected: <Pencil size={16} />,
  forfeit: <Flag size={16} />,
  cleared: <CircleSlash size={16} />,
  scheduled: <CalendarClock size={16} />,
}

interface Props {
  tournament: Tournament
  teamName: (slot: Slot) => string
  /** Show only the newest entries, for the stage screen. */
  limit?: number
  large?: boolean
}

export function describeEvent(e: LogEvent, name: (id: number | null | undefined) => string): { headline: ReactNode; detail?: string } {
  const a = name(e.data.team_a)
  const b = name(e.data.team_b)
  const scored = e.data.score_a != null && e.data.score_b != null
  const winner = e.data.winner === 'A' ? a : e.data.winner === 'B' ? b : null
  const loser = e.data.winner === 'A' ? b : a
  const line = scored ? `${e.data.score_a}:${e.data.score_b}` : ''
  switch (e.kind) {
    case 'started':
      return { headline: <>Tournament started with {e.data.teams ?? '?'} teams</> }
    case 'reset':
      return { headline: <>Bracket reset by the organizer</> }
    case 'result':
    case 'corrected': {
      const prefix = e.kind === 'corrected' ? 'Corrected: ' : ''
      return winner
        ? { headline: <>{prefix}<strong>{winner}</strong> beat {loser} <span className="font-mono tabular-nums">{line}</span></> }
        : { headline: <>{prefix}{a} and {b} drew <span className="font-mono tabular-nums">{line}</span></> }
    }
    case 'forfeit':
      return { headline: <><strong>{winner ?? a}</strong> advanced by forfeit against {loser}</> }
    case 'scheduled': {
      const when = e.data.scheduled_at == null ? null : formatSlot(e.data.scheduled_at)
      const where = e.data.venue ? ` in ${e.data.venue}` : ''
      return when || where
        ? { headline: <>{a} versus {b} planned for <strong>{when ?? 'a time to be set'}</strong>{where}</> }
        : { headline: <>Schedule removed for {a} versus {b}</> }
    }
    case 'cleared':
      return { headline: <>Result removed for {a} versus {b}</>, detail: 'The match is open again.' }
  }
}

export function ActivityFeed({ tournament, teamName, limit, large = false }: Props) {
  const { data, isPending, error } = useActivity(tournament.slug)
  const label = tournament.bracket ? roundNamer(tournament.bracket) : null
  const nameOf = (id: number | null | undefined) => (id == null ? 'TBD' : teamName({ kind: 'team', team: id }))
  const events = (data ?? []).slice(0, limit)

  if (isPending) return <p className="text-muted">Loading activity</p>
  if (error) return <p role="alert" className="text-danger">Activity could not be loaded.</p>
  if (events.length === 0)
    return (
      <div className="rounded-card border border-dashed border-field-line p-10 text-center">
        <History className="mx-auto mb-2 text-muted" size={20} aria-hidden="true" />
        <p className="text-lg font-medium">Nothing has happened yet</p>
        <p className="mt-1 text-muted">Every result, correction and forfeit appears here with its time.</p>
      </div>
    )

  return (
    <ol className="flex flex-col" aria-label="Activity">
      {events.map((e, i) => {
        const { headline, detail } = describeEvent(e, nameOf)
        const round = label && e.match_id !== null ? label(tournament.bracket!.matches[e.match_id]) : null
        const teams = [e.data.team_a, e.data.team_b].filter((id): id is number => id != null)
        return (
          <li key={e.id} className="relative flex gap-3 pb-5 last:pb-0">
            {i < events.length - 1 && <span className="absolute left-[15px] top-8 h-[calc(100%-1.5rem)] w-px bg-line" aria-hidden="true" />}
            <span className={`mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-full border border-line bg-surface ${e.kind === 'corrected' ? 'text-danger' : 'text-muted'}`}>{ICONS[e.kind]}</span>
            <div className="min-w-0 flex-1">
              <p className={large ? 'text-lg' : ''}>{headline}</p>
              <p className="flex flex-wrap items-center gap-x-2 text-xs text-muted">
                <time dateTime={new Date(e.at * 1000).toISOString()} title={new Date(e.at * 1000).toLocaleString()}>
                  {timeAgo(e.at)}
                </time>
                {round && (
                  <>
                    <ArrowRight size={10} aria-hidden="true" />
                    <span>{round}</span>
                  </>
                )}
                {detail && <span>{detail}</span>}
              </p>
            </div>
            {!large && teams.length > 0 && (
              <div className="hidden items-center sm:flex" aria-hidden="true">
                {teams.map((id) => (
                  <span key={id} className="-ml-1.5 rounded-full ring-2 ring-bg first:ml-0">
                    <Avatar name={nameOf(id)} size={22} />
                  </span>
                ))}
              </div>
            )}
          </li>
        )
      })}
    </ol>
  )
}
