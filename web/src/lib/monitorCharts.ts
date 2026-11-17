/** A round number at or above `value` to use as the top of a chart axis: 1, 2, 5 times a power of ten. */
export function niceMax(value: number): number {
  if (value <= 1) return 1
  const power = 10 ** Math.floor(Math.log10(value))
  for (const step of [1, 2, 5, 10]) {
    if (value <= step * power) return step * power
  }
  return 10 * power
}

/** Positions of the buckets that stand far above the rest: more than three standard deviations over the mean,
 *  at least twice the mean, and big enough to matter (`floor`). Quiet stretches never produce a peak. */
export function findPeaks(values: number[], floor = 10): number[] {
  if (values.length < 5) return []
  const mean = values.reduce((a, b) => a + b, 0) / values.length
  const sd = Math.sqrt(values.reduce((a, b) => a + (b - mean) ** 2, 0) / values.length)
  const limit = Math.max(mean + 3 * sd, 2 * mean, floor)
  const peaks: number[] = []
  values.forEach((v, i) => {
    if (v >= limit) peaks.push(i)
  })
  return peaks
}

/** The time label for a bucket: clock time for a short range, day and hour for a long one. */
export function bucketLabel(t: number, step: number): string {
  const d = new Date(t * 1000)
  const clock = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', ...(step < 60 ? { second: '2-digit' } : {}) })
  return step >= 3600 ? `${d.toLocaleDateString([], { weekday: 'short' })} ${clock}` : clock
}

/** Which of `count` evenly spaced axis labels to draw, as bucket positions. */
export function tickPositions(buckets: number, wanted = 6): number[] {
  if (buckets <= 1) return [0]
  const gap = Math.max(1, Math.round((buckets - 1) / (wanted - 1)))
  const out: number[] = []
  for (let i = 0; i < buckets; i += gap) out.push(i)
  return out
}

/** The 7 by 24 grid of a week of hourly counts, in the viewer's time zone: rows are days oldest first, columns are hours. */
export function heatGrid(from: number, hours: number[]): { day: string; cells: { count: number; at: number }[] }[] {
  const rows: { day: string; cells: { count: number; at: number }[] }[] = []
  hours.forEach((count, i) => {
    const at = from + i * 3600
    const d = new Date(at * 1000)
    const key = d.toLocaleDateString([], { weekday: 'short', day: 'numeric' })
    let row = rows.find((r) => r.day === key)
    if (!row) {
      row = { day: key, cells: Array.from({ length: 24 }, (_, h) => ({ count: -1, at: h })) }
      rows.push(row)
    }
    row.cells[d.getHours()] = { count, at }
  })
  return rows
}

export interface Window {
  from: number
  to: number
}

/** The part of a series inside a window, with its alert markers. `from` follows the first bucket
 *  that is left, because alert markers are placed by their distance from it. */
export function cropSeries<S extends { from: number; step: number; buckets: { t: number }[]; markers: { last_at: number }[] }>(series: S, w: Window): S {
  const buckets = series.buckets.filter((b) => b.t >= w.from && b.t < w.to)
  return {
    ...series,
    buckets,
    from: buckets[0]?.t ?? w.from,
    markers: series.markers.filter((m) => m.last_at >= w.from && m.last_at < w.to),
  }
}

/** The window one step in (`factor` below 1) or out (above 1) around its middle, never wider than the
 *  whole range and never narrower than `minSpan` (a handful of buckets). Null means the whole range. */
export function scaleWindow(full: Window, current: Window | null, factor: number, minSpan: number): Window | null {
  const now = current ?? full
  const fullSpan = full.to - full.from
  const span = Math.min(fullSpan, Math.max(minSpan, (now.to - now.from) * factor))
  if (span >= fullSpan) return null
  const mid = (now.from + now.to) / 2
  const from = Math.min(full.to - span, Math.max(full.from, mid - span / 2))
  return { from, to: from + span }
}

/** The window between two bucket positions of the series on screen, both ends included. */
export function windowOf(times: number[], step: number, a: number, b: number): Window | null {
  if (times.length === 0) return null
  const lo = Math.max(0, Math.min(a, b))
  const hi = Math.min(times.length - 1, Math.max(a, b))
  return { from: times[lo], to: times[hi] + step }
}

/** Axis ticks for counts: whole numbers only, at most about five, the last at or above `top`. */
export function countTicks(top: number): number[] {
  const max = Math.max(1, Math.ceil(top))
  if (max <= 4) return Array.from({ length: max + 1 }, (_, i) => i)
  const rough = max / 4
  const power = 10 ** Math.floor(Math.log10(rough))
  const unit = Math.max(1, [1, 2, 5, 10].map((m) => m * power).find((u) => u >= rough) ?? 10 * power)
  const ticks: number[] = []
  for (let v = 0; v < max + unit; v += unit) {
    ticks.push(v)
    if (v >= max) break
  }
  return ticks
}
