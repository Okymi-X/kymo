// Geometry for the score chart. Pure functions, so the maths is tested without a browser.

export interface Step {
  at: number
  total: number
}

/** A "nice" axis maximum and ticks (1, 2, 2.5 or 5 times a power of ten) covering `max`. */
export function niceTicks(max: number, wanted = 5): number[] {
  const top = Math.max(max, 1)
  const rough = top / Math.max(wanted, 1)
  const power = 10 ** Math.floor(Math.log10(rough))
  const unit = [1, 2, 2.5, 5, 10].map((m) => m * power).find((u) => u >= rough) ?? 10 * power
  const ticks: number[] = []
  for (let v = 0; v < top + unit * 0.999; v += unit) ticks.push(Math.round(v * 1000) / 1000)
  return ticks
}

const MINUTE = 60
const HOUR = 3600
const DAY = 86_400
const TIME_STEPS = [MINUTE, 5 * MINUTE, 10 * MINUTE, 30 * MINUTE, HOUR, 3 * HOUR, 6 * HOUR, 12 * HOUR, DAY, 2 * DAY, 7 * DAY, 30 * DAY]

/** Tick times inside [from, to], aligned to the local clock, about `wanted` of them. */
export function timeTicks(from: number, to: number, wanted = 6): number[] {
  const span = Math.max(to - from, 1)
  const step = TIME_STEPS.find((s) => span / s <= wanted) ?? TIME_STEPS[TIME_STEPS.length - 1]
  const offset = new Date(from * 1000).getTimezoneOffset() * 60
  const first = Math.ceil((from - offset) / step) * step + offset
  const out: number[] = []
  for (let t = first; t <= to; t += step) out.push(t)
  return out
}

/** A short label for a tick: a clock time inside a day or two, a date once the span is longer. */
export function timeLabel(t: number, span: number): string {
  const d = new Date(t * 1000)
  if (span <= 2 * DAY) return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
  return d.toLocaleDateString([], { month: 'short', day: 'numeric' }) + (span <= 7 * DAY ? ` ${d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}` : '')
}

/** The total a series had at time `t`: the last step at or before it, 0 before the first. */
export function valueAt(steps: Step[], t: number): number {
  let value = 0
  for (const s of steps) {
    if (s.at > t) break
    value = s.total
  }
  return value
}

export interface Point {
  x: number
  y: number
}

/**
 * A line that stays flat, then rises in a smooth S-curve into each solve, like a score that
 * climbs rather than jumps. `ramp` is how many pixels the rise takes; it shrinks when two solves
 * are close, so curves never overlap. `points` are in pixels, in time order, and `start` is where
 * the line begins (zero points at the left edge); the line is extended flat to `endX`.
 */
export function stepPath(start: Point, points: Point[], endX: number, ramp = 26): string {
  let d = `M${f(start.x)} ${f(start.y)}`
  let prev = start
  for (const p of points) {
    const run = Math.max(p.x - prev.x, 0)
    const r = Math.min(ramp, run)
    const a = p.x - r
    if (a > prev.x) d += `L${f(a)} ${f(prev.y)}`
    if (p.y === prev.y) d += `L${f(p.x)} ${f(p.y)}`
    else d += `C${f(a + r / 2)} ${f(prev.y)} ${f(p.x - r / 2)} ${f(p.y)} ${f(p.x)} ${f(p.y)}`
    prev = p
  }
  if (endX > prev.x) d += `L${f(endX)} ${f(prev.y)}`
  return d
}

const f = (n: number) => String(Math.round(n * 100) / 100)

/** The widest span a chart can honestly show: a single instant gets an hour of room. */
export function paddedRange(from: number, to: number): [number, number] {
  return to - from < MINUTE ? [from - HOUR / 2, from + HOUR / 2] : [from, to]
}
