import { useEffect, useRef, useState, type PointerEvent, type ReactNode } from 'react'
import { bucketLabel, countTicks, tickPositions } from '../../lib/monitorCharts'

const PAD = { l: 44, r: 10, t: 12, b: 28 }
/** Buckets a drag must cross to count as a zoom rather than a click. */
const MIN_BRUSH = 1

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
  /** A drag across buckets `from` to `to`: zoom in on them. */
  onZoom?: (from: number, to: number) => void
  /** Drawn over the marks, for example alert markers. */
  overlay?: (c: PlotContext) => ReactNode
}

/** The frame every live chart shares, drawn at its real pixel size so text stays readable: axes with
 *  whole-number ticks, gridlines, a hover tooltip, a click to pick a bucket and a drag to zoom. The
 *  marks themselves (bars, lines) come from the caller. */
export function TimePlot({ label, times, step, top, marks, tooltip, onPick, onZoom, overlay }: Props) {
  const wrap = useRef<HTMLDivElement>(null)
  const [width, setWidth] = useState(640)
  const [hover, setHover] = useState<number | null>(null)
  const [brush, setBrush] = useState<{ a: number; b: number } | null>(null)
  useEffect(() => {
    const el = wrap.current
    if (!el) return
    const observer = new ResizeObserver(([entry]) => setWidth(Math.max(Math.round(entry.contentRect.width), 280)))
    observer.observe(el)
    return () => observer.disconnect()
  }, [])
  // A bucket that no longer exists (the range was zoomed while the pointer stood on it) is no hover.
  const active = hover !== null && hover < times.length ? hover : null
  const height = Math.round(Math.min(400, Math.max(250, width * 0.42)))
  const n = Math.max(1, times.length)
  const ticks = countTicks(top)
  const max = ticks[ticks.length - 1]
  const cell = (width - PAD.l - PAD.r) / n
  const bottom = height - PAD.b
  const ctx: PlotContext = {
    x: (i) => PAD.l + (i + 0.5) * cell,
    y: (v) => bottom - (v / max) * (bottom - PAD.t),
    cell,
    bottom,
  }
  const indexAt = (e: PointerEvent<SVGRectElement>) => {
    const box = e.currentTarget.getBoundingClientRect()
    return Math.min(n - 1, Math.max(0, Math.floor((e.clientX - box.left) / cell)))
  }
  const finish = (e: PointerEvent<SVGRectElement>) => {
    if (!brush) return
    const i = indexAt(e)
    const [lo, hi] = [Math.min(brush.a, i), Math.max(brush.a, i)]
    setBrush(null)
    if (onZoom && hi - lo >= MIN_BRUSH) onZoom(lo, hi)
    else onPick?.(i)
  }
  const shown = brush ? { lo: Math.min(brush.a, brush.b), hi: Math.max(brush.a, brush.b) } : null
  return (
    <div ref={wrap} className="relative">
      <svg width={width} height={height} role="img" aria-label={label} className="block max-w-full touch-pan-y select-none" onMouseLeave={() => setHover(null)}>
        {ticks.map((v) => (
          <g key={v}>
            <line x1={PAD.l} x2={width - PAD.r} y1={ctx.y(v)} y2={ctx.y(v)} className="stroke-line" strokeWidth="1" />
            <text x={PAD.l - 8} y={ctx.y(v) + 4} textAnchor="end" className="fill-muted font-mono text-[11px]">
              {v}
            </text>
          </g>
        ))}
        {tickPositions(times.length, width < 480 ? 3 : 6).map((i) => (
          <text key={i} x={ctx.x(i)} y={height - 8} textAnchor="middle" className="fill-muted font-mono text-[11px]">
            {bucketLabel(times[i], step)}
          </text>
        ))}
        {active !== null && !brush && <rect x={PAD.l + active * cell} y={PAD.t} width={cell} height={bottom - PAD.t} className="fill-line/50" />}
        {shown && <rect x={PAD.l + shown.lo * cell} y={PAD.t} width={(shown.hi - shown.lo + 1) * cell} height={bottom - PAD.t} className="fill-accent/15 stroke-accent" strokeWidth="1" />}
        {marks(ctx)}
        <rect
          x={PAD.l}
          y={PAD.t}
          width={Math.max(0, width - PAD.l - PAD.r)}
          height={bottom - PAD.t}
          fill="transparent"
          className={onZoom ? 'cursor-crosshair' : onPick ? 'cursor-pointer' : undefined}
          onPointerMove={(e) => {
            const i = indexAt(e)
            setHover(i)
            if (brush) setBrush({ a: brush.a, b: i })
          }}
          onPointerDown={(e) => {
            if (e.button !== 0) return
            e.currentTarget.setPointerCapture(e.pointerId)
            const i = indexAt(e)
            setBrush({ a: i, b: i })
          }}
          onPointerUp={finish}
          onPointerCancel={() => setBrush(null)}
        />
        {overlay?.(ctx)}
      </svg>
      {active !== null && !brush && (
        <div
          role="tooltip"
          className="pointer-events-none absolute top-1 z-10 -translate-x-1/2 whitespace-nowrap rounded-control border border-line bg-surface px-3 py-2 text-xs shadow-[var(--shadow-lift)]"
          style={{ left: Math.min(width - 80, Math.max(80, PAD.l + (active + 0.5) * cell)) }}
        >
          {tooltip(active)}
        </div>
      )}
    </div>
  )
}
