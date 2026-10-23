import { useMemo, useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useNavigate, useParams } from 'react-router'
import { api, useIsAdmin } from '../api/client'
import { useLiveUpdates, useTournament } from '../api/hooks'
import type { Match, Slot, Tournament } from '../api/types'
import { BracketView } from '../components/BracketView'
import { ConfirmDialog } from '../components/ConfirmDialog'
import { RosterEditor } from '../components/RosterEditor'
import { ScoreDialog } from '../components/ScoreDialog'
import { Button, ErrorText, StatusPill } from '../components/ui'
import { formatLabel } from '../lib/labels'

function useTeamName(t: Tournament | undefined) {
  return useMemo(() => {
    const byId = new Map(t?.teams.map((team) => [team.id, team.name]))
    return (slot: Slot) => (slot.kind === 'team' ? (byId.get(slot.team) ?? 'Unknown') : slot.kind === 'bye' ? 'Bye' : 'TBD')
  }, [t])
}

function Champion({ t, teamName }: { t: Tournament; teamName: (s: Slot) => string }) {
  if (t.champion === null) return null
  return (
    <p className="rounded-card border border-line bg-surface px-4 py-3">
      <span className="text-muted">Champion </span>
      <span className="font-semibold">{teamName({ kind: 'team', team: t.champion })}</span>
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

export function TournamentPage() {
  const { slug = '' } = useParams()
  const admin = useIsAdmin()
  const { data: t, isPending, error } = useTournament(slug)
  const teamName = useTeamName(t)
  const [editing, setEditing] = useState<Match | null>(null)
  useLiveUpdates(slug)

  if (isPending) return <p className="text-muted">Loading</p>
  if (error || !t) return <ErrorText error={error ?? new Error('Tournament not found')} />

  return (
    <div className="flex flex-col gap-8">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="text-4xl font-semibold tracking-tight">{t.name}</h1>
          <p className="mt-2 flex items-center gap-3 text-muted">
            {formatLabel[t.format]} · {t.teams.length} teams <StatusPill status={t.status} />
          </p>
        </div>
        {admin && <AdminActions t={t} />}
      </div>
      <Champion t={t} teamName={teamName} />
      {t.bracket ? (
        <BracketView bracket={t.bracket} teamName={teamName} onEdit={admin ? setEditing : undefined} />
      ) : admin ? (
        <RosterEditor key={t.teams.map((x) => x.id).join()} tournament={t} />
      ) : (
        <p className="text-muted">The bracket will appear here once the organizer starts the tournament.</p>
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
