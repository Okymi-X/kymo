import { ArrowDown, ArrowUp } from 'lucide-react'
import { useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from '../api/client'
import type { Tournament } from '../api/types'
import type { TeamRow } from '../lib/stats'
import { Modal } from './Modal'
import { Button, ErrorText } from './ui'

interface Props {
  tournament: Tournament
  sets: TeamRow[][]
  onClose: () => void
}

/** Lets the organizer rank teams that are level on points, difference and goals. */
export function TieDialog({ tournament, sets, onClose }: Props) {
  const client = useQueryClient()
  const [orders, setOrders] = useState(() => sets.map((set) => set.map((r) => r.team)))
  const save = useMutation({
    mutationFn: () => {
      const chosen = orders.flat().map((t) => t.id)
      const kept = (tournament.bracket?.tiebreak ?? []).filter((id) => !chosen.includes(id))
      return api.setTiebreak(tournament.id, [...chosen, ...kept])
    },
    onSuccess: (t) => {
      client.setQueryData(['tournament', t.slug], t)
      onClose()
    },
  })
  const move = (set: number, index: number, by: -1 | 1) =>
    setOrders((prev) =>
      prev.map((list, i) => {
        if (i !== set) return list
        const next = [...list]
        ;[next[index], next[index + by]] = [next[index + by]!, next[index]!]
        return next
      }),
    )
  return (
    <Modal label="Decide ties" onClose={onClose} width="30rem">
      <div className="flex flex-col gap-5 p-6">
        <div>
          <h2 className="text-lg font-semibold tracking-tight">Decide ties</h2>
          <p className="mt-1 text-sm text-muted">These teams are level on points, difference and goals. Put the better team first. In a group, this also decides who qualifies.</p>
        </div>
        {orders.map((list, set) => (
          <ol key={set} aria-label={`Tied teams ${set + 1}`} className="flex flex-col divide-y divide-line overflow-hidden rounded-card border border-line">
            {list.map((team, i) => (
              <li key={team.id} className="flex items-center gap-2 px-3 py-2">
                <span className="w-5 font-mono text-xs tabular-nums text-muted">{i + 1}</span>
                <span className="min-w-0 flex-1 truncate font-medium">{team.name}</span>
                <button type="button" aria-label={`Move ${team.name} up`} disabled={i === 0} onClick={() => move(set, i, -1)} className="rounded-control p-2 text-muted hover:text-ink disabled:opacity-30">
                  <ArrowUp size={16} aria-hidden="true" />
                </button>
                <button type="button" aria-label={`Move ${team.name} down`} disabled={i === list.length - 1} onClick={() => move(set, i, 1)} className="rounded-control p-2 text-muted hover:text-ink disabled:opacity-30">
                  <ArrowDown size={16} aria-hidden="true" />
                </button>
              </li>
            ))}
          </ol>
        ))}
        <ErrorText error={save.error} />
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" onClick={() => save.mutate()} disabled={save.isPending}>
            Save order
          </Button>
        </div>
      </div>
    </Modal>
  )
}
