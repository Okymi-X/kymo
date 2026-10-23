import { describe, expect, it } from 'vitest'
import type { Match, Stage } from '../api/types'
import { CARD_H, CARD_W, ROW_GAP, layoutBracket } from './layout'

const match = (id: number, round: number, position: number, next: number | null, stage: Stage = 'winners'): Match => ({
  id,
  stage,
  round,
  position,
  slots: [{ kind: 'empty' }, { kind: 'empty' }],
  score: null,
  winner: null,
  forfeit: false,
  group: null,
  next_win: next === null ? null : { id: next, side: 'A' },
})

const at = (layout: ReturnType<typeof layoutBracket>, id: number) => layout.placed.find((p) => p.match.id === id)!

describe('layoutBracket', () => {
  const layout = layoutBracket([match(0, 1, 0, 2), match(1, 1, 1, 2), match(2, 2, 0, null)])

  it('stacks the first round', () => {
    expect(at(layout, 1).y - at(layout, 0).y).toBe(CARD_H + ROW_GAP)
  })

  it('centers a match between its feeders', () => {
    expect(at(layout, 2).y).toBe((at(layout, 0).y + at(layout, 1).y) / 2)
  })

  it('links each feeder to its destination', () => {
    expect(layout.links.map((l) => l.key).sort()).toEqual(['0-2', '1-2'])
  })

  it('keeps matches in one column from overlapping', () => {
    const crowded = layoutBracket([match(0, 1, 0, 2), match(1, 1, 1, 2), match(2, 2, 0, null), match(3, 2, 1, null)])
    const ys = crowded.placed.filter((p) => p.match.round === 2).map((p) => p.y)
    expect(ys[1] - ys[0]).toBeGreaterThanOrEqual(CARD_H + ROW_GAP)
  })

  it('has no group titles for a single bracket', () => {
    expect(layout.titles).toEqual([])
  })
})

describe('double elimination canvas', () => {
  const double = layoutBracket([
    match(0, 1, 0, 2),
    match(1, 1, 1, 2),
    match(2, 2, 0, 4),
    match(3, 1, 0, 4, 'losers'),
    match(4, 1, 0, null, 'grand_final'),
  ])

  it('puts losers below winners and finals to the right', () => {
    expect(at(double, 3).y).toBeGreaterThan(at(double, 2).y)
    expect(at(double, 4).x).toBeGreaterThanOrEqual(at(double, 2).x + CARD_W)
  })

  it('links both brackets into the grand final', () => {
    expect(double.links.map((l) => l.key)).toEqual(expect.arrayContaining(['2-4', '3-4']))
  })

  it('centers the grand final between the two finalists', () => {
    expect(at(double, 4).y).toBe((at(double, 2).y + at(double, 3).y) / 2)
  })

  it('titles each group', () => {
    expect(double.titles.map((t) => t.text)).toEqual(['Winners bracket', 'Losers bracket', 'Finals'])
  })
})

describe('third place match', () => {
  const layout = layoutBracket([
    match(0, 1, 0, 2),
    match(1, 1, 1, 2),
    match(2, 2, 0, null),
    match(3, 2, 0, null, 'third_place'),
  ])

  it('sits in the final column below the final', () => {
    expect(at(layout, 3).x).toBe(at(layout, 2).x)
    expect(at(layout, 3).y).toBeGreaterThanOrEqual(at(layout, 2).y + CARD_H + ROW_GAP)
  })

  it('adds no column of its own', () => {
    expect(new Set(layout.heads.map((h) => h.x)).size).toBe(2)
  })
})
