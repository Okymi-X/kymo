import { Maximize2, Pause, Play } from 'lucide-react'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { Link, useParams } from 'react-router'
import type { Match, Slot, Tournament } from '../api/types'
import { useLiveUpdates, useTournament } from '../api/hooks'
import { ActivityFeed } from '../components/ActivityFeed'
import { Avatar } from '../components/Avatar'
import { BracketView } from '../components/BracketView'
import { GroupsView } from '../components/GroupsView'
import { StatusPill } from '../components/ui'
import { accentProps } from '../lib/theme'
import { stageFits } from '../lib/layout'
import { formatLabel, isLeagueFormat, roundNamer } from '../lib/labels'
import { bySchedule, planLine } from '../lib/schedule'
import { computeTeamRows, isRealMatch, matchState, type TeamRow } from '../lib/stats'

const ROTATE_MS = 15_000

const ROUND_PAGE = 8
const TABLE_PAGE = 10
const TABLE_PAGES_MAX = 3

type Slide =
  | { id: string; label: string; kind: 'round'; title: string; matches: Match[] }
  | { id: string; label: string; kind: 'bracket' }
  | { id: string; label: string; kind: 'standings'; rows: TeamRow[]; offset: number }
  | { id: string; label: string; kind: 'groups' }

function chunk<T>(items: T[], size: number): T[][] {
  const out: T[][] = []
  for (let i = 0; i < items.length; i += size) out.push(items.slice(i, i + size))
  return out
}

/** The screens to rotate through: the round being played, a standings table, a small bracket. */
function buildSlides(t: Tournament, rows: TeamRow[]): Slide[] {
  const bracket = t.bracket
  if (!bracket) return []
  const slides: Slide[] = []
  const tableSlides = (limit: number) => {
    const pages = chunk(rows, TABLE_PAGE).slice(0, limit)
    pages.forEach((page, i) =>
      slides.push({ id: `standings-${i}`, label: pages.length > 1 ? `Standings ${i + 1}/${pages.length}` : 'Standings', kind: 'standings', rows: page, offset: i * TABLE_PAGE }),
    )
  }
  if (t.format === 'swiss') {
    const latest = Math.max(...bracket.matches.map((m) => m.round))
    const open = bracket.matches.filter((m) => !m.slots.some((s) => s.kind === 'bye') && m.score === null && !m.forfeit)
    const round = open.length > 0 ? Math.min(...open.map((m) => m.round)) : latest
    const matches = bracket.matches.filter((m) => m.round === round && !m.slots.some((s) => s.kind === 'bye'))
    const title = `Round ${round} of ${bracket.swiss_rounds ?? latest}`
    chunk(matches, ROUND_PAGE).forEach((page, i, all) =>
      slides.push({ id: `round-${i}`, label: all.length > 1 ? `${title}, ${i + 1}/${all.length}` : title, kind: 'round', title, matches: page }),
    )
    tableSlides(TABLE_PAGES_MAX)
    return slides
  }
  if (t.format === 'round_robin') {
    tableSlides(Math.ceil(32 / TABLE_PAGE))
    return slides
  }
  if (t.format === 'group_knockout') slides.push({ id: 'groups', label: 'Groups', kind: 'groups' })
  const label = roundNamer(bracket)
  const real = bracket.matches.filter((m) => m.stage !== 'league' && isRealMatch(m))
  const rounds = new Map<string, Match[]>()
  for (const m of real) {
    const key = `${m.stage}-${m.round}`
    rounds.set(key, [...(rounds.get(key) ?? []), m])
  }
  const live = [...rounds.values()].filter((ms) => ms.some((m) => matchState(m) !== 'played'))
  const current = live[0]
  if (current) {
    const title = label(current[0]!)
    const pages = chunk(current, ROUND_PAGE)
    pages.forEach((page, i) =>
      slides.push({ id: `round-${i}`, label: pages.length > 1 ? `${title} ${i + 1}/${pages.length}` : title, kind: 'round', title, matches: page }),
    )
  }
  if (stageFits(bracket.matches.filter((m) => m.stage !== 'league'))) slides.push({ id: 'bracket', label: 'Bracket', kind: 'bracket' })
  if (t.format !== 'group_knockout') tableSlides(TABLE_PAGES_MAX)
  return slides
}

function useNames(t: Tournament | undefined) {
  return useMemo(() => {
    const byId = new Map(t?.teams.map((team) => [team.id, team.name]))
    return (slot: Slot) => (slot.kind === 'team' ? (byId.get(slot.team) ?? '') : slot.kind === 'bye' ? 'Bye' : 'TBD')
  }, [t])
}

function UpNext({ t, name }: { t: Tournament; name: (s: Slot) => string }) {
  const label = t.bracket ? roundNamer(t.bracket) : () => ''
  const next = bySchedule((t.bracket?.matches ?? []).filter((m) => isRealMatch(m) && matchState(m) === 'ready')).slice(0, 3)
  return (
    <section aria-label="Up next" className="flex flex-col gap-3">
      <h2 className="text-sm font-medium uppercase tracking-wider text-muted">Up next</h2>
      {next.length === 0 ? (
        <p className="text-muted">No match is waiting.</p>
      ) : (
        <ul className="flex flex-col gap-3">
          {next.map((m) => (
            <li key={m.id} className="rounded-card border border-line bg-surface p-4">
              <p className="mb-2 flex items-center justify-between gap-2 text-xs text-muted">
                <span>{label(m)}</span>
                {planLine(m) && <span className="font-medium text-accent">{planLine(m)}</span>}
              </p>
              {m.slots.map((slot, i) => (
                <div key={i} className="flex items-center gap-3 py-1 text-xl font-semibold tracking-tight">
                  <Avatar name={name(slot)} size={28} />
                  <span className="truncate">{name(slot)}</span>
                </div>
              ))}
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}

function StageMatch({ match, name, title }: { match: Match; name: (s: Slot) => string; title: string }) {
  const done = match.score !== null || match.forfeit
  return (
    <li className="rounded-card border border-line bg-surface p-5 shadow-card">
      <p className="mb-3 flex items-center justify-between gap-2 text-sm text-muted">
        <span>{title}</span>
        {planLine(match) ? <span className="font-medium text-accent">{planLine(match)}</span> : done && <span>{match.forfeit ? 'Forfeit' : 'Final'}</span>}
      </p>
      {match.slots.map((slot, i) => {
        const won = match.winner === (i === 0 ? 'A' : 'B')
        return (
          <div key={i} className={`flex items-center gap-3 py-1.5 text-2xl tracking-tight ${done && !won ? 'text-muted' : 'font-semibold'}`}>
            <Avatar name={name(slot)} size={32} />
            <span className="min-w-0 flex-1 truncate">{name(slot)}</span>
            {match.score && <span className="font-mono tabular-nums">{match.score[i]}</span>}
          </div>
        )
      })}
    </li>
  )
}

function StageTable({ rows, offset, league }: { rows: TeamRow[]; offset: number; league: boolean }) {
  return (
    <ol className="flex flex-col divide-y divide-line overflow-hidden rounded-card border border-line bg-surface text-2xl">
      {rows.map((r, i) => (
        <li key={r.team.id} className="flex items-center gap-4 px-5 py-3">
          <span className="w-10 font-mono text-lg tabular-nums text-muted">{offset + i + 1}</span>
          <Avatar name={r.team.name} size={32} />
          <span className="min-w-0 flex-1 truncate font-semibold tracking-tight">{r.team.name}</span>
          <span className="font-mono text-lg tabular-nums text-muted">{r.wins}W {r.losses}L</span>
          {league && <span className="w-16 text-right font-mono tabular-nums">{r.points}</span>}
        </li>
      ))}
    </ol>
  )
}

function Champion({ t, name }: { t: Tournament; name: string }) {
  return (
    <div className="stage-pop flex h-full flex-col items-center justify-center gap-6 text-center">
      <Avatar name={name} size={160} />
      <p className="text-sm font-medium uppercase tracking-[0.3em] text-muted">Champion</p>
      <p className="text-6xl font-semibold leading-none tracking-[-0.04em] sm:text-8xl">{name}</p>
      <p className="text-xl text-muted">{t.name}</p>
    </div>
  )
}

/** A full screen view for a projector: rotating bracket or tables, what is next, and live results. */
export function StagePage() {
  const { slug = '' } = useParams()
  const { data: t, isPending, error } = useTournament(slug)
  const name = useNames(t)
  const [index, setIndex] = useState(0)
  const [paused, setPaused] = useState(false)
  useLiveUpdates(slug)

  const rows = useMemo(() => (t ? computeTeamRows(t) : []), [t])
  const panels = useMemo(() => (t ? buildSlides(t, rows) : []), [t, rows])
  const active = panels[index % Math.max(panels.length, 1)]

  useEffect(() => {
    if (paused || panels.length < 2) return
    const timer = window.setInterval(() => setIndex((i) => i + 1), ROTATE_MS)
    return () => window.clearInterval(timer)
  }, [paused, panels.length])

  const toggleFullscreen = useCallback(() => {
    if (document.fullscreenElement) void document.exitFullscreen()
    else void document.documentElement.requestFullscreen().catch(() => undefined)
  }, [])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === ' ') {
        e.preventDefault()
        setPaused((p) => !p)
      } else if (e.key === 'ArrowRight') setIndex((i) => i + 1)
      else if (e.key === 'ArrowLeft') setIndex((i) => i + panels.length - 1)
      else if (e.key.toLowerCase() === 'f') toggleFullscreen()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [panels.length, toggleFullscreen])

  if (isPending) return <p className="p-10 text-muted">Loading</p>
  if (error || !t) return <p role="alert" className="p-10 text-danger">Tournament not found.</p>

  const champion = t.champion === null ? null : name({ kind: 'team', team: t.champion })
  const knockout = t.bracket ? { ...t.bracket, matches: t.bracket.matches.filter((m) => m.stage !== 'league') } : null

  return (
    <div className="flex min-h-screen flex-col gap-6 p-6 lg:p-10 xl:h-screen xl:overflow-hidden" {...accentProps(t.accent)}>
      <header className="flex flex-wrap items-center justify-between gap-4">
        <div className="flex min-w-0 items-center gap-4">
          <h1 className="truncate text-4xl font-semibold leading-none tracking-[-0.03em] lg:text-5xl">{t.name}</h1>
          <StatusPill status={t.status} />
          <span className="hidden text-lg text-muted sm:inline">{formatLabel[t.format]}</span>
          {t.progress.total > 0 && (
            <span className="font-mono text-lg tabular-nums text-muted">
              {t.progress.played}/{t.progress.total}
            </span>
          )}
        </div>
        <div className="flex items-center gap-2 text-muted">
          {!champion && panels.length > 1 && (
            <div className="mr-2 flex max-w-[50vw] items-center gap-1.5 overflow-x-auto" role="tablist" aria-label="Screens">
              {panels.map((p, i) => (
                <button
                  key={p.id}
                  type="button"
                  role="tab"
                  aria-selected={p.id === active?.id}
                  onClick={() => setIndex(i)}
                  className={`rounded-full px-3 py-1 text-sm font-medium transition-colors ${p.id === active?.id ? 'bg-ink text-bg' : 'hover:text-ink'}`}
                >
                  {p.label}
                </button>
              ))}
            </div>
          )}
          <button type="button" onClick={() => setPaused((p) => !p)} aria-label={paused ? 'Resume rotation' : 'Pause rotation'} className="rounded-control p-2 hover:text-ink">
            {paused ? <Play size={18} /> : <Pause size={18} />}
          </button>
          <button type="button" onClick={toggleFullscreen} aria-label="Toggle full screen" className="rounded-control p-2 hover:text-ink">
            <Maximize2 size={18} />
          </button>
          <Link to={`/t/${t.slug}`} className="rounded-control px-3 py-1.5 text-sm hover:text-ink">
            Exit
          </Link>
        </div>
      </header>

      {t.bracket === null ? (
        <div className="flex flex-1 items-center justify-center text-2xl text-muted">The bracket appears here once the tournament starts.</div>
      ) : (
        <div className="grid min-h-0 flex-1 gap-8 xl:grid-cols-[minmax(0,1fr)_22rem]">
          <main className="min-h-0 min-w-0 overflow-auto">
            {champion ? (
              <Champion t={t} name={champion} />
            ) : (
              <div key={active?.id} className="stage-fade">
                {active?.kind === 'round' && (
                  <div className="flex flex-col gap-4">
                    <h2 className="text-3xl font-semibold tracking-tight">{active.title}</h2>
                    <ul className="grid gap-4 md:grid-cols-2">
                      {active.matches.map((m) => (
                        <StageMatch key={m.id} match={m} name={name} title={`Match ${m.position + 1}`} />
                      ))}
                    </ul>
                  </div>
                )}
                {active?.kind === 'bracket' && knockout && (
                  <div className="h-[calc(100vh-9rem)] min-h-[24rem]">
                    <BracketView bracket={knockout} teamName={name} fill hideControls />
                  </div>
                )}
                {active?.kind === 'standings' && <StageTable rows={active.rows} offset={active.offset} league={isLeagueFormat(t.format)} />}
                {active?.kind === 'groups' && <GroupsView tournament={t} teamName={name} onSelect={() => undefined} />}
              </div>
            )}
          </main>
          <aside className="flex min-h-0 flex-col gap-6 xl:overflow-hidden">
            {!champion && <UpNext t={t} name={name} />}
            <section aria-label="Latest" className="flex flex-col gap-3">
              <h2 className="text-sm font-medium uppercase tracking-wider text-muted">Latest</h2>
              <ActivityFeed tournament={t} teamName={name} limit={4} large />
            </section>
          </aside>
        </div>
      )}
    </div>
  )
}
