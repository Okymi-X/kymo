import type { Format, Stage } from '../api/types'

export const formatLabel: Record<Format, string> = {
  single_elimination: 'Single elimination',
  double_elimination: 'Double elimination',
  round_robin: 'Round robin',
}

export function roundLabel(stage: Stage, round: number, totalRounds: number, format: Format): string {
  if (stage === 'grand_final') return 'Grand final'
  if (stage === 'grand_reset') return 'Reset'
  if (stage === 'third_place') return 'Third place'
  if (stage === 'league') return `Round ${round}`
  const fromEnd = totalRounds - round
  if (stage === 'losers') return fromEnd === 0 ? 'Losers final' : `Losers round ${round}`
  if (fromEnd === 0) return format === 'single_elimination' ? 'Final' : 'Winners final'
  if (fromEnd === 1) return 'Semifinals'
  if (fromEnd === 2) return 'Quarterfinals'
  return `Round ${round}`
}

import type { Bracket, Match } from '../api/types'

/** Returns a function that names the round a match belongs to. */
export function roundNamer(bracket: Bracket): (m: Match) => string {
  const totals = new Map<Stage, number>()
  for (const m of bracket.matches) totals.set(m.stage, Math.max(totals.get(m.stage) ?? 0, m.round))
  return (m) => roundLabel(m.stage, m.round, totals.get(m.stage) ?? 0, bracket.format)
}
