import { useQuery } from '@tanstack/react-query'
import { useEffect, useMemo, useRef, useState } from 'react'
import { api } from '../api/client'
import type { LadderSeries } from '../api/types'
import { niceTicks, paddedRange, timeLabel, timeTicks } from '../lib/chart'

const MARGIN = { top: 12, right: 14, bottom: 28, left: 40 }
const HEIGHT = 280
const DEFAULT_SHOWN = 5
const colour = (index: number) => `var(--series-${(index % 10) + 1})`
const dateTime = (t: number) => new Date(t * 1000).toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' })

/** A staircase: flat, then straight up at each match that earned points. */
function steps(points: { x: number; y: number }[], baseline: number, startX: number, endX: number): string {
  let path = `M${startX},${baseline}`
  for (const p of points) {
    path += `H${p.x}V${p.y}`
  }
  return `${path}H${endX}`
}

/** Cumulative ladder points over time for the teams you pick, with the same data as a table. */
export function LadderChart() {
  const q = useQuery({ queryKey: ['ladder-timeline'], queryFn: api.ladderTimeline, refetchInterval: 60_000 })
  const series = useMemo(() => q.data ?? [], [q.data])
  const [chosen, setChosen] = useState<Set<string> | null>(null)
  const wrap = useRef<HTMLDivElement>(null)
  const [width, setWidth] = useState(640)
  useEffect(() => {
    const el = wrap.current
    if (!el) return
    const observer = new ResizeObserver(([entry]) => setWidth(Math.max(Math.round(entry.contentRect.width), 260)))
    observer.observe(el)
    return () => observer.disconnect()
  }, [series.length > 0])

  if (q.isPending) return null
  if (series.length === 0) return <p className="text-muted">No dated results yet, so there is nothing to draw. The graph appears with the first played match.</p>

  const keyOf = (s: LadderSeries) => s.name.toLowerCase()
  const selected = chosen ?? new Set(series.slice(0, DEFAULT_SHOWN).map(keyOf))
  const shown = series.filter((s) => selected.has(keyOf(s)))
  const toggle = (s: LadderSeries) => {
    const next = new Set(selected)
    if (next.has(keyOf(s))) next.delete(keyOf(s))
    else next.add(keyOf(s))
    setChosen(next)
  }

  const times = shown.flatMap((s) => s.steps.map((p) => p.at))
  const [from, to] = paddedRange(times.length ? Math.min(...times) : 0, times.length ? Math.max(...times) : 1)
  const top = Math.max(1, ...shown.map((s) => s.points))
  const ticksY = niceTicks(top, 4)
  const maxY = ticksY[ticksY.length - 1]
  const innerW = width - MARGIN.left - MARGIN.right
  const innerH = HEIGHT - MARGIN.top - MARGIN.bottom
  const x = (t: number) => MARGIN.left + ((t - from) / (to - from)) * innerW
  const y = (v: number) => MARGIN.top + innerH - (v / maxY) * innerH
  const span = to - from

  return (
    <section aria-label="Ladder points over time" className="flex flex-col gap-4">
      <div>
        <h2 className="text-lg font-semibold tracking-tight">Points over time</h2>
        <p className="mt-1 max-w-prose text-sm text-muted">
          Each step is a match that earned points, dated when it was last decided; a correction moves its points to the date of the correction. Teams with the same name count as one team across tournaments, and a renamed team starts a new line.
        </p>
      </div>

      <fieldset className="flex max-h-40 flex-wrap gap-2 overflow-y-auto">
        <legend className="mb-1 text-xs text-muted">Teams on the graph ({shown.length} of {series.length})</legend>
        {series.map((s, i) => {
          const on = selected.has(keyOf(s))
          return (
            <label key={keyOf(s)} className={`flex cursor-pointer items-center gap-2 rounded-full border px-3 py-1 text-sm ${on ? 'border-ink' : 'border-line text-muted'}`}>
              <input type="checkbox" checked={on} onChange={() => toggle(s)} className="sr-only" />
              <span aria-hidden="true" className="size-2.5 rounded-full" style={{ background: on ? colour(i) : 'var(--color-line)' }} />
              <span className="max-w-[10rem] truncate">{s.name}</span>
              <span className="font-mono text-xs tabular-nums">{s.points}</span>
            </label>
          )
        })}
      </fieldset>

      <div ref={wrap} className="rounded-card border border-line bg-surface p-2">
        {shown.length === 0 ? (
          <p className="p-6 text-center text-sm text-muted">Pick at least one team to see its line.</p>
        ) : (
          <svg role="img" aria-label={`Ladder points over time for ${shown.map((s) => `${s.name}, ${s.points} points`).join('; ')}`} width={width} height={HEIGHT} className="block max-w-full">
            {ticksY.map((v) => (
              <g key={v}>
                <line x1={MARGIN.left} x2={width - MARGIN.right} y1={y(v)} y2={y(v)} stroke="var(--color-line)" strokeWidth={1} />
                <text x={MARGIN.left - 6} y={y(v) + 4} textAnchor="end" fontSize={11} fill="var(--color-muted)">
                  {v}
                </text>
              </g>
            ))}
            {timeTicks(from, to, width < 480 ? 3 : 6).map((t) => (
              <text key={t} x={x(t)} y={HEIGHT - 8} textAnchor="middle" fontSize={11} fill="var(--color-muted)">
                {timeLabel(t, span)}
              </text>
            ))}
            {shown.map((s) => {
              const index = series.indexOf(s)
              const pts = s.steps.map((p) => ({ x: x(p.at), y: y(p.total) }))
              const first = s.steps[0]?.at ?? from
              return (
                <g key={keyOf(s)}>
                  <path d={steps(pts, y(0), x(first), width - MARGIN.right)} fill="none" stroke={colour(index)} strokeWidth={2} strokeLinejoin="round" />
                  {pts.map((p, i) => (
                    <circle key={i} cx={p.x} cy={p.y} r={3} fill={colour(index)}>
                      <title>{`${s.name}: ${s.steps[i].total} points on ${dateTime(s.steps[i].at)}`}</title>
                    </circle>
                  ))}
                </g>
              )
            })}
          </svg>
        )}
      </div>

      <details className="rounded-card border border-line">
        <summary className="cursor-pointer px-4 py-3 text-sm font-medium">Show the same data as a table</summary>
        <div className="max-h-96 overflow-auto">
          <table className="w-full text-sm">
            <caption className="sr-only">Ladder points after each match that earned points</caption>
            <thead>
              <tr className="border-y border-line text-left text-xs text-muted">
                <th scope="col" className="px-4 py-2 font-medium">Team</th>
                <th scope="col" className="px-4 py-2 font-medium">When</th>
                <th scope="col" className="px-4 py-2 text-right font-medium">Points after</th>
              </tr>
            </thead>
            <tbody>
              {shown.flatMap((s) =>
                s.steps.map((p) => (
                  <tr key={`${keyOf(s)}-${p.at}`} className="border-b border-line last:border-0">
                    <th scope="row" className="px-4 py-2 text-left font-medium">{s.name}</th>
                    <td className="px-4 py-2 text-muted">{dateTime(p.at)}</td>
                    <td className="px-4 py-2 text-right font-mono tabular-nums">{p.total}</td>
                  </tr>
                )),
              )}
              {shown.length === 0 && (
                <tr>
                  <td colSpan={3} className="px-4 py-6 text-center text-muted">Pick a team to list its steps.</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </details>
    </section>
  )
}
