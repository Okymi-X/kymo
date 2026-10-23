import { useEffect, useMemo, useRef, useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useNavigate, useParams, useSearchParams } from 'react-router'
import { api, useIsAdmin } from '../api/client'
import { useLiveUpdates, useTournament } from '../api/hooks'
import type { Match, Slot, Tournament } from '../api/types'
import { Avatar } from '../components/Avatar'
import { BracketView } from '../components/BracketView'
import { ConfirmDialog } from '../components/ConfirmDialog'
import { GroupsView } from '../components/GroupsView'
import { BackLink } from '../components/BackLink'
import { EmbedDialog } from '../components/EmbedDialog'
import { ActivityFeed } from '../components/ActivityFeed'
import { usePublishHeader } from '../components/HeaderContext'
import { MatchList } from '../components/MatchList'
import { ScheduleBoard } from '../components/ScheduleBoard'
import { RosterEditor } from '../components/RosterEditor'
import { ScoreDialog } from '../components/ScoreDialog'
import { StandingsTable } from '../components/StandingsTable'
import { Tabs, type TabItem } from '../components/Tabs'
import { TeamDialog } from '../components/TeamDialog'
import { TeamsGrid } from '../components/TeamsGrid'
import { ExportButton } from '../components/ExportButton'
import { Modal } from '../components/Modal'
import { Button, ErrorText, Field, Input, PageTitle, StatusPill } from '../components/ui'
import { formatLabel, isLeagueFormat, roundNamer } from '../lib/labels'
import { useFollowedTeam } from '../lib/follow'
import { accentProps, isHex } from '../lib/theme'
import { relativeStart } from '../lib/schedule'
import { computeTeamRows, teamHistory } from '../lib/stats'
import { CalendarPlus, Code, Monitor, Printer, X } from 'lucide-react'
import { Link } from 'react-router'

function useTeamName(t: Tournament | undefined) {
  return useMemo(() => {
    const byId = new Map(t?.teams.map((team) => [team.id, team.name]))
    return (slot: Slot) => (slot.kind === 'team' ? (byId.get(slot.team) ?? 'Unknown') : slot.kind === 'bye' ? 'Bye' : 'TBD')
  }, [t])
}

function Champion({ t, teamName }: { t: Tournament; teamName: (s: Slot) => string }) {
  if (t.champion === null) return null
  const name = teamName({ kind: 'team', team: t.champion })
  return (
    <p className="flex items-center gap-3 rounded-card border border-line bg-surface px-4 py-3">
      <span className="text-muted">Champion</span>
      <Avatar name={name} size={28} />
      <span className="font-semibold">{name}</span>
    </p>
  )
}

type Pending = 'reset' | 'delete' | 'rename' | 'color' | null

function FollowChip({ t, teamId, teamName, onOpen, onStop }: { t: Tournament; teamId: number; teamName: (s: Slot) => string; onOpen: () => void; onStop: () => void }) {
  const row = computeTeamRows(t).find((r) => r.team.id === teamId)
  if (!row) return null
  const label = t.bracket ? roundNamer(t.bracket) : () => ''
  const next = teamHistory(t, teamId).find((h) => h.result === 'next' || h.result === 'waiting')
  const opponent = next?.opponent == null ? 'an opponent to be decided' : teamName({ kind: 'team', team: next.opponent })
  const message =
    row.status === 'champion'
      ? 'Champion'
      : row.status === 'out'
        ? 'Eliminated'
        : next
          ? `${next.result === 'next' ? 'Up next' : 'Next'}: ${label(next.match)}, versus ${opponent}${next.match.scheduled_at === null ? '' : `, ${relativeStart(next.match.scheduled_at)}`}${next.match.venue ? ` in ${next.match.venue}` : ''}`
          : t.bracket
            ? 'No match left'
            : 'Waiting for the start'
  return (
    <div className="flex items-center gap-3 rounded-card border border-accent/40 bg-accent/5 px-4 py-3" role="status">
      <Avatar name={row.team.name} size={32} />
      <button type="button" onClick={onOpen} className="min-w-0 flex-1 text-left">
        <span className="block truncate font-medium">Following {row.team.name}</span>
        <span className="block truncate text-sm text-muted">{message}</span>
      </button>
      <button type="button" onClick={onStop} aria-label={`Stop following ${row.team.name}`} className="rounded-control p-2 text-muted hover:text-ink">
        <X size={16} />
      </button>
    </div>
  )
}

function AdminActions({ t }: { t: Tournament }) {
  const client = useQueryClient()
  const navigate = useNavigate()
  const [asking, setAsking] = useState<Pending>(null)
  const reset = useMutation({
    mutationFn: () => api.reset(t.id),
    onSuccess: (next) => {
      client.setQueryData(['tournament', next.slug], next)
      setAsking(null)
    },
  })
  const [name, setName] = useState(t.name)
  const duplicate = useMutation({
    mutationFn: () =>
      api.importSetup({
        name: `${t.name} copy`.slice(0, 80),
        format: t.format,
        third_place: t.third_place,
        groups: t.groups,
        advance: t.advance,
        rounds: t.rounds,
        teams: t.teams.map((team) => ({ name: team.name, score: team.score })),
      }),
    onSuccess: (copy) => {
      void client.invalidateQueries({ queryKey: ['tournaments'] })
      navigate(`/t/${copy.slug}`)
    },
  })
  const [color, setColor] = useState(t.accent ?? '#0a6fe0')
  const recolor = useMutation({
    mutationFn: (accent: string | null) => api.setAccent(t.id, accent),
    onSuccess: (next) => {
      client.setQueryData(['tournament', next.slug], next)
      setAsking(null)
    },
  })
  const rename = useMutation({
    mutationFn: () => api.rename(t.id, name.trim()),
    onSuccess: (next) => {
      client.setQueryData(['tournament', next.slug], next)
      void client.invalidateQueries({ queryKey: ['tournaments'] })
      setAsking(null)
    },
  })
  const remove = useMutation({
    mutationFn: () => api.remove(t.id),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: ['tournaments'] })
      navigate('/')
    },
  })
  return (
    <div className="flex flex-wrap items-center gap-2">
      <Button
        onClick={() => {
          setName(t.name)
          setAsking('rename')
        }}
      >
        Rename
      </Button>
      <Button
        onClick={() => {
          setColor(t.accent ?? '#0a6fe0')
          setAsking('color')
        }}
      >
        Color
      </Button>
      <Button onClick={() => duplicate.mutate()} disabled={duplicate.isPending}>
        Duplicate
      </Button>
      <ExportButton filename={`${t.slug}-backup.json`} build={() => JSON.stringify(t, null, 2)} label="Backup JSON" type="application/json" />
      {t.status !== 'draft' && <Button onClick={() => setAsking('reset')}>Reset bracket</Button>}
      <Button variant="danger" onClick={() => setAsking('delete')}>
        Delete
      </Button>
      <ErrorText error={reset.error ?? remove.error ?? duplicate.error} />
      {asking === 'color' && (
        <Modal label="Tournament color" onClose={() => setAsking(null)}>
          <form
            className="flex flex-col gap-4 p-6"
            onSubmit={(e) => {
              e.preventDefault()
              if (isHex(color)) recolor.mutate(color)
            }}
          >
            <div>
              <h2 className="text-lg font-semibold tracking-tight">Tournament color</h2>
              <p className="mt-1 text-sm text-muted">Tints buttons and highlights on this tournament, its stage screen and its embed.</p>
            </div>
            <div className="flex items-end gap-3">
              <input type="color" aria-label="Pick a color" value={isHex(color) ? color : '#0a6fe0'} onChange={(e) => setColor(e.target.value)} className="h-10 w-14 cursor-pointer rounded-control border border-field-line bg-field p-1" />
              <Field label="Hex color">
                <Input value={color} onChange={(e) => setColor(e.target.value)} maxLength={7} className="w-32 font-mono" />
              </Field>
            </div>
            <ErrorText error={recolor.error} />
            <div className="flex items-center justify-between gap-2">
              <Button type="button" variant="ghost" onClick={() => recolor.mutate(null)} disabled={recolor.isPending || t.accent === null}>
                Use site color
              </Button>
              <div className="flex gap-2">
                <Button type="button" variant="ghost" onClick={() => setAsking(null)}>
                  Cancel
                </Button>
                <Button type="submit" variant="primary" disabled={recolor.isPending || !isHex(color)}>
                  Save color
                </Button>
              </div>
            </div>
          </form>
        </Modal>
      )}
      {asking === 'rename' && (
        <Modal label="Rename tournament" onClose={() => setAsking(null)}>
          <form
            className="flex flex-col gap-4 p-6"
            onSubmit={(e) => {
              e.preventDefault()
              if (name.trim()) rename.mutate()
            }}
          >
            <Field label="Tournament name">
              <Input value={name} maxLength={80} onChange={(e) => setName(e.target.value)} autoFocus />
            </Field>
            <ErrorText error={rename.error} />
            <div className="flex justify-end gap-2">
              <Button type="button" variant="ghost" onClick={() => setAsking(null)}>
                Cancel
              </Button>
              <Button type="submit" variant="primary" disabled={rename.isPending || !name.trim()}>
                Save name
              </Button>
            </div>
          </form>
        </Modal>
      )}
      {asking === 'reset' && (
        <ConfirmDialog
          title="Reset the bracket?"
          message="All results are removed. The roster stays so you can start again."
          confirmLabel="Reset bracket"
          danger
          pending={reset.isPending}
          onConfirm={() => reset.mutate()}
          onClose={() => setAsking(null)}
        />
      )}
      {asking === 'delete' && (
        <ConfirmDialog
          title={`Delete ${t.name}?`}
          message="The tournament, its teams and its bracket are removed. This cannot be undone."
          confirmLabel="Delete"
          danger
          pending={remove.isPending}
          onConfirm={() => remove.mutate()}
          onClose={() => setAsking(null)}
        />
      )}
    </div>
  )
}

type View = 'bracket' | 'schedule' | 'standings' | 'groups' | 'matches' | 'teams' | 'activity'

const KNOCKOUT_VIEWS: TabItem<View>[] = [
  { id: 'bracket', label: 'Bracket' },
  { id: 'standings', label: 'Standings' },
  { id: 'matches', label: 'Matches' },
  { id: 'teams', label: 'Teams' },
  { id: 'activity', label: 'Activity' },
]

const GROUP_VIEWS: TabItem<View>[] = [
  { id: 'groups', label: 'Groups' },
  { id: 'bracket', label: 'Knockout' },
  { id: 'matches', label: 'Matches' },
  { id: 'teams', label: 'Teams' },
  { id: 'activity', label: 'Activity' },
]

const LEAGUE_VIEWS: TabItem<View>[] = [
  { id: 'standings', label: 'Standings' },
  { id: 'schedule', label: 'Schedule' },
  { id: 'matches', label: 'Matches' },
  { id: 'teams', label: 'Teams' },
  { id: 'activity', label: 'Activity' },
]

export function TournamentPage() {
  const { slug = '' } = useParams()
  const admin = useIsAdmin()
  const { data: t, isPending, error } = useTournament(slug)
  const teamName = useTeamName(t)
  const [editing, setEditing] = useState<Match | null>(null)
  const [teamId, setTeamId] = useState<number | null>(null)
  const [embedding, setEmbedding] = useState(false)
  const [params, setParams] = useSearchParams()
  const rows = useMemo(() => (t ? computeTeamRows(t) : []), [t])
  const views = t && isLeagueFormat(t.format) ? LEAGUE_VIEWS : t?.format === 'group_knockout' ? GROUP_VIEWS : KNOCKOUT_VIEWS
  const groupsDone = (t?.bracket?.matches ?? []).filter((m) => m.stage === 'league').every((m) => m.score !== null || m.forfeit)
  const fallback = t?.format === 'group_knockout' && groupsDone ? 'bracket' : views[0].id
  const requested = params.get('view')
  const view: View = views.find((v) => v.id === requested)?.id ?? fallback
  const knockout = useMemo(() => (t?.bracket ? { ...t.bracket, matches: t.bracket.matches.filter((m) => m.stage !== 'league') } : null), [t?.bracket])
  const [followed, follow] = useFollowedTeam(slug)
  const titleRef = useRef<HTMLDivElement>(null)
  const [titleHidden, setTitleHidden] = useState(false)
  useLiveUpdates(slug)

  useEffect(() => {
    const el = titleRef.current
    if (!el || typeof IntersectionObserver === 'undefined') return
    const observer = new IntersectionObserver(([entry]) => setTitleHidden(!entry.isIntersecting), { rootMargin: '-56px 0px 0px 0px' })
    observer.observe(el)
    return () => observer.disconnect()
  }, [t?.id])
  usePublishHeader(t && titleHidden ? { title: t.name, status: t.status, progress: t.progress } : null)

  if (isPending) return <p className="text-muted">Loading</p>
  if (error || !t) return <ErrorText error={error ?? new Error('Tournament not found')} />

  return (
    <div className="flex flex-col gap-8" {...accentProps(t.accent)}>
      <div className="-mb-4">
        <BackLink>Tournaments</BackLink>
      </div>
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div ref={titleRef}>
          <PageTitle>{t.name}</PageTitle>
          <p className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-muted">
            <span>
              {formatLabel[t.format]} · {t.teams.length} teams
              {t.progress.total > 0 && (
                <>
                  {' '}
                  · <span className="font-mono tabular-nums">{t.progress.played}/{t.progress.total}</span> matches played
                </>
              )}
            </span>
            <StatusPill status={t.status} />
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {t.bracket?.matches.some((m) => m.scheduled_at !== null) && (
            <a
              href={`/api/v1/tournaments/${encodeURIComponent(t.slug)}/calendar.ics`}
              className="inline-flex h-10 items-center gap-2 rounded-control border border-line bg-surface px-3.5 font-medium transition-colors hover:bg-line/40"
            >
              <CalendarPlus size={16} aria-hidden="true" /> Calendar
            </a>
          )}
          {t.bracket && (
            <>
              <button
                type="button"
                onClick={() => window.print()}
                className="no-print inline-flex h-10 items-center gap-2 rounded-control border border-line bg-surface px-3.5 font-medium transition-colors hover:bg-line/40"
              >
                <Printer size={16} aria-hidden="true" /> Print
              </button>
              <button
                type="button"
                onClick={() => setEmbedding(true)}
                className="no-print inline-flex h-10 items-center gap-2 rounded-control border border-line bg-surface px-3.5 font-medium transition-colors hover:bg-line/40"
              >
                <Code size={16} aria-hidden="true" /> Embed
              </button>
            </>
          )}
          {t.bracket && (
            <Link to={`/t/${t.slug}/stage`} className="inline-flex h-10 items-center gap-2 rounded-control border border-line bg-surface px-3.5 font-medium transition-colors hover:bg-line/40">
              <Monitor size={16} aria-hidden="true" /> Stage mode
            </Link>
          )}
          {admin && <AdminActions t={t} />}
        </div>
      </div>
      <Champion t={t} teamName={teamName} />
      {followed !== null && <FollowChip t={t} teamId={followed} teamName={teamName} onOpen={() => setTeamId(followed)} onStop={() => follow(null)} />}
      {t.bracket ? (
        <Tabs items={views} value={view} onChange={(v) => setParams(v === fallback ? {} : { view: v }, { replace: true })} label="Tournament views">
          {view === 'bracket' && knockout && <BracketView bracket={knockout} teamName={teamName} onEdit={admin ? setEditing : undefined} pinned={followed} />}
          {view === 'groups' && <GroupsView tournament={t} teamName={teamName} onSelect={setTeamId} onEdit={admin ? setEditing : undefined} />}
          {view === 'schedule' && <ScheduleBoard tournament={t} teamName={teamName} onEdit={admin ? setEditing : undefined} />}
          {view === 'standings' && <StandingsTable tournament={t} rows={rows} onSelect={setTeamId} />}
          {view === 'matches' && <MatchList tournament={t} teamName={teamName} onEdit={admin ? setEditing : undefined} />}
          {view === 'teams' && <TeamsGrid rows={rows} onSelect={setTeamId} />}
          {view === 'activity' && (
            <div className="max-w-2xl">
              <ActivityFeed tournament={t} teamName={teamName} paged />
            </div>
          )}
        </Tabs>
      ) : admin ? (
        <RosterEditor key={`${t.id}-draft`} tournament={t} />
      ) : (
        <div className="rounded-card border border-dashed border-field-line p-10 text-center">
          <p className="text-lg font-medium">The bracket is not ready</p>
          <p className="mt-1 text-muted">It appears here once the organizer starts the tournament.</p>
        </div>
      )}
      {embedding && <EmbedDialog tournament={t} onClose={() => setEmbedding(false)} />}
      {teamId !== null && (
        <TeamDialog
          tournament={t}
          teamId={teamId}
          teamName={teamName}
          onClose={() => setTeamId(null)}
          followed={followed === teamId}
          onToggleFollow={() => follow(followed === teamId ? null : teamId)}
        />
      )}
      {editing && (
        <ScoreDialog
          key={editing.id}
          tournament={t}
          match={t.bracket?.matches[editing.id] ?? editing}
          names={[teamName(editing.slots[0]), teamName(editing.slots[1])]}
          onClose={() => setEditing(null)}
        />
      )}
    </div>
  )
}
