import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { platform } from '../../api/platform'
import type { EventInfo } from '../../api/platformTypes'
import { useTournament } from '../../api/hooks'
import type { ChallengeAssignment, ChallengeSettings } from '../../api/types'
import { ConfirmDialog } from '../../components/ConfirmDialog'
import { Panel } from '../../components/platform/bits'
import { Button, ErrorText, Field, Input, Select } from '../../components/ui'

const MODES: { value: ChallengeAssignment; label: string; help: string }[] = [
  { value: 'none', label: 'No challenges', help: 'Scores are typed in by an organizer or a referee, as in any tournament.' },
  { value: 'manual', label: 'Organizer picks', help: 'You choose the challenges of every match from the match dialog.' },
  { value: 'per_match', label: 'Random per match', help: 'Kymo draws different challenges for each match, avoiding ones a team already played.' },
  { value: 'per_round', label: 'Random per round', help: 'Kymo draws once per round; every match of that round gets the same challenges.' },
]

/** How this tournament decides its matches, and who plays for each team. */
export function TournamentChallengeSettings({ event }: { event: EventInfo }) {
  const client = useQueryClient()
  const q = useTournament(event.slug)
  const t = q.data
  const [draft, setDraft] = useState<(ChallengeSettings & { minutes: string }) | null>(null)
  const [filling, setFilling] = useState(false)
  const save = useMutation({
    mutationFn: (s: ChallengeSettings) => platform.saveChallengeSettings(t!.id, s),
    onSuccess: (next) => {
      client.setQueryData(['tournament', event.slug], next)
      setDraft(null)
    },
  })
  const fill = useMutation({
    mutationFn: () => platform.fillRoster(t!.id),
    onSuccess: (next) => {
      client.setQueryData(['tournament', event.slug], next)
      setFilling(false)
    },
  })
  if (!t) return null
  const s = draft ?? { ...t.challenges, minutes: t.challenges.time_limit ? String(Math.round(t.challenges.time_limit / 60)) : '' }
  const set = (patch: Partial<ChallengeSettings & { minutes: string }>) => setDraft({ ...s, ...patch })
  const mode = MODES.find((m) => m.value === s.assignment)
  const linked = t.teams.filter((team) => team.user_id || team.player_team_id).length
  const count = event.mode === 'solo' ? s.solo : s.team
  return (
    <Panel className="flex flex-col gap-4">
      <div>
        <h2 className="text-lg font-semibold tracking-tight">Challenges decide the matches</h2>
        <p className="mt-1 text-sm text-muted">
          Each side answers the same challenges and its points become the match score. The format then decides wins, draws and standings as usual. Author the challenges in the Challenges tab; they are worth fixed points.
        </p>
      </div>
      <div className="grid gap-4 sm:grid-cols-2">
        <Field label="How challenges reach matches" hint={mode?.help}>
          <Select value={s.assignment} onChange={(e) => set({ assignment: e.target.value as ChallengeAssignment })}>
            {MODES.map((m) => (
              <option key={m.value} value={m.value}>
                {m.label}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Time limit (minutes)" hint="Optional. A match closes this long after its first answer. Empty means no limit.">
          <Input type="number" min={1} value={s.minutes} onChange={(e) => set({ minutes: e.target.value })} className="font-mono" />
        </Field>
        <Field label="Challenges per match, solo" hint={event.mode === 'solo' ? 'Used by this event.' : 'Used if this event is solo.'}>
          <Input type="number" min={1} max={10} value={s.solo} onChange={(e) => set({ solo: Number(e.target.value) })} className="font-mono" />
        </Field>
        <Field label="Challenges per match, team" hint={event.mode === 'team' ? 'Used by this event.' : 'Used if this event is in teams.'}>
          <Input type="number" min={1} max={10} value={s.team} onChange={(e) => set({ team: Number(e.target.value) })} className="font-mono" />
        </Field>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Button
          variant="primary"
          disabled={draft === null || save.isPending}
          onClick={() =>
            save.mutate({ assignment: s.assignment, solo: s.solo, team: s.team, seed: s.seed, time_limit: s.minutes.trim() === '' ? null : Math.round(Number(s.minutes) * 60) })
          }
        >
          Save
        </Button>
        {draft && (
          <Button variant="ghost" onClick={() => setDraft(null)}>
            Discard
          </Button>
        )}
        <span className="text-sm text-muted">
          Each match carries {count} {count === 1 ? 'challenge' : 'challenges'}.
        </span>
      </div>
      <ErrorText error={save.error} />
      {s.assignment !== 'none' && (
        <div className="flex flex-col gap-2 border-t border-line pt-4">
          <h3 className="text-sm font-semibold">Who plays for each team</h3>
          <p className="text-sm text-muted">
            {event.mode === 'solo' ? 'Each team here is one player.' : 'Each team here is a registered team.'} {linked} of {t.teams.length} roster entries are linked to registered{' '}
            {event.mode === 'solo' ? 'players' : 'teams'}. Only linked entries can receive challenges.
          </p>
          {t.status === 'draft' ? (
            <Button className="self-start" onClick={() => setFilling(true)}>
              Fill the roster from registrations
            </Button>
          ) : (
            <p className="text-sm text-muted">The roster is fixed once the tournament has started.</p>
          )}
        </div>
      )}
      {filling && (
        <ConfirmDialog
          title="Replace the roster?"
          message="The roster becomes everyone registered for this event, in the order they registered. Teams you added by hand are removed."
          confirmLabel="Fill the roster"
          pending={fill.isPending}
          onConfirm={() => fill.mutate()}
          onClose={() => setFilling(false)}
        />
      )}
      <ErrorText error={fill.error} />
    </Panel>
  )
}
