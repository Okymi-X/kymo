import { describe, expect, it } from 'vitest'
import type { Bracket, Match, Slot } from '../api/types'
import { bracketImage } from './bracketImage'

const team = (id: number): Slot => ({ kind: 'team', team: id })
const match = (over: Partial<Match>): Match => ({
  id: 0,
  stage: 'winners',
  round: 1,
  position: 0,
  slots: [team(1), team(2)],
  score: [3, 1],
  winner: 'A',
  forfeit: false,
  games: [],
  group: null,
  scheduled_at: null,
  venue: null,
  next_win: null,
  ...over,
})

describe('bracketImage', () => {
  const bracket: Bracket = { format: 'single_elimination', matches: [match({})], tiebreak: [], best_of: 1 }
  const names: Record<number, string> = { 1: 'Alpha <b>', 2: 'Bravo & Co' }
  const name = (s: Slot) => (s.kind === 'team' ? names[s.team] : 'Bye')

  it('draws a standalone SVG with names, scores and an escaped title', () => {
    const { svg, width, height } = bracketImage(bracket, name, 'Cup "One"')
    expect(svg.startsWith('<svg xmlns="http://www.w3.org/2000/svg"')).toBe(true)
    expect(svg).toContain('Alpha &lt;b&gt;')
    expect(svg).toContain('Bravo &amp; Co')
    expect(svg).toContain('Cup &quot;One&quot;')
    expect(svg).not.toContain('<b>')
    expect(svg).toContain('>3<')
    expect(width).toBeGreaterThan(224)
    expect(height).toBeGreaterThan(76)
  })

  it('marks a forfeit and uses the dark palette on request', () => {
    const forfeit: Bracket = { ...bracket, matches: [match({ score: null, forfeit: true })] }
    const { svg } = bracketImage(forfeit, name, 'x', true)
    expect(svg).toContain('>W<')
    expect(svg).toContain('>FF<')
    expect(svg).toContain('#0b0b0c')
  })
})
