import type { Match, Slot, Tournament } from '../api/types'
import { roundNamer } from '../lib/labels'
import { bySchedule, planLine } from '../lib/schedule'
import { isRealMatch, matchState, resultsCsv, stageOrder, type MatchState } from '../lib/stats'
import { Avatar } from './Avatar'
import { ExportButton } from './ExportButton'

interface Props {
  tournament: Tournament
  teamName: (slot: Slot) => string
  onEdit?: (match: Match) => void
}

const groups: { state: MatchState; title: string; empty: string }[] = [
  { state: 'ready', title: 'Up next', empty: 'No match is waiting for a result.' },
  { state: 'waiting', title: 'Waiting for teams', empty: 'Every match has both teams.' },
  { state: 'played', title: 'Played', empty: 'No result recorded yet.' },
]

function TeamCell({ slot, name, won }: { slot: Slot; name: string; won: boolean }) {
  return (
    <span className={`flex min-w-0 items-center gap-2 ${slot.kind === 'team' ? '' : 'text-muted'} ${won ? 'font-semibold' : ''}`}>
      {slot.kind === 'team' && <Avatar name={name} size={20} />}
      <span className="truncate">{name}</span>
    </span>
  )
}

function Row({ match, label, teamName, onEdit }: { match: Match; label: string; teamName: Props['teamName']; onEdit?: Props['onEdit'] }) {
  const [a, b] = match.slots
  const plan = planLine(match)
  const editable = onEdit !== undefined && matchState(match) !== 'waiting'
  const score = match.forfeit ? 'Forfeit' : match.score ? `${match.score[0]} : ${match.score[1]}` : 'vs'
  const content = (
    <>
      <span className="w-28 shrink-0 text-xs text-muted">
        {label}
        {plan && <span className="block text-accent">{plan}</span>}
      </span>
      <TeamCell slot={a} name={teamName(a)} won={match.winner === 'A'} />
      <span className="shrink-0 text-center font-mono text-xs tabular-nums text-muted">{score}</span>
      <TeamCell slot={b} name={teamName(b)} won={match.winner === 'B'} />
    </>
  )
  const cls = 'grid w-full grid-cols-[7rem_1fr_4.5rem_1fr] items-center gap-3 px-4 py-3 text-left max-sm:grid-cols-[1fr_3.5rem_1fr] max-sm:[&>span:first-child]:col-span-3'
  return editable ? (
    <button type="button" onClick={() => onEdit(match)} className={`${cls} transition-colors hover:bg-line/30`}>
      {content}
    </button>
  ) : (
    <div className={cls}>{content}</div>
  )
}

export function MatchList({ tournament, teamName, onEdit }: Props) {
  const bracket = tournament.bracket
  if (!bracket) return null
  const label = roundNamer(bracket)
  const real = bracket.matches
    .filter(isRealMatch)
    .filter((m) => !(m.stage === 'grand_reset' && matchState(m) === 'waiting' && m.slots.every((s) => s.kind === 'empty') && bracket.matches.some((x) => x.stage === 'grand_final' && x.winner === 'A')))
    .sort((x, y) => stageOrder[x.stage] - stageOrder[y.stage] || x.round - y.round || x.position - y.position)
  return (
    <div className="flex flex-col gap-8">
      <div className="flex justify-end">
        <ExportButton filename={`${tournament.slug}-results.csv`} build={() => resultsCsv(tournament, label)} />
      </div>
      {groups.map((g) => {
        const items = g.state === 'played' ? real.filter((m) => matchState(m) === g.state) : bySchedule(real.filter((m) => matchState(m) === g.state))
        return (
          <section key={g.state} aria-label={g.title} className="flex flex-col gap-3">
            <h2 className="text-sm font-medium text-muted">
              {g.title} <span className="font-mono text-xs tabular-nums">{items.length}</span>
            </h2>
            {items.length === 0 ? (
              <p className="text-sm text-muted">{g.empty}</p>
            ) : (
              <ul className="divide-y divide-line overflow-hidden rounded-card border border-line bg-surface">
                {items.map((m) => (
                  <li key={m.id}>
                    <Row match={m} label={label(m)} teamName={teamName} onEdit={onEdit} />
                  </li>
                ))}
              </ul>
            )}
          </section>
        )
      })}
    </div>
  )
}
