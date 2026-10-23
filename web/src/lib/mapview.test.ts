import { describe, expect, it } from 'vitest'
import { centerOn, clampScale, clampView, fitAllScale, minimapScale, releaseVelocity, visibleRect, widthScale, zoomAt } from './mapview'

const content = { w: 2000, h: 3000 }
const viewport = { w: 1000, h: 600 }

describe('clampView', () => {
  it('lets a large map pan to its edges but not past them', () => {
    expect(clampView({ x: 500, y: 500, s: 1 }, content, viewport)).toEqual({ x: 16, y: 16, s: 1 })
    expect(clampView({ x: -9000, y: -9000, s: 1 }, content, viewport)).toEqual({ x: 1000 - 2000 - 16, y: 600 - 3000 - 16, s: 1 })
    expect(clampView({ x: -300, y: -400, s: 1 }, content, viewport)).toEqual({ x: -300, y: -400, s: 1 })
  })

  it('parks a map that fits at the padding instead of drifting', () => {
    expect(clampView({ x: 400, y: -200, s: 0.1 }, content, viewport)).toEqual({ x: 16, y: 16, s: 0.1 })
  })
})

describe('zoomAt', () => {
  it('keeps the point under the cursor fixed', () => {
    const before = { x: -200, y: -100, s: 1 }
    const after = zoomAt(before, 2, 400, 300)
    const contentPoint = (v: typeof before) => ({ x: (400 - v.x) / v.s, y: (300 - v.y) / v.s })
    expect(contentPoint(after).x).toBeCloseTo(contentPoint(before).x)
    expect(contentPoint(after).y).toBeCloseTo(contentPoint(before).y)
    expect(after.s).toBe(2)
  })

  it('stops at the zoom limits', () => {
    expect(zoomAt({ x: 0, y: 0, s: 1.9 }, 5, 0, 0).s).toBe(2)
    expect(zoomAt({ x: 0, y: 0, s: 0.2 }, 0.1, 0, 0).s).toBe(0.15)
    expect(clampScale(10)).toBe(2)
  })
})

describe('scales', () => {
  it('fits the width without enlarging and keeps text readable', () => {
    expect(widthScale(800, 1000)).toBe(1)
    expect(widthScale(2000, 1000)).toBe(0.6)
    expect(widthScale(1300, 1000)).toBe(0.76)
    expect(widthScale(0, 1000)).toBe(1)
  })

  it('fits everything for the overview', () => {
    const s = fitAllScale(content, viewport)
    expect(content.w * s).toBeLessThanOrEqual(viewport.w)
    expect(content.h * s).toBeLessThanOrEqual(viewport.h)
    expect(fitAllScale({ w: 100, h: 100 }, viewport)).toBe(1)
  })

  it('sizes the minimap to its box', () => {
    const m = minimapScale(content, { w: 160, h: 110 })
    expect(content.w * m).toBeLessThanOrEqual(160)
    expect(content.h * m).toBeLessThanOrEqual(110)
  })
})

describe('centerOn and visibleRect', () => {
  it('puts a rectangle in the middle of the viewport', () => {
    const view = centerOn({ x: 1000, y: 1500, w: 200, h: 80 }, 1, viewport)
    expect(visibleRect(view, viewport).x + visibleRect(view, viewport).w / 2).toBeCloseTo(1100)
    expect(visibleRect(view, viewport).y + visibleRect(view, viewport).h / 2).toBeCloseTo(1540)
  })
})

describe('releaseVelocity', () => {
  it('uses only the last 100ms and handles empty input', () => {
    const samples = [
      { t: 0, x: 0, y: 0 },
      { t: 200, x: 100, y: 0 },
      { t: 250, x: 150, y: 25 },
      { t: 300, x: 200, y: 50 },
    ]
    const v = releaseVelocity(samples)
    expect(v.vx).toBeCloseTo(1)
    expect(v.vy).toBeCloseTo(0.5)
    expect(releaseVelocity([])).toEqual({ vx: 0, vy: 0 })
    expect(releaseVelocity([{ t: 5, x: 1, y: 1 }])).toEqual({ vx: 0, vy: 0 })
  })
})
