import { useMemo } from 'react'
import { heatGrid } from '../../lib/monitorCharts'

/** Requests per hour for the last week: one row per day, one column per hour, in the viewer's time zone.
 *  Darker means busier. Used to see when the platform is really used. */
export function Heatmap({ from, hours }: { from: number; hours: number[] }) {
  const grid = useMemo(() => heatGrid(from, hours), [from, hours])
  const max = Math.max(1, ...hours)
  return (
    <div className="flex flex-col gap-1 overflow-x-auto" role="img" aria-label={`Requests per hour over the last week, busiest hour ${max} requests`}>
      <div className="grid min-w-[34rem] grid-cols-[4.5rem_repeat(24,minmax(0,1fr))] gap-px text-[10px] text-muted">
        <span />
        {Array.from({ length: 24 }, (_, h) => (
          <span key={h} className="text-center tabular-nums">
            {h % 3 === 0 ? h : ''}
          </span>
        ))}
        {grid.map((row) => (
          <div key={row.day} className="contents">
            <span className="pr-2 text-right text-xs leading-5">{row.day}</span>
            {row.cells.map((cell, h) => (
              <span
                key={h}
                title={cell.count < 0 ? '' : `${row.day} ${h}:00, ${cell.count} requests`}
                className="h-5 rounded-[2px] border border-line/60"
                style={cell.count > 0 ? { background: `color-mix(in oklab, var(--color-accent) ${Math.max(8, Math.round((cell.count / max) * 100))}%, transparent)` } : undefined}
              />
            ))}
          </div>
        ))}
      </div>
    </div>
  )
}
