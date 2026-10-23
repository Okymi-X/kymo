import { Maximize2, Pause, Play } from 'lucide-react'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { Link, useParams } from 'react-router'
import type { Slot, Tournament } from '../api/types'
import { useLiveUpdates, useTournament } from '../api/hooks'
import { ActivityFeed } from '../components/ActivityFeed'
import { Avatar } from '../components/Avatar'
import { BracketView } from '../components/BracketView'
import { GroupsView } from '../components/GroupsView'
import { StandingsTable } from '../components/StandingsTable'
import { StatusPill } from '../components/ui'
import { formatLabel, roundNamer } from '../lib/labels'
import { computeTeamRows, isRealMatch, matchState } from '../lib/stats'

const ROTATE_MS = 15_000

type Panel = 'bracket' | 'standings' | 'groups'

const PANELS: Record<Tournament['format'], { id: Panel; label: string }[]> = {
  single_elimination: [{ id: 'bracket', label: 'Bracket' }, { id: 'standings', label: 'Standings' }],
  double_elimination: [{ id: 'bracket', label: 'Bracket' }, { id: 'standings', label: 'Standings' }],
  round_robin: [{ id: 'standings', label: 'Standings' }],
  group_knockout: [{ id: 'groups', label: 'Groups' }, { id: 'bracket', label: 'Knockout' }],
}

function useNames(t: Tournament | undefined) {
  return useMemo(() => {
    const byId = new Map(t?.teams.map((team) => [team.id, team.name]))
    return (slot: Slot) => (slot.kind === 'team' ? (byId.get(slot.team) ?? '') : slot.kind === 'bye' ? 'Bye' : 'TBD')
  }, [t])
}

function UpNext({ t, name }: { t: Tournament; name: (s: Slot) => string }) {
  const label = t.bracket ? roundNamer(t.bracket) : () => ''
  const next = (t.bracket?.matches ?? []).filter((m) => isRealMatch(m) && matchState(m) === 'ready').slice(0, 4)
  return (
    <section aria-label="Up next" className="flex flex-col gap-3">
      <h2 className="text-sm font-medium uppercase tracking-wider text-muted">Up next</h2>
      {next.length === 0 ? (
        <p className="text-muted">No match is waiting.</p>
      ) : (
        <ul className="flex flex-col gap-3">
          {next.map((m) => (
            <li key={m.id} className="rounded-card border border-line bg-surface p-4">
              <p className="mb-2 text-xs text-muted">{label(m)}</p>
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

  const panels = t ? PANELS[t.format] : []
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

  const rows = computeTeamRows(t)
  const champion = t.champion === null ? null : name({ kind: 'team', team: t.champion })
  const knockout = t.bracket ? { ...t.bracket, matches: t.bracket.matches.filter((m) => m.stage !== 'league') } : null

  return (
    <div className="flex min-h-screen flex-col gap-6 p-6 lg:p-10">
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
            <div className="mr-2 flex items-center gap-1.5" role="tablist" aria-label="Screens">
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
        <div className="grid flex-1 gap-8 xl:grid-cols-[minmax(0,1fr)_22rem]">
          <main className="min-w-0 overflow-auto">
            {champion ? (
              <Champion t={t} name={champion} />
            ) : (
              <div key={active?.id} className="stage-fade">
                {active?.id === 'bracket' && knockout && <BracketView bracket={knockout} teamName={name} contained hideControls />}
                {active?.id === 'standings' && <StandingsTable tournament={t} rows={rows} onSelect={() => undefined} hideExport />}
                {active?.id === 'groups' && <GroupsView tournament={t} teamName={name} onSelect={() => undefined} />}
              </div>
            )}
          </main>
          <aside className="flex flex-col gap-8">
            {!champion && <UpNext t={t} name={name} />}
            <section aria-label="Latest" className="flex flex-col gap-3">
              <h2 className="text-sm font-medium uppercase tracking-wider text-muted">Latest</h2>
              <ActivityFeed tournament={t} teamName={name} limit={6} large />
            </section>
          </aside>
        </div>
      )}
    </div>
  )
}
