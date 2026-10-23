import { ExternalLink } from 'lucide-react'
import { useEffect, useMemo } from 'react'
import { useParams, useSearchParams } from 'react-router'
import type { Slot, Tournament } from '../api/types'
import { useLiveUpdates, useTournament } from '../api/hooks'
import { BracketView } from '../components/BracketView'
import { GroupsView } from '../components/GroupsView'
import { StandingsTable } from '../components/StandingsTable'
import { StatusPill } from '../components/ui'
import { computeTeamRows } from '../lib/stats'

export type EmbedView = 'bracket' | 'standings' | 'groups'

export function defaultEmbedView(format: Tournament['format']): EmbedView {
  return format === 'round_robin' ? 'standings' : format === 'group_knockout' ? 'groups' : 'bracket'
}

/** A tournament view meant for an iframe or a stream overlay: no navigation, optional transparency. */
export function EmbedPage() {
  const { slug = '' } = useParams()
  const [params] = useSearchParams()
  const { data: t, isPending, error } = useTournament(slug)
  useLiveUpdates(slug)

  const theme = params.get('theme')
  const transparent = params.get('transparent') === '1'
  const showTitle = params.get('title') !== '0'

  useEffect(() => {
    if (theme === 'light' || theme === 'dark') document.documentElement.dataset.scheme = theme
    if (transparent) {
      document.documentElement.style.background = 'transparent'
      document.body.style.background = 'transparent'
    }
  }, [theme, transparent])

  const names = useMemo(() => {
    const byId = new Map(t?.teams.map((team) => [team.id, team.name]))
    return (slot: Slot) => (slot.kind === 'team' ? (byId.get(slot.team) ?? '') : slot.kind === 'bye' ? 'Bye' : 'TBD')
  }, [t])

  if (isPending) return <p className="p-4 text-muted">Loading</p>
  if (error || !t) return <p role="alert" className="p-4 text-danger">Tournament not found.</p>

  const requested = params.get('view')
  const view: EmbedView = requested === 'bracket' || requested === 'standings' || requested === 'groups' ? requested : defaultEmbedView(t.format)
  const knockout = t.bracket ? { ...t.bracket, matches: t.bracket.matches.filter((m) => m.stage !== 'league') } : null

  return (
    <div className="flex h-screen flex-col gap-4 p-4">
      {showTitle && (
        <header className="flex flex-wrap items-center gap-3">
          <h1 className="text-xl font-semibold tracking-tight">{t.name}</h1>
          <StatusPill status={t.status} />
          {t.progress.total > 0 && (
            <span className="font-mono text-sm tabular-nums text-muted">
              {t.progress.played}/{t.progress.total}
            </span>
          )}
        </header>
      )}
      <main className="min-h-0 min-w-0 flex-1 overflow-auto">
        {t.bracket === null ? (
          <p className="text-muted">The bracket appears here once the tournament starts.</p>
        ) : view === 'bracket' && knockout ? (
          <div className="h-full">
            <BracketView bracket={knockout} teamName={names} fill />
          </div>
        ) : view === 'groups' ? (
          <GroupsView tournament={t} teamName={names} onSelect={() => undefined} />
        ) : (
          <StandingsTable tournament={t} rows={computeTeamRows(t)} onSelect={() => undefined} hideExport />
        )}
      </main>
      <footer className="flex justify-end text-xs text-muted">
        <a href={`/t/${t.slug}`} target="_blank" rel="noopener noreferrer" className="inline-flex items-center gap-1 hover:text-ink">
          Open on Kymo <ExternalLink size={12} aria-hidden="true" />
        </a>
      </footer>
    </div>
  )
}
