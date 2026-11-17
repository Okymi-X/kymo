import { useState, type ReactNode } from 'react'
import { bucketLabel, niceMax, tickPositions } from '../../lib/monitorCharts'

const W = 720
const H = 210
const PAD = { l: 42, r: 8, t: 10, b: 24 }

export interface PlotContext {
  /** Horizontal centre of bucket `i`. */
  x: (i: number) => number
  /** Vertical position of a value. */
  y: (v: number) => number
  /** Width of one bucket. */
  cell: number
  bottom: number
}

interface Props {
  label: string
  /** Start time of every bucket. */
  times: number[]
  step: number
  /** Largest value any mark reaches; the axis tops out just above it. */
  top: number
  marks: (c: PlotContext) => ReactNode
  /** What the tooltip says about bucket `i`. */
  tooltip: (i: number) => ReactNode
  /** A click on bucket `i`. */
  onPick?: (i: number) => void
  /** Drawn over the marks, for example alert markers. */
  overlay?: (c: PlotContext) => ReactNode
}

/** The frame every live chart shares: axes, gridlines, a hover tooltip and click to pick a bucket.
 *  The marks themselves (bars, lines) come from the caller. */
export function TimePlot({ label, times, step, top, marks, tooltip, onPick, overlay }: Props) {
  const [hover, setHover] = useState<number | null>(null)
  const n = Math.max(1, times.length)
  const max = niceMax(top)
  const cell = (W - PAD.l - PAD.r) / n
  const bottom = H - PAD.b
  const ctx: PlotContext = {
    x: (i) => PAD.l + (i + 0.5) * cell,
    y: (v) => bottom - (v / max) * (bottom - PAD.t),
    cell,
    bottom,
  }
  const grid = [0, 0.25, 0.5, 0.75, 1]
  return (
    <div className="relative">
      <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label={label} className="h-auto w-full" onMouseLeave={() => setHover(null)}>
        {grid.map((g) => (
          <g key={g}>
            <line x1={PAD.l} x2={W - PAD.r} y1={ctx.y(max * g)} y2={ctx.y(max * g)} className="stroke-line" strokeWidth="1" />
            <text x={PAD.l - 6} y={ctx.y(max * g) + 3} textAnchor="end" className="fill-muted text-[10px]">
              {Math.round(max * g)}
            </text>
          </g>
        ))}
        {tickPositions(times.length).map((i) => (
          <text key={i} x={ctx.x(i)} y={H - 6} textAnchor="middle" className="fill-muted text-[10px]">
            {bucketLabel(times[i], step)}
          </text>
        ))}
        {hover !== null && <rect x={PAD.l + hover * cell} y={PAD.t} width={cell} height={bottom - PAD.t} className="fill-line/50" />}
        {marks(ctx)}
        {times.map((_, i) => (
          <rect
            key={i}
            x={PAD.l + i * cell}
            y={PAD.t}
            width={cell}
            height={bottom - PAD.t}
            fill="transparent"
            className={onPick ? 'cursor-pointer' : undefined}
            onMouseEnter={() => setHover(i)}
            onClick={() => onPick?.(i)}
          />
        ))}
        {overlay?.(ctx)}
      </svg>
      {hover !== null && (
        <div
          role="tooltip"
          className="pointer-events-none absolute top-1 z-10 -translate-x-1/2 whitespace-nowrap rounded-control border border-line bg-surface px-2.5 py-1.5 text-xs shadow-[var(--shadow-lift)]"
          style={{ left: `${(((PAD.l + (hover + 0.5) * cell) / W) * 100).toFixed(2)}%` }}
        >
          {tooltip(hover)}
        </div>
      )}
    </div>
  )
}
