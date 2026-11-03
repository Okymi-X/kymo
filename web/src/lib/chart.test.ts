import { describe, expect, it } from 'vitest'
import { niceTicks, paddedRange, stepPath, timeLabel, timeTicks, valueAt } from './chart'

describe('niceTicks', () => {
  it('starts at zero, covers the maximum and uses round steps', () => {
    expect(niceTicks(285)).toEqual([0, 100, 200, 300])
    expect(niceTicks(100, 4)).toEqual([0, 25, 50, 75, 100])
    const t = niceTicks(7, 5)
    expect(t[0]).toBe(0)
    expect(t[t.length - 1]).toBeGreaterThanOrEqual(7)
  })
  it('never returns an empty or flat axis', () => {
    expect(niceTicks(0).length).toBeGreaterThan(1)
    expect(new Set(niceTicks(0)).size).toBe(niceTicks(0).length)
  })
})

describe('timeTicks', () => {
  it('stays inside the range and is evenly spaced', () => {
    const from = 1_800_000_000
    const to = from + 3 * 3600 + 17 * 60
    const ticks = timeTicks(from, to)
    expect(ticks.length).toBeGreaterThan(1)
    expect(ticks.every((t) => t >= from && t <= to)).toBe(true)
    const gaps = ticks.slice(1).map((t, i) => t - ticks[i])
    expect(new Set(gaps).size).toBe(1)
  })
  it('chooses coarser steps for longer spans', () => {
    const day = timeTicks(0, 86_400 * 14)
    expect(day[1] - day[0]).toBeGreaterThanOrEqual(86_400)
  })
})

describe('timeLabel', () => {
  it('uses a clock time for a short span and a date for a long one', () => {
    expect(timeLabel(1_800_000_000, 3600)).toMatch(/\d/)
    expect(timeLabel(1_800_000_000, 20 * 86_400)).not.toMatch(/:/)
  })
})

describe('valueAt', () => {
  const steps = [
    { at: 10, total: 100 },
    { at: 20, total: 150 },
  ]
  it('is zero before the first solve and steps up at each solve', () => {
    expect(valueAt(steps, 5)).toBe(0)
    expect(valueAt(steps, 10)).toBe(100)
    expect(valueAt(steps, 19)).toBe(100)
    expect(valueAt(steps, 99)).toBe(150)
    expect(valueAt([], 5)).toBe(0)
  })
})

describe('stepPath', () => {
  it('stays flat, then curves into the next level, then runs flat to the end', () => {
    const d = stepPath({ x: 0, y: 100 }, [{ x: 100, y: 40 }], 200, 20)
    expect(d).toBe('M0 100L80 100C90 100 90 40 100 40L200 40')
  })
  it('shrinks the ramp when solves are close so curves do not overlap', () => {
    const d = stepPath({ x: 0, y: 100 }, [{ x: 10, y: 60 }, { x: 14, y: 20 }], 50, 26)
    // First rise starts at the left edge (run 10 < ramp 26), the second spans only the 4px gap.
    expect(d.startsWith('M0 100C5 100 5 60 10 60')).toBe(true)
    expect(d).toContain('C12 60 12 20 14 20')
  })
  it('draws a flat run when the total does not change', () => {
    expect(stepPath({ x: 0, y: 50 }, [{ x: 30, y: 50 }], 30)).toBe('M0 50L4 50L30 50')
  })
})

describe('paddedRange', () => {
  it('gives a single instant room on both sides', () => {
    const [a, b] = paddedRange(1000, 1000)
    expect(a).toBeLessThan(1000)
    expect(b).toBeGreaterThan(1000)
    expect(paddedRange(0, 7200)).toEqual([0, 7200])
  })
})
