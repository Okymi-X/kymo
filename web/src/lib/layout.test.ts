import { describe, expect, it } from 'vitest'
import type { Match } from '../api/types'
import { CARD_H, ROW_GAP, layoutSection } from './layout'

const match = (id: number, round: number, position: number, next: number | null): Match => ({
  id,
  stage: 'winners',
  round,
  position,
  slots: [{ kind: 'empty' }, { kind: 'empty' }],
  score: null,
  winner: null,
  next_win: next === null ? null : { id: next, side: 'A' },
})

describe('layoutSection', () => {
  const layout = layoutSection([match(0, 1, 0, 2), match(1, 1, 1, 2), match(2, 2, 0, null)])
  const y = (id: number) => layout.placed.find((p) => p.match.id === id)!.y

  it('stacks the first round', () => {
    expect(y(1) - y(0)).toBe(CARD_H + ROW_GAP)
  })

  it('centers a match between its feeders', () => {
    expect(y(2)).toBe((y(0) + y(1)) / 2)
  })

  it('links each feeder to its destination', () => {
    expect(layout.links.map((l) => l.key).sort()).toEqual(['0-2', '1-2'])
  })

  it('keeps matches in one column from overlapping', () => {
    const crowded = layoutSection([match(0, 1, 0, 2), match(1, 1, 1, 2), match(2, 2, 0, null), match(3, 2, 1, null)])
    const ys = crowded.placed.filter((p) => p.match.round === 2).map((p) => p.y)
    expect(ys[1] - ys[0]).toBeGreaterThanOrEqual(CARD_H + ROW_GAP)
  })
})
