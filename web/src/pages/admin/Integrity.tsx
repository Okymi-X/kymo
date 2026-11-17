import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { platform } from '../../api/platform'
import type { CodeMatch, EventInfo, IntegrityPlayer } from '../../api/platformTypes'
import { Modal, ModalBody, ModalHeader } from '../../components/Modal'
import { ConfirmDialog } from '../../components/ConfirmDialog'
import { DataTable, Empty, Loading, Pill, TD } from '../../components/platform/bits'
import { Button, ErrorText, Input } from '../../components/ui'
import { timeAgo } from '../../lib/time'

/** Where a player stands on the way from a warning to a ban. */
function Standing({ p }: { p: IntegrityPlayer }) {
  if (p.banned) return <Pill tone="text-danger">Banned</Pill>
  if (p.disqualified_at !== null) {
    return (
      <span title={p.disqualified_reason}>
        <Pill tone="text-danger">Disqualified</Pill>
      </span>
    )
  }
  return <Pill tone="text-accent">Warned</Pill>
}

/** Players caught sharing flags in this event, and the steps an organizer can take, gentlest first:
 *  forgive, disqualify from the event, ban the account. */
export function IntegrityTab({ event }: { event: EventInfo }) {
  const client = useQueryClient()
  const q = useQuery({ queryKey: ['integrity', event.id], queryFn: () => platform.integrity(event.id), refetchInterval: 20_000 })
  const done = () => {
    void client.invalidateQueries({ queryKey: ['integrity', event.id] })
    void client.invalidateQueries({ queryKey: ['scoreboard', event.slug] })
    void client.invalidateQueries({ queryKey: ['admin-participants', event.id] })
  }
  const forgive = useMutation({ mutationFn: (userId: number) => platform.forgiveStrikes(event.id, userId), onSuccess: done })
  const reinstate = useMutation({ mutationFn: (userId: number) => platform.reinstate(event.id, userId), onSuccess: done })
  const disqualify = useMutation({ mutationFn: (v: { userId: number; reason: string }) => platform.disqualify(event.id, v.userId, v.reason), onSuccess: done })
  const ban = useMutation({ mutationFn: (userId: number) => platform.updateUser(userId, { banned: true }), onSuccess: done })
  const [drafting, setDrafting] = useState<{ userId: number; reason: string } | null>(null)
  const [banning, setBanning] = useState<IntegrityPlayer | null>(null)
  if (q.isPending) return <Loading />
  const players = q.data?.players ?? []
  return (
    <div className="flex flex-col gap-8">
      <SimilarSolutions event={event} />
      {players.length === 0 ? (
        <Empty>No one has submitted another player's flag in this event.</Empty>
      ) : (
      <div className="flex flex-col gap-4">
      <DataTable caption="Players with strikes" head={['Player', 'Strikes', 'Where', 'Standing', '']}>
        {players.map((p) => {
          const gone = p.banned || p.disqualified_at !== null
          return (
            <tr key={p.user_id}>
              <td className={TD}>
                <span className="block font-medium">{p.display_name || p.username}</span>
                <span className="text-xs text-muted">@{p.username}</span>
              </td>
              <td className={TD}>
                <span className="font-mono tabular-nums">{p.strikes}</span>
                <span className="ml-2 text-xs text-muted">{timeAgo(p.last_at)}</span>
              </td>
              <td className={`${TD} max-w-xs text-xs text-muted`}>{p.challenges.map((c) => c.title).join(', ')}</td>
              <td className={TD}>
                <Standing p={p} />
              </td>
              <td className={TD}>
                {drafting?.userId === p.user_id ? (
                  <form
                    className="flex min-w-64 gap-2"
                    onSubmit={(e) => {
                      e.preventDefault()
                      if (drafting.reason.trim()) disqualify.mutate({ userId: p.user_id, reason: drafting.reason.trim() }, { onSuccess: () => setDrafting(null) })
                    }}
                  >
                    <Input value={drafting.reason} onChange={(e) => setDrafting({ userId: p.user_id, reason: e.target.value })} placeholder="Why" aria-label="Reason" maxLength={200} autoFocus className="min-w-0 flex-1" />
                    <Button type="submit" variant="danger" disabled={!drafting.reason.trim() || disqualify.isPending}>
                      Disqualify
                    </Button>
                    <Button variant="ghost" onClick={() => setDrafting(null)}>
                      Cancel
                    </Button>
                  </form>
                ) : (
                  <span className="flex flex-wrap gap-1.5">
                    {!gone && (
                      <Button variant="ghost" onClick={() => forgive.mutate(p.user_id)} disabled={forgive.isPending}>
                        Forgive
                      </Button>
                    )}
                    {p.disqualified_at !== null ? (
                      <Button variant="ghost" onClick={() => reinstate.mutate(p.user_id)} disabled={reinstate.isPending}>
                        Reinstate
                      </Button>
                    ) : (
                      !p.banned && (
                        <Button variant="ghost" onClick={() => setDrafting({ userId: p.user_id, reason: 'Flag sharing' })}>
                          Disqualify
                        </Button>
                      )
                    )}
                    {!p.banned && (
                      <Button variant="danger" onClick={() => setBanning(p)}>
                        Ban account
                      </Button>
                    )}
                  </span>
                )}
              </td>
            </tr>
          )
        })}
      </DataTable>
      <ErrorText error={forgive.error ?? reinstate.error ?? disqualify.error ?? ban.error} />
      </div>
      )}
      {banning && (
        <ConfirmDialog
          title={`Ban ${banning.username}?`}
          message="The account is signed out everywhere and cannot sign in again until you unban it on the Users page. This is for every event, not just this one."
          confirmLabel="Ban account"
          danger
          pending={ban.isPending}
          onConfirm={() => ban.mutate(banning.user_id, { onSuccess: () => setBanning(null) })}
          onClose={() => setBanning(null)}
        />
      )}
    </div>
  )
}

/** Accepted coding solutions that look copied from each other, to read side by side. */
function SimilarSolutions({ event }: { event: EventInfo }) {
  const client = useQueryClient()
  const q = useQuery({ queryKey: ['code-matches', event.id], queryFn: () => platform.codeMatches(event.id), refetchInterval: 30_000 })
  const dismiss = useMutation({ mutationFn: (id: number) => platform.dismissCodeMatch(id), onSuccess: () => void client.invalidateQueries({ queryKey: ['code-matches', event.id] }) })
  const [open, setOpen] = useState<CodeMatch | null>(null)
  const detail = useQuery({ queryKey: ['code-match', open?.id], queryFn: () => platform.codeMatch((open as CodeMatch).id), enabled: open !== null })
  const matches = q.data?.matches ?? []
  if (matches.length === 0) return null
  return (
    <section aria-label="Similar solutions" className="flex flex-col gap-3">
      <h2 className="text-sm font-semibold">Similar solutions</h2>
      <DataTable caption="Similar coding solutions" head={['Challenge', 'Between', 'Alike', 'Found', '']}>
        {matches.map((m) => (
          <tr key={m.id}>
            <td className={`${TD} font-medium`}>{m.challenge}</td>
            <td className={TD}>
              {m.other_name} and {m.name}
            </td>
            <td className={`${TD} font-mono tabular-nums ${m.score >= 95 ? 'text-danger' : ''}`}>{m.score}%</td>
            <td className={`${TD} text-muted`}>{timeAgo(m.at)}</td>
            <td className={TD}>
              <span className="flex gap-1.5">
                <Button variant="ghost" onClick={() => setOpen(m)}>
                  Compare
                </Button>
                <Button variant="ghost" onClick={() => dismiss.mutate(m.id)} disabled={dismiss.isPending}>
                  Dismiss
                </Button>
              </span>
            </td>
          </tr>
        ))}
      </DataTable>
      <ErrorText error={dismiss.error} />
      {open && (
        <Modal label="Compare solutions" onClose={() => setOpen(null)} width="64rem">
          <ModalHeader title={`${open.challenge}: ${open.score}% alike`} subtitle="Names and spacing are ignored when comparing; what you read here is exactly what was submitted." />
          <ModalBody>
            {detail.isPending ? (
              <Loading />
            ) : detail.data ? (
              <div className="grid gap-3 md:grid-cols-2">
                {[detail.data.b, detail.data.a].map((side) => (
                  <div key={side.name} className="min-w-0">
                    <p className="mb-1 text-sm font-medium">
                      {side.name} <span className="text-xs font-normal text-muted">{side.language}</span>
                    </p>
                    <pre className="max-h-96 overflow-auto whitespace-pre rounded-control border border-line bg-bg p-3 font-mono text-xs">{side.source}</pre>
                  </div>
                ))}
              </div>
            ) : (
              <ErrorText error={detail.error} />
            )}
          </ModalBody>
        </Modal>
      )}
    </section>
  )
}
