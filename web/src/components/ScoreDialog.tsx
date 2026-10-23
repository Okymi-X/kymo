import { useEffect, useRef, useState, type FormEvent } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from '../api/client'
import type { Match, Side, Tournament } from '../api/types'
import { fromLocalInput, toLocalInput } from '../lib/schedule'
import { Button, ErrorText, Field, Input } from './ui'

interface Props {
  tournament: Tournament
  match: Match
  names: [string, string]
  onClose: () => void
}

export function ScoreDialog({ tournament, match, names, onClose }: Props) {
  const ref = useRef<HTMLDialogElement>(null)
  const [a, setA] = useState(String(match.score?.[0] ?? ''))
  const [b, setB] = useState(String(match.score?.[1] ?? ''))
  const client = useQueryClient()
  const [when, setWhen] = useState(toLocalInput(match.scheduled_at))
  const [venue, setVenue] = useState(match.venue ?? '')

  useEffect(() => {
    ref.current?.showModal()
  }, [])

  const done = (t: Tournament) => {
    client.setQueryData(['tournament', t.slug], t)
    onClose()
  }
  const save = useMutation({
    mutationFn: () => api.setResult(tournament.id, match.id, Number(a), Number(b)),
    onSuccess: done,
  })
  const forfeit = useMutation({
    mutationFn: (winner: Side) => api.setForfeit(tournament.id, match.id, winner),
    onSuccess: done,
  })
  const schedule = useMutation({
    mutationFn: (clearIt: boolean) => api.setSchedule(tournament.id, match.id, clearIt ? null : fromLocalInput(when), clearIt ? null : venue.trim() || null),
    onSuccess: done,
  })
  const clear = useMutation({ mutationFn: () => api.clearResult(tournament.id, match.id), onSuccess: done })

  const league = match.stage === 'league'
  const valid = a !== '' && b !== '' && Number(a) >= 0 && Number(b) >= 0 && (league || Number(a) !== Number(b))
  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (valid) save.mutate()
  }

  return (
    <dialog
      ref={ref}
      onClose={onClose}
      onClick={(e) => e.target === ref.current && onClose()}
      className="m-auto w-[min(92vw,26rem)] rounded-card border border-line bg-surface p-0 text-ink"
    >
      <form onSubmit={submit} className="flex flex-col gap-5 p-6">
        <h2 className="text-lg font-semibold tracking-tight">Match result</h2>
        <div className="grid grid-cols-[1fr_5rem] items-end gap-x-3 gap-y-4">
          <span className="truncate pb-2 font-medium">{names[0]}</span>
          <Field label="Score">
            <Input type="number" min={0} inputMode="numeric" value={a} onChange={(e) => setA(e.target.value)} autoFocus className="font-mono" />
          </Field>
          <span className="truncate pb-2 font-medium">{names[1]}</span>
          <Field label="Score">
            <Input type="number" min={0} inputMode="numeric" value={b} onChange={(e) => setB(e.target.value)} className="font-mono" />
          </Field>
        </div>
        {!league && a !== '' && a === b && <p className="text-sm text-muted">Knockout matches cannot end tied.</p>}
        <div className="flex flex-col gap-2 border-t border-line pt-4">
          <p className="text-xs font-medium text-muted">No game played</p>
          <div className="grid grid-cols-2 gap-2">
            <Button onClick={() => forfeit.mutate('A')} disabled={forfeit.isPending}>
              <span className="truncate">{names[0]} advances</span>
            </Button>
            <Button onClick={() => forfeit.mutate('B')} disabled={forfeit.isPending}>
              <span className="truncate">{names[1]} advances</span>
            </Button>
          </div>
        </div>
        <div className="flex flex-col gap-3 border-t border-line pt-4">
          <p className="text-xs font-medium text-muted">Schedule</p>
          <div className="grid grid-cols-[1fr_1fr] gap-3 max-sm:grid-cols-1">
            <Field label="Starts">
              <Input type="datetime-local" value={when} onChange={(e) => setWhen(e.target.value)} />
            </Field>
            <Field label="Place">
              <Input value={venue} onChange={(e) => setVenue(e.target.value)} placeholder="Room A" maxLength={40} />
            </Field>
          </div>
          <div className="flex gap-2">
            <Button onClick={() => schedule.mutate(false)} disabled={schedule.isPending || (when === '' && venue.trim() === '')}>
              Save schedule
            </Button>
            {(match.scheduled_at !== null || match.venue) && (
              <Button variant="ghost" onClick={() => schedule.mutate(true)} disabled={schedule.isPending}>
                Clear schedule
              </Button>
            )}
          </div>
        </div>
        <ErrorText error={save.error ?? clear.error ?? forfeit.error ?? schedule.error} />
        <div className="flex items-center justify-between gap-2">
          {match.score || match.forfeit ? (
            <Button variant="danger" onClick={() => clear.mutate()} disabled={clear.isPending}>
              Clear result
            </Button>
          ) : (
            <span />
          )}
          <div className="flex gap-2">
            <Button variant="ghost" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" variant="primary" disabled={!valid || save.isPending}>
              Save
            </Button>
          </div>
        </div>
      </form>
    </dialog>
  )
}
