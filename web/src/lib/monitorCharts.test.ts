import { describe, expect, it } from 'vitest'
import { countTicks, cropSeries, findPeaks, heatGrid, niceMax, scaleWindow, tickPositions, windowOf } from './monitorCharts'

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

describe('zooming', () => {
  const buckets = Array.from({ length: 12 }, (_, i) => ({ t: 1000 + i * 60 }))
  const series = { from: 1000, step: 60, buckets, markers: [{ last_at: 1100 }, { last_at: 1600 }] }
  it('crops buckets and markers to the window and moves `from` to the first bucket left', () => {
    const c = cropSeries(series, { from: 1180, to: 1420 })
    expect(c.buckets.map((b) => b.t)).toEqual([1180, 1240, 1300, 1360])
    expect(c.from).toBe(1180)
    expect(c.markers).toEqual([])
    expect(cropSeries(series, { from: 1000, to: 1200 }).markers).toEqual([{ last_at: 1100 }])
  })
  it('zooms in around the middle, out again, and back to the whole range', () => {
    const full = { from: 0, to: 1000 }
    const a = scaleWindow(full, null, 0.5, 100)
    expect(a).toEqual({ from: 250, to: 750 })
    expect(scaleWindow(full, a, 0.5, 100)).toEqual({ from: 375, to: 625 })
    expect(scaleWindow(full, a, 2, 100)).toBeNull()
  })
  it('never zooms in past the smallest span and keeps the window inside the range', () => {
    const full = { from: 0, to: 1000 }
    expect(scaleWindow(full, { from: 400, to: 500 }, 0.5, 100)).toEqual({ from: 400, to: 500 })
    expect(scaleWindow(full, { from: 900, to: 1000 }, 1.5, 100)).toEqual({ from: 850, to: 1000 })
  })
  it('turns two bucket positions into a window, in either order', () => {
    const times = [10, 20, 30, 40]
    expect(windowOf(times, 10, 1, 2)).toEqual({ from: 20, to: 40 })
    expect(windowOf(times, 10, 3, 0)).toEqual({ from: 10, to: 50 })
    expect(windowOf([], 10, 0, 1)).toBeNull()
  })
})

describe('countTicks', () => {
  it('uses whole numbers, never repeating one', () => {
    expect(countTicks(0)).toEqual([0, 1])
    expect(countTicks(2)).toEqual([0, 1, 2])
    expect(countTicks(4)).toEqual([0, 1, 2, 3, 4])
  })
  it('spaces a bigger range in round steps that cover the top', () => {
    expect(countTicks(5)).toEqual([0, 2, 4, 6])
    expect(countTicks(47)).toEqual([0, 20, 40, 60])
    expect(countTicks(1000)).toEqual([0, 500, 1000])
    for (const top of [3, 7, 13, 99, 480, 50000]) {
      const t = countTicks(top)
      expect(t.every(Number.isInteger)).toBe(true)
      expect(t[t.length - 1]).toBeGreaterThanOrEqual(top)
      expect(t.length).toBeLessThanOrEqual(7)
    }
  })
})
