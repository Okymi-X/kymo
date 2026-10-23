import { useMemo, useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useNavigate, useParams, useSearchParams } from 'react-router'
import { api, useIsAdmin } from '../api/client'
import { useLiveUpdates, useTournament } from '../api/hooks'
import type { Match, Slot, Tournament } from '../api/types'
import { Avatar } from '../components/Avatar'
import { BracketView } from '../components/BracketView'
import { ConfirmDialog } from '../components/ConfirmDialog'
import { MatchList } from '../components/MatchList'
import { ScheduleBoard } from '../components/ScheduleBoard'
import { RosterEditor } from '../components/RosterEditor'
import { ScoreDialog } from '../components/ScoreDialog'
import { StandingsTable } from '../components/StandingsTable'
import { Tabs, type TabItem } from '../components/Tabs'
import { TeamDialog } from '../components/TeamDialog'
import { TeamsGrid } from '../components/TeamsGrid'
import { Button, ErrorText, PageTitle, StatusPill } from '../components/ui'
import { formatLabel } from '../lib/labels'
import { computeTeamRows } from '../lib/stats'

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

type Pending = 'reset' | 'delete' | null

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
  const remove = useMutation({
    mutationFn: () => api.remove(t.id),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: ['tournaments'] })
      navigate('/')
    },
  })
  return (
    <div className="flex flex-wrap items-center gap-2">
      {t.status !== 'draft' && <Button onClick={() => setAsking('reset')}>Reset bracket</Button>}
      <Button variant="danger" onClick={() => setAsking('delete')}>
        Delete
      </Button>
      <ErrorText error={reset.error ?? remove.error} />
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

type View = 'bracket' | 'schedule' | 'standings' | 'matches' | 'teams'

const KNOCKOUT_VIEWS: TabItem<View>[] = [
  { id: 'bracket', label: 'Bracket' },
  { id: 'standings', label: 'Standings' },
  { id: 'matches', label: 'Matches' },
  { id: 'teams', label: 'Teams' },
]

const LEAGUE_VIEWS: TabItem<View>[] = [
  { id: 'standings', label: 'Standings' },
  { id: 'schedule', label: 'Schedule' },
  { id: 'matches', label: 'Matches' },
  { id: 'teams', label: 'Teams' },
]

export function TournamentPage() {
  const { slug = '' } = useParams()
  const admin = useIsAdmin()
  const { data: t, isPending, error } = useTournament(slug)
  const teamName = useTeamName(t)
  const [editing, setEditing] = useState<Match | null>(null)
  const [teamId, setTeamId] = useState<number | null>(null)
  const [params, setParams] = useSearchParams()
  const rows = useMemo(() => (t ? computeTeamRows(t) : []), [t])
  const views = t?.format === 'round_robin' ? LEAGUE_VIEWS : KNOCKOUT_VIEWS
  const requested = params.get('view')
  const view: View = views.find((v) => v.id === requested)?.id ?? views[0].id
  useLiveUpdates(slug)

  if (isPending) return <p className="text-muted">Loading</p>
  if (error || !t) return <ErrorText error={error ?? new Error('Tournament not found')} />

  return (
    <div className="flex flex-col gap-8">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
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
        {admin && <AdminActions t={t} />}
      </div>
      <Champion t={t} teamName={teamName} />
      {t.bracket ? (
        <Tabs items={views} value={view} onChange={(v) => setParams(v === views[0].id ? {} : { view: v }, { replace: true })} label="Tournament views">
          {view === 'bracket' && <BracketView bracket={t.bracket} teamName={teamName} onEdit={admin ? setEditing : undefined} />}
          {view === 'schedule' && <ScheduleBoard tournament={t} teamName={teamName} onEdit={admin ? setEditing : undefined} />}
          {view === 'standings' && <StandingsTable tournament={t} rows={rows} onSelect={setTeamId} />}
          {view === 'matches' && <MatchList tournament={t} teamName={teamName} onEdit={admin ? setEditing : undefined} />}
          {view === 'teams' && <TeamsGrid rows={rows} onSelect={setTeamId} />}
        </Tabs>
      ) : admin ? (
        <RosterEditor key={`${t.id}-draft`} tournament={t} />
      ) : (
        <div className="rounded-card border border-dashed border-field-line p-10 text-center">
          <p className="text-lg font-medium">The bracket is not ready</p>
          <p className="mt-1 text-muted">It appears here once the organizer starts the tournament.</p>
        </div>
      )}
      {teamId !== null && <TeamDialog tournament={t} teamId={teamId} teamName={teamName} onClose={() => setTeamId(null)} />}
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
