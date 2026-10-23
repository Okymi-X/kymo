import type { Format, Stage } from '../api/types'

export const formatLabel: Record<Format, string> = {
  single_elimination: 'Single elimination',
  double_elimination: 'Double elimination',
}

export function roundLabel(stage: Stage, round: number, totalRounds: number, format: Format): string {
  if (stage === 'grand_final') return 'Grand final'
  if (stage === 'grand_reset') return 'Reset'
  const fromEnd = totalRounds - round
  if (stage === 'losers') return fromEnd === 0 ? 'Losers final' : `Losers round ${round}`
  if (fromEnd === 0) return format === 'single_elimination' ? 'Final' : 'Winners final'
  if (fromEnd === 1) return 'Semifinals'
  if (fromEnd === 2) return 'Quarterfinals'
  return `Round ${round}`
}
