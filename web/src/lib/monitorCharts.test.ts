import { describe, expect, it } from 'vitest'
import { findPeaks, heatGrid, niceMax, tickPositions } from './monitorCharts'

describe('niceMax', () => {
  it('rounds up to 1, 2, 5 or 10 times a power of ten', () => {
    expect(niceMax(0)).toBe(1)
    expect(niceMax(1)).toBe(1)
    expect(niceMax(3)).toBe(5)
    expect(niceMax(7)).toBe(10)
    expect(niceMax(11)).toBe(20)
    expect(niceMax(180)).toBe(200)
    expect(niceMax(1001)).toBe(2000)
  })
})

describe('findPeaks', () => {
  it('marks a burst and nothing on a flat or quiet stretch', () => {
    const calm = Array.from({ length: 60 }, () => 12)
    expect(findPeaks(calm)).toEqual([])
    const burst = [...calm]
    burst[30] = 400
    expect(findPeaks(burst)).toEqual([30])
    expect(findPeaks(Array.from({ length: 60 }, (_, i) => (i === 10 ? 6 : 1)))).toEqual([])
    expect(findPeaks([1, 2, 3])).toEqual([])
  })
})

describe('tickPositions', () => {
  it('spreads about six labels over the buckets and always starts at the first', () => {
    const ticks = tickPositions(61)
    expect(ticks[0]).toBe(0)
    expect(ticks.length).toBeGreaterThanOrEqual(5)
    expect(ticks.length).toBeLessThanOrEqual(8)
    expect(tickPositions(1)).toEqual([0])
  })
})

describe('heatGrid', () => {
  it('lays a week of hours into days of 24 cells, empty cells marked', () => {
    const from = new Date(2026, 10, 9, 0, 0, 0).getTime() / 1000
    const hours = Array.from({ length: 168 }, (_, i) => i)
    const grid = heatGrid(from, hours)
    expect(grid).toHaveLength(7)
    expect(grid[0].cells).toHaveLength(24)
    expect(grid[0].cells[0].count).toBe(0)
    expect(grid[6].cells[23].count).toBe(167)
    // A range that starts mid day leaves the earlier hours of its first day empty.
    const partial = heatGrid(from + 5 * 3600, hours.slice(0, 30))
    expect(partial[0].cells[0].count).toBe(-1)
    expect(partial[0].cells[5].count).toBe(0)
  })
})
