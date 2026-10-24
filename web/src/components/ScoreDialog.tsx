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
  const bestOf = tournament.bracket?.best_of ?? 1
  const need = Math.floor(bestOf / 2) + 1
  const [games, setGames] = useState<[string, string][]>(() =>
    match.games.length > 0 ? match.games.map(([x, y]) => [String(x), String(y)] as [string, string]) : [['', '']],
  )
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
  const saveGames = useMutation({
    mutationFn: () => api.setGames(tournament.id, match.id, games.map(([x, y]) => [Number(x), Number(y)] as [number, number])),
    onSuccess: done,
  })
  const clear = useMutation({ mutationFn: () => api.clearResult(tournament.id, match.id), onSuccess: done })

  const league = match.stage === 'league'
  const valid = a !== '' && b !== '' && Number(a) >= 0 && Number(b) >= 0 && (league || Number(a) !== Number(b))
  // A series is complete when one side has a majority, no game is tied, and nothing follows the last game.
  const tally = (() => {
    let a = 0
    let b = 0
    let bad = false
    let over = false
    for (const [x, y] of games) {
      if (over || x === '' || y === '' || Number(x) === Number(y)) {
        bad = true
        continue
      }
      if (Number(x) > Number(y)) a++
      else b++
      over = a === need || b === need
    }
    return { a, b, bad, over }
  })()
  const seriesValid = bestOf > 1 && !tally.bad && tally.over
  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (bestOf > 1 ? seriesValid : valid) (bestOf > 1 ? saveGames : save).mutate()
  }

  return (
    <dialog
      ref={ref}
      onClose={onClose}
      onClick={(e) => e.target === ref.current && onClose()}
      className="m-auto w-[min(92vw,28rem)] overflow-x-hidden rounded-card border border-line bg-surface p-0 text-ink"
    >
      <form onSubmit={submit} className="flex flex-col gap-5 p-6">
        <h2 className="text-lg font-semibold tracking-tight">Match result</h2>
        {bestOf > 1 ? (
        <div className="flex flex-col gap-3">
          <p className="text-sm text-muted">
            Best of {bestOf}: first to {need} games. Series{' '}
            <span className="font-mono font-medium text-ink">
              {tally.a}:{tally.b}
            </span>
          </p>
          {games.map(([x, y], i) => (
            <div key={i} className="grid grid-cols-[1fr_4.5rem_4.5rem] items-center gap-2">
              <span className="text-sm font-medium">Game {i + 1}</span>
              {[x, y].map((value, side) => (
                <Input
                  key={side}
                  type="number"
                  min={0}
                  inputMode="numeric"
                  aria-label={`Game ${i + 1} score for ${names[side]}`}
                  value={value}
                  onChange={(e) => setGames((g) => g.map((row, j) => (j === i ? (side === 0 ? [e.target.value, row[1]] : [row[0], e.target.value]) : row) as [string, string]))}
                  className="text-center font-mono"
                />
              ))}
            </div>
          ))}
          <div className="flex gap-2">
            <Button onClick={() => setGames((g) => [...g, ['', '']])} disabled={games.length >= bestOf || tally.over}>
              Add game
            </Button>
            <Button variant="ghost" onClick={() => setGames((g) => g.slice(0, -1))} disabled={games.length <= 1}>
              Remove last
            </Button>
          </div>
          <p className="text-xs text-muted">
            {names[0]} is the first score of each game, {names[1]} the second.
          </p>
        </div>
        ) : (
          <>
        <div className="flex flex-col gap-2">
          {([[names[0], a, setA], [names[1], b, setB]] as const).map(([name, value, set], i) => (
            <div key={i} className="flex items-center gap-3 rounded-control border border-line px-3 py-2">
              <span className="min-w-0 flex-1 truncate font-medium">{name}</span>
              <Input
                type="number"
                min={0}
                inputMode="numeric"
                aria-label={`Score for ${name}`}
                value={value}
                onChange={(e) => set(e.target.value)}
                autoFocus={i === 0}
                className="w-20 shrink-0 text-center font-mono"
              />
            </div>
          ))}
        </div>
        {!league && a !== '' && a === b && <p className="text-sm text-muted">Knockout matches cannot end tied.</p>}
          </>
        )}
        <div className="flex flex-col gap-2 border-t border-line pt-4">
          <p className="text-xs font-medium text-muted">No game played</p>
          <div className="flex flex-col gap-2">
            <Button onClick={() => forfeit.mutate('A')} disabled={forfeit.isPending} className="justify-start">
              <span className="truncate">{names[0]} advances</span>
            </Button>
            <Button onClick={() => forfeit.mutate('B')} disabled={forfeit.isPending} className="justify-start">
              <span className="truncate">{names[1]} advances</span>
            </Button>
          </div>
        </div>
        <div className="flex flex-col gap-3 border-t border-line pt-4">
          <p className="text-xs font-medium text-muted">Schedule</p>
          <div className="flex flex-col gap-3">
            <Field label="Starts">
              <Input type="datetime-local" value={when} onChange={(e) => setWhen(e.target.value)} className="w-full min-w-0" />
            </Field>
            <Field label="Place">
              <Input value={venue} onChange={(e) => setVenue(e.target.value)} placeholder="Room A" maxLength={40} className="w-full min-w-0" />
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
        <ErrorText error={save.error ?? saveGames.error ?? clear.error ?? forfeit.error ?? schedule.error} />
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
            <Button type="submit" variant="primary" disabled={(bestOf > 1 ? !seriesValid : !valid) || save.isPending || saveGames.isPending}>
              Save
            </Button>
          </div>
        </div>
      </form>
    </dialog>
  )
}
