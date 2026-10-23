import { useMemo } from 'react'
import type { Bracket, Match, Slot, Stage } from '../api/types'
import { roundLabel } from '../lib/labels'
import { MatchCard } from './MatchCard'

interface Props {
  bracket: Bracket
  teamName: (slot: Slot) => string
  onEdit?: (match: Match) => void
}

interface Column {
  key: string
  label: string
  matches: Match[]
}

const isVoidReset = (m: Match) => m.stage === 'grand_reset' && m.slots.every((s) => s.kind !== 'team')

function columnsFor(bracket: Bracket, stages: Stage[], totals: Map<Stage, number>): Column[] {
  const groups = new Map<string, Match[]>()
  for (const m of bracket.matches) {
    if (!stages.includes(m.stage) || isVoidReset(m)) continue
    const key = `${m.stage}-${m.round}`
    groups.set(key, [...(groups.get(key) ?? []), m])
  }
  return [...groups.entries()].map(([key, matches]) => ({
    key,
    label: roundLabel(matches[0].stage, matches[0].round, totals.get(matches[0].stage) ?? 0, bracket.format),
    matches: matches.sort((a, b) => a.position - b.position),
  }))
}

function Section({ title, columns, teamName, onEdit }: { title?: string; columns: Column[] } & Omit<Props, 'bracket'>) {
  if (columns.length === 0) return null
  return (
    <section aria-label={title}>
      {title && <h3 className="mb-4 text-xs font-medium uppercase tracking-wider text-muted">{title}</h3>}
      <div className="flex gap-10 overflow-x-auto pb-4">
        {columns.map((col) => (
          <div key={col.key} className="flex flex-col gap-3">
            <h4 className="text-xs font-medium text-muted">{col.label}</h4>
            <div className="flex flex-1 flex-col justify-around gap-3">
              {col.matches.map((m) => (
                <MatchCard key={m.id} match={m} teamName={teamName} onEdit={onEdit} />
              ))}
            </div>
          </div>
        ))}
      </div>
    </section>
  )
}

export function BracketView({ bracket, teamName, onEdit }: Props) {
  const sections = useMemo(() => {
    const totals = new Map<Stage, number>()
    for (const m of bracket.matches) totals.set(m.stage, Math.max(totals.get(m.stage) ?? 0, m.round))
    const double = bracket.format === 'double_elimination'
    return [
      { title: double ? 'Winners bracket' : undefined, columns: columnsFor(bracket, ['winners'], totals) },
      { title: 'Losers bracket', columns: columnsFor(bracket, ['losers'], totals) },
      { title: 'Finals', columns: columnsFor(bracket, ['grand_final', 'grand_reset'], totals) },
    ]
  }, [bracket])

  return (
    <div className="flex flex-col gap-10">
      {sections.map((s, i) => (
        <Section key={i} title={s.title} columns={s.columns} teamName={teamName} onEdit={onEdit} />
      ))}
    </div>
  )
}
