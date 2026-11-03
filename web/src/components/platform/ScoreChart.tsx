import { useQuery } from '@tanstack/react-query'
import { useEffect, useMemo, useRef, useState, type KeyboardEvent, type PointerEvent } from 'react'
import { platform } from '../../api/platform'
import { useEventStream } from '../../api/platformHooks'
import type { ScoreSeries } from '../../api/platformTypes'
import { niceTicks, paddedRange, stepPath, timeLabel, timeTicks, valueAt } from '../../lib/chart'
import { Segmented } from '../ui'
import { Panel } from './bits'

const MARGIN = { top: 14, right: 18, bottom: 30, left: 44 }
const TOPS = ['5', '10', '20'] as const
type Top = (typeof TOPS)[number]

/** Stable colours: by participant, so a line keeps its colour when the ranking shifts. */
function colours(series: ScoreSeries[]): Map<number, string> {
  const ids = series.map((s) => s.participant_id).sort((a, b) => a - b)
  return new Map(ids.map((id, i) => [id, `var(--series-${(i % 10) + 1})`]))
}

const fullTime = (t: number) => new Date(t * 1000).toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })

/** Points over time for the best players or teams: flat between solves, rising smoothly into each one. */
export function ScoreChart({ slug }: { slug: string }) {
  useEventStream(slug, true)
  const [top, setTop] = useState<Top>('10')
  const q = useQuery({ queryKey: ['timeline', slug, top], queryFn: () => platform.scoreTimeline(slug, Number(top)), refetchInterval: 30_000, placeholderData: (prev) => prev })
  const wrap = useRef<HTMLDivElement>(null)
  const [width, setWidth] = useState(640)
  const [hidden, setHidden] = useState<Set<number>>(new Set())
  const [focused, setFocused] = useState<number | null>(null)
  const [hoverT, setHoverT] = useState<number | null>(null)
  const [drawing, setDrawing] = useState(true)

  useEffect(() => {
    const el = wrap.current
    if (!el) return
    const observer = new ResizeObserver(([entry]) => setWidth(Math.max(Math.round(entry.contentRect.width), 240)))
    observer.observe(el)
    return () => observer.disconnect()
  }, [q.data === undefined])

  // The lines draw in once. Later refreshes only move them.
  useEffect(() => {
    const id = window.setTimeout(() => setDrawing(false), 1600)
    return () => window.clearTimeout(id)
  }, [])

  const data = q.data
  const series = useMemo(() => data?.series ?? [], [data])
  const colour = useMemo(() => colours(series), [series])
  if (q.isPending || !data || series.length === 0) return null

  const height = width < 480 ? 240 : 320
  const plotW = width - MARGIN.left - MARGIN.right
  const plotH = height - MARGIN.top - MARGIN.bottom
  const [t0, t1] = paddedRange(data.from, data.to)
  const span = t1 - t0
  const yTicks = niceTicks(Math.max(...series.map((s) => s.points)), 4)
  const yMax = yTicks[yTicks.length - 1]
  const x = (t: number) => MARGIN.left + ((t - t0) / span) * plotW
  const y = (v: number) => MARGIN.top + plotH - (v / yMax) * plotH
  const visible = series.filter((s) => !hidden.has(s.participant_id))
  const xTicks = timeTicks(t0, t1, width < 480 ? 4 : 8)

  const timeFromPointer = (e: PointerEvent<SVGSVGElement>) => {
    const box = e.currentTarget.getBoundingClientRect()
    const px = ((e.clientX - box.left) / box.width) * width
    return Math.min(Math.max(t0 + ((px - MARGIN.left) / plotW) * span, t0), t1)
  }
  const onKey = (e: KeyboardEvent<SVGSVGElement>) => {
    const stepT = span / 40
    if (e.key === 'ArrowLeft') setHoverT((h) => Math.max((h ?? t1) - stepT, t0))
    else if (e.key === 'ArrowRight') setHoverT((h) => Math.min((h ?? t0) + stepT, t1))
    else if (e.key === 'Home') setHoverT(t0)
    else if (e.key === 'End') setHoverT(t1)
    else if (e.key === 'Escape') setHoverT(null)
    else return
    e.preventDefault()
  }
  const toggle = (id: number) =>
    setHidden((h) => {
      const next = new Set(h)
      if (!next.delete(id)) next.add(id)
      return next
    })

  const rows = hoverT === null ? [] : visible.map((s) => ({ s, v: valueAt(s.steps, hoverT) })).sort((a, b) => b.v - a.v)
  const hoverX = hoverT === null ? 0 : x(hoverT)
  const flip = hoverX > width * 0.6
  const leader = series[0]

  return (
    <Panel className="flex flex-col gap-3 !p-4 sm:!p-5">
      <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
        <h3 className="text-base font-semibold tracking-tight">Score over time</h3>
        <Segmented label="Lines shown" value={top} options={TOPS.map((t) => ({ value: t, label: `Top ${t}` }))} onChange={setTop} />
      </div>

      <div ref={wrap} className="relative">
        <svg
          width={width}
          height={height}
          viewBox={`0 0 ${width} ${height}`}
          role="img"
          aria-label={`Score over time for the top ${series.length}. ${leader.name} leads with ${leader.points} points. Arrow keys move along the time axis.`}
          tabIndex={0}
          onPointerMove={(e) => setHoverT(timeFromPointer(e))}
          onPointerDown={(e) => setHoverT(timeFromPointer(e))}
          onPointerLeave={(e) => e.pointerType === 'mouse' && setHoverT(null)}
          onKeyDown={onKey}
          onBlur={() => setHoverT(null)}
          className="block max-w-full touch-pan-y select-none rounded-control outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
        >
          <g aria-hidden="true">
            {yTicks.map((v) => (
              <g key={v}>
                <line x1={MARGIN.left} x2={width - MARGIN.right} y1={y(v)} y2={y(v)} stroke="var(--color-line)" strokeWidth={1} />
                <text x={MARGIN.left - 8} y={y(v)} dy="0.32em" textAnchor="end" className="fill-muted font-mono text-[11px] tabular-nums">
                  {v}
                </text>
              </g>
            ))}
            {xTicks.map((t) => (
              <text key={t} x={x(t)} y={height - 8} textAnchor="middle" className="fill-muted font-mono text-[11px]">
                {timeLabel(t, span)}
              </text>
            ))}

            {visible.map((s) => {
              const dim = focused !== null && focused !== s.participant_id
              const pts = s.steps.map((st) => ({ x: x(st.at), y: y(st.total) }))
              return (
                <g key={s.participant_id} className="chart-series" opacity={dim ? 0.18 : 1}>
                  <path
                    d={stepPath({ x: x(t0), y: y(0) }, pts, x(t1))}
                    pathLength={1}
                    fill="none"
                    stroke={colour.get(s.participant_id)}
                    strokeWidth={focused === s.participant_id ? 3 : 2.25}
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    className={`chart-line ${drawing ? 'is-drawing' : ''}`}
                  />
                  {pts.map((p, i) => (
                    <circle key={i} cx={p.x} cy={p.y} r={2.75} fill={colour.get(s.participant_id)} className={drawing ? 'chart-dot' : ''} />
                  ))}
                </g>
              )
            })}

            {hoverT !== null && (
              <g>
                <line x1={hoverX} x2={hoverX} y1={MARGIN.top} y2={MARGIN.top + plotH} stroke="var(--color-muted)" strokeWidth={1} strokeDasharray="3 3" />
                {rows.map(({ s, v }) => (
                  <circle key={s.participant_id} cx={hoverX} cy={y(v)} r={4.5} fill={colour.get(s.participant_id)} stroke="var(--color-surface)" strokeWidth={2} />
                ))}
              </g>
            )}
          </g>
        </svg>

        {hoverT !== null && (
          <div
            aria-hidden="true"
            className="pointer-events-none absolute z-10 w-max max-w-[15rem] rounded-control border border-line bg-surface px-3 py-2 text-xs shadow-[var(--shadow-lift)]"
            style={{ top: MARGIN.top, left: flip ? hoverX - 12 : hoverX + 12, transform: flip ? 'translateX(-100%)' : undefined }}
          >
            <p className="mb-1.5 font-medium text-muted">{fullTime(hoverT)}</p>
            <ul className="flex flex-col gap-1">
              {rows.slice(0, 10).map(({ s, v }) => (
                <li key={s.participant_id} className="flex items-center gap-2">
                  <span className="size-2 shrink-0 rounded-full" style={{ background: colour.get(s.participant_id) }} />
                  <span className="min-w-0 flex-1 truncate">{s.name}</span>
                  <span className="font-mono font-semibold tabular-nums">{v}</span>
                </li>
              ))}
              {rows.length > 10 && <li className="text-muted">and {rows.length - 10} more</li>}
            </ul>
          </div>
        )}
      </div>

      <ul className="flex flex-wrap gap-x-1 gap-y-1" aria-label="Lines">
        {series.map((s) => {
          const off = hidden.has(s.participant_id)
          return (
            <li key={s.participant_id}>
              <button
                type="button"
                aria-pressed={!off}
                onClick={() => toggle(s.participant_id)}
                onMouseEnter={() => setFocused(s.participant_id)}
                onMouseLeave={() => setFocused(null)}
                onFocus={() => setFocused(s.participant_id)}
                onBlur={() => setFocused(null)}
                title={off ? 'Show this line' : 'Hide this line'}
                className={`flex max-w-[12rem] items-center gap-1.5 rounded-full border border-transparent px-2.5 py-1 text-xs transition-[background-color,opacity] duration-150 hover:bg-line/50 ${off ? 'opacity-45' : ''}`}
              >
                <span className="size-2.5 shrink-0 rounded-full border-2" style={{ borderColor: colour.get(s.participant_id), background: off ? 'transparent' : colour.get(s.participant_id) }} />
                <span className={`min-w-0 truncate ${off ? 'line-through' : ''}`}>{s.name}</span>
                <span className="font-mono tabular-nums text-muted">{s.points}</span>
              </button>
            </li>
          )
        })}
      </ul>
    </Panel>
  )
}
