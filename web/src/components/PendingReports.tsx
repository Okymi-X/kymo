import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '../api/client'
import type { Slot, Tournament } from '../api/types'
import { Button, ErrorText } from './ui'

/** Scores that team captains submitted, waiting for an organizer or referee to approve them. */
export function PendingReports({ tournament, teamName }: { tournament: Tournament; teamName: (slot: Slot) => string }) {
  const client = useQueryClient()
  const reports = useQuery({ queryKey: ['reports'], queryFn: api.listReports, refetchInterval: 15_000 })
  const done = () => client.invalidateQueries({ queryKey: ['reports'] })
  const approve = useMutation({
    mutationFn: (id: number) => api.approveReport(id),
    onSuccess: (t) => {
      client.setQueryData(['tournament', t.slug], t)
      void done()
      void client.invalidateQueries({ queryKey: ['activity', t.slug] })
    },
    onError: () => void done(),
  })
  const reject = useMutation({ mutationFn: (id: number) => api.rejectReport(id), onSuccess: done })
  const mine = (reports.data ?? []).filter((r) => r.tournament_id === tournament.id)
  if (mine.length === 0) return null
  return (
    <section aria-label="Reported scores" className="flex flex-col gap-3 rounded-card border border-accent/40 bg-accent/5 p-4">
      <div>
        <h2 className="text-sm font-semibold">Reported scores {mine.length}</h2>
        <p className="text-xs text-muted">Captains submitted these. Nothing changes until you approve.</p>
      </div>
      <ul className="flex flex-col gap-3">
        {mine.map((r) => {
          const m = tournament.bracket?.matches[r.match_id]
          const a = m ? teamName(m.slots[0]) : '?'
          const b = m ? teamName(m.slots[1]) : '?'
          return (
            <li key={r.id} className="flex flex-wrap items-center justify-between gap-3">
              <p className="min-w-0 text-sm">
                <strong>{r.team}</strong> reports {a} <span className="font-mono tabular-nums">{r.score_a}:{r.score_b}</span> {b}
              </p>
              <div className="flex gap-2">
                <Button variant="primary" onClick={() => approve.mutate(r.id)} disabled={approve.isPending}>
                  Approve
                </Button>
                <Button variant="ghost" onClick={() => reject.mutate(r.id)} disabled={reject.isPending}>
                  Reject
                </Button>
              </div>
            </li>
          )
        })}
      </ul>
      <ErrorText error={approve.error ?? reject.error} />
    </section>
  )
}
