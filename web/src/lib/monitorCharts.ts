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
