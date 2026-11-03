import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Crown, RefreshCw } from 'lucide-react'
import { useState, type FormEvent } from 'react'
import { useWho } from '../api/client'
import { platform } from '../api/platform'
import { useMyTeam } from '../api/platformHooks'
import type { Member, Team } from '../api/platformTypes'
import { Avatar } from '../components/Avatar'
import { ConfirmDialog } from '../components/ConfirmDialog'
import { CopyButton, Gate, Loading, Panel } from '../components/platform/bits'
import { Button, ErrorText, Field, Input, PageTitle } from '../components/ui'
import { usePageTitle } from '../lib/usePageTitle'

function NoTeam() {
  const client = useQueryClient()
  const done = () => void client.invalidateQueries({ queryKey: ['team'] })
  const [name, setName] = useState('')
  const [code, setCode] = useState('')
  const create = useMutation({ mutationFn: () => platform.createTeam(name.trim()), onSuccess: done })
  const join = useMutation({ mutationFn: () => platform.joinTeam(code.trim()), onSuccess: done })
  const onCreate = (e: FormEvent) => {
    e.preventDefault()
    create.mutate()
  }
  const onJoin = (e: FormEvent) => {
    e.preventDefault()
    join.mutate()
  }
  return (
    <div className="grid gap-6 md:grid-cols-2">
      <Panel>
        <form onSubmit={onCreate} className="flex flex-col gap-4">
          <h2 className="text-lg font-semibold tracking-tight">Create a team</h2>
          <p className="text-sm text-muted">You become the owner and get an invite code to share.</p>
          <Field label="Team name">
            <Input value={name} onChange={(e) => setName(e.target.value)} maxLength={40} />
          </Field>
          <ErrorText error={create.error} />
          <Button type="submit" variant="primary" disabled={!name.trim() || create.isPending}>
            Create team
          </Button>
        </form>
      </Panel>
      <Panel>
        <form onSubmit={onJoin} className="flex flex-col gap-4">
          <h2 className="text-lg font-semibold tracking-tight">Join with a code</h2>
          <p className="text-sm text-muted">Ask the team owner for their invite code.</p>
          <Field label="Invite code">
            <Input value={code} onChange={(e) => setCode(e.target.value)} className="font-mono" autoComplete="off" />
          </Field>
          <ErrorText error={join.error} />
          <Button type="submit" disabled={!code.trim() || join.isPending}>
            Join team
          </Button>
        </form>
      </Panel>
    </div>
  )
}

function Roster({ team, me }: { team: Team; me: number }) {
  const client = useQueryClient()
  const done = () => void client.invalidateQueries({ queryKey: ['team'] })
  const owner = team.owner_user_id === me
  const [leaving, setLeaving] = useState(false)
  const [kicking, setKicking] = useState<Member | null>(null)
  const leave = useMutation({ mutationFn: () => platform.leaveTeam(team.id), onSuccess: done })
  const kick = useMutation({
    mutationFn: (user: number) => platform.kick(team.id, user),
    onSuccess: () => {
      setKicking(null)
      done()
    },
  })
  const rotate = useMutation({ mutationFn: () => platform.rotateCode(team.id), onSuccess: done })
  return (
    <div className="flex flex-col gap-6">
      {owner && team.invite_code && (
        <Panel className="flex flex-col gap-3">
          <h2 className="text-lg font-semibold tracking-tight">Invite code</h2>
          <p className="text-sm text-muted">Share this code with people you want on your team. A new code stops the old one working.</p>
          <div className="flex flex-wrap items-center gap-2">
            <code className="rounded-control border border-line bg-bg px-3 py-2 font-mono">{team.invite_code}</code>
            <CopyButton text={team.invite_code} />
            <Button onClick={() => rotate.mutate()} disabled={rotate.isPending}>
              <RefreshCw size={15} aria-hidden="true" /> New code
            </Button>
          </div>
          <ErrorText error={rotate.error} />
        </Panel>
      )}
      <Panel>
        <h2 className="mb-3 text-lg font-semibold tracking-tight">Members ({team.members.length})</h2>
        <ul className="flex flex-col divide-y divide-line">
          {team.members.map((m) => (
            <li key={m.user_id} className="flex items-center gap-3 py-3 first:pt-0 last:pb-0">
              <Avatar name={m.username} size={28} />
              <span className="min-w-0 flex-1">
                <span className="font-medium">{m.display_name || m.username}</span>
                <span className="ml-2 text-sm text-muted">@{m.username}</span>
              </span>
              {m.role === 'owner' && (
                <span className="inline-flex items-center gap-1 text-xs text-muted">
                  <Crown size={13} aria-hidden="true" /> Owner
                </span>
              )}
              {owner && m.user_id !== me && (
                <Button variant="danger" onClick={() => setKicking(m)}>
                  Remove
                </Button>
              )}
            </li>
          ))}
        </ul>
      </Panel>
      <div className="flex flex-col items-start gap-2">
        <Button variant="danger" onClick={() => setLeaving(true)}>
          Leave team
        </Button>
        <ErrorText error={leave.error ?? kick.error} />
      </div>
      {leaving && (
        <ConfirmDialog
          title="Leave this team?"
          message={owner ? 'Ownership passes to the member who has been here longest. If you are the last member the team is deleted.' : 'You can join again with an invite code.'}
          confirmLabel="Leave"
          danger
          pending={leave.isPending}
          onConfirm={() => leave.mutate(undefined, { onSettled: () => setLeaving(false) })}
          onClose={() => setLeaving(false)}
        />
      )}
      {kicking && (
        <ConfirmDialog
          title={`Remove ${kicking.display_name || kicking.username}?`}
          message="They lose access to the team's challenges and progress."
          confirmLabel="Remove"
          danger
          pending={kick.isPending}
          onConfirm={() => kick.mutate(kicking.user_id)}
          onClose={() => setKicking(null)}
        />
      )}
    </div>
  )
}

function TeamBody() {
  const who = useWho()
  const q = useMyTeam()
  if (q.isPending) return <Loading />
  if (q.error) return <ErrorText error={q.error} />
  if (!q.data) return <NoTeam />
  return (
    <div className="flex flex-col gap-6">
      <div className="flex items-center gap-3">
        <Avatar name={q.data.name} size={40} />
        <h2 className="text-2xl font-semibold tracking-tight">{q.data.name}</h2>
      </div>
      <Roster team={q.data} me={who?.id ?? 0} />
    </div>
  )
}

export function TeamPage() {
  usePageTitle('My team')
  return (
    <div className="flex max-w-3xl flex-col gap-6">
      <PageTitle>My team</PageTitle>
      <Gate need="player">
        <TeamBody />
      </Gate>
    </div>
  )
}
