import { useQuery } from '@tanstack/react-query'
import { Link2, Radio } from 'lucide-react'
import { useMemo, useState, type ReactNode } from 'react'
import { platform } from '../../api/platform'
import type { AlertMarker, MonitorBucket, MonitorSeries } from '../../api/platformTypes'
import { bucketLabel, findPeaks } from '../../lib/monitorCharts'
import { Heatmap } from '../charts/Heatmap'
import { Spark } from '../charts/Spark'
import { TimePlot, type PlotContext } from '../charts/TimePlot'
import { Empty, Loading, Panel, Stat } from '../platform/bits'
import { ErrorText, Select } from '../ui'

const RANGES = [
  { minutes: 15, label: 'Last 15 minutes' },
  { minutes: 60, label: 'Last hour' },
  { minutes: 1440, label: 'Last day' },
  { minutes: 10080, label: 'Last week' },
]

interface Line {
  key: string
  label: string
  color: string
  value: (b: MonitorBucket) => number
}

const STACK: Line[] = [
  { key: 'ok', label: 'Succeeded', color: 'var(--series-1)', value: (b) => b.total - b.client - b.server - b.throttled },
  { key: 'client', label: 'Refused or not found', color: 'var(--series-3)', value: (b) => b.client },
  { key: 'throttled', label: 'Rate limited', color: 'var(--series-7)', value: (b) => b.throttled },
  { key: 'server', label: 'Server errors', color: 'var(--color-danger)', value: (b) => b.server },
]

const ACTIVITY: Line[] = [
  { key: 'login_ok', label: 'Sign ins', color: 'var(--series-2)', value: (b) => b.login_ok },
  { key: 'login_failed', label: 'Failed sign ins', color: 'var(--color-danger)', value: (b) => b.login_failed },
  { key: 'signups', label: 'New accounts', color: 'var(--series-5)', value: (b) => b.signups },
  { key: 'submissions', label: 'Flag submissions', color: 'var(--series-1)', value: (b) => b.submissions },
  { key: 'instance_starts', label: 'Instances started', color: 'var(--series-6)', value: (b) => b.instance_starts },
]

const PEOPLE: Line[] = [
  { key: 'users', label: 'Players', color: 'var(--series-1)', value: (b) => b.users },
  { key: 'ips', label: 'Addresses', color: 'var(--series-4)', value: (b) => b.ips },
]

const SPEED: Line[] = [
  { key: 'avg', label: 'Average (ms)', color: 'var(--series-2)', value: (b) => b.avg_ms },
  { key: 'max', label: 'Slowest (ms)', color: 'var(--series-3)', value: (b) => b.max_ms },
]

function Legend({ lines, buckets }: { lines: Line[]; buckets: MonitorBucket[] }) {
  return (
    <ul className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted">
      {lines.map((l) => (
        <li key={l.key} className="flex items-center gap-1.5">
          <span className="size-2.5 rounded-[2px]" style={{ background: l.color }} aria-hidden="true" />
          {l.label}
          <span className="font-mono tabular-nums text-ink">{buckets.reduce((a, b) => a + l.value(b), 0)}</span>
        </li>
      ))}
    </ul>
  )
}

function Tip({ t, step, rows }: { t: number; step: number; rows: [string, ReactNode][] }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="font-medium">{bucketLabel(t, step)}</span>
      {rows.map(([name, value]) => (
        <span key={name} className="flex justify-between gap-4">
          <span className="text-muted">{name}</span>
          <span className="font-mono tabular-nums">{value}</span>
        </span>
      ))}
    </div>
  )
}

/** Alert markers along the bottom of a chart: where the detectors fired. Red while open. */
function alertMarks(series: MonitorSeries, onAlert: (id: number) => void) {
  const place = (m: AlertMarker) => Math.min(series.buckets.length - 1, Math.max(0, Math.floor((m.last_at - series.from) / series.step)))
  return (c: PlotContext) =>
    series.markers.map((m) => {
      const x = c.x(place(m))
      const y = c.bottom - 8
      return (
        <g
          key={m.id}
          role="button"
          tabIndex={0}
          aria-label={`Open alert ${m.id}`}
          className="cursor-pointer outline-none"
          onClick={() => onAlert(m.id)}
          onKeyDown={(e) => (e.key === 'Enter' || e.key === ' ') && onAlert(m.id)}
        >
          <title>{`Alert ${m.id}: ${m.kind.replace('_', ' ')} (${m.status})`}</title>
          <polygon points={`${x},${y - 7} ${x + 6},${y + 4} ${x - 6},${y + 4}`} className={m.status === 'open' ? 'fill-danger' : 'fill-muted'} stroke="var(--color-surface, #fff)" strokeWidth="1" />
        </g>
      )
    })
}

function Card({ title, hint, children }: { title: string; hint?: string; children: ReactNode }) {
  return (
    <Panel className="flex min-w-0 flex-col gap-3">
      <div>
        <h3 className="text-sm font-semibold">{title}</h3>
        {hint && <p className="text-xs text-muted">{hint}</p>}
      </div>
      {children}
    </Panel>
  )
}

function StackedChart({ series, onInspect, onAlert }: { series: MonitorSeries; onInspect: (since: number, until: number) => void; onAlert: (id: number) => void }) {
  const { buckets, step } = series
  const totals = buckets.map((b) => b.total)
  const peaks = useMemo(() => findPeaks(totals), [totals])
  const top = Math.max(1, ...totals)
  return (
    <div className="flex flex-col gap-2">
      <TimePlot
        label={`Requests over time, ${buckets.reduce((a, b) => a + b.total, 0)} in total, busiest bucket ${top}`}
        times={buckets.map((b) => b.t)}
        step={step}
        top={top}
        onPick={(i) => onInspect(buckets[i].t, buckets[i].t + step)}
        tooltip={(i) => (
          <Tip t={buckets[i].t} step={step} rows={[['Requests', buckets[i].total], ...STACK.map((l): [string, ReactNode] => [l.label, l.value(buckets[i])])]} />
        )}
        marks={(c) =>
          buckets.map((b, i) => {
            let base = 0
            const w = Math.max(1, c.cell * 0.78)
            return (
              <g key={b.t}>
                {STACK.map((l) => {
                  const v = l.value(b)
                  if (v <= 0) return null
                  const y0 = c.y(base)
                  base += v
                  return <rect key={l.key} x={c.x(i) - w / 2} y={c.y(base)} width={w} height={Math.max(0.5, y0 - c.y(base))} fill={l.color} />
                })}
                {peaks.includes(i) && <circle cx={c.x(i)} cy={c.y(b.total) - 6} r="3.5" className="fill-danger" />}
              </g>
            )
          })
        }
        overlay={alertMarks(series, onAlert)}
      />
      <Legend lines={STACK} buckets={buckets} />
      {peaks.length > 0 && (
        <p className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
          <span className="font-medium text-danger">Peaks</span>
          {peaks.map((i) => (
            <button key={buckets[i].t} type="button" className="rounded-control border border-line px-2 py-0.5 font-mono hover:bg-line/50" onClick={() => onInspect(buckets[i].t, buckets[i].t + step)} title="Show the requests of this moment">
              {bucketLabel(buckets[i].t, step)} · {buckets[i].total}
            </button>
          ))}
        </p>
      )}
    </div>
  )
}

function LineChart({ series, lines, label, onInspect }: { series: MonitorSeries; lines: Line[]; label: string; onInspect: (since: number, until: number) => void }) {
  const { buckets, step } = series
  const top = Math.max(1, ...buckets.flatMap((b) => lines.map((l) => l.value(b))))
  return (
    <div className="flex flex-col gap-2">
      <TimePlot
        label={label}
        times={buckets.map((b) => b.t)}
        step={step}
        top={top}
        onPick={(i) => onInspect(buckets[i].t, buckets[i].t + step)}
        tooltip={(i) => <Tip t={buckets[i].t} step={step} rows={lines.map((l): [string, ReactNode] => [l.label, l.value(buckets[i])])} />}
        marks={(c) =>
          lines.map((l) => (
            <polyline key={l.key} fill="none" stroke={l.color} strokeWidth="1.75" strokeLinejoin="round" strokeLinecap="round" points={buckets.map((b, i) => `${c.x(i)},${c.y(l.value(b))}`).join(' ')} />
          ))
        }
      />
      <Legend lines={lines} buckets={buckets} />
    </div>
  )
}

function HotList({ title, rows, onPick }: { title: string; rows: { key: string; label: string; count: number; trend: number[] }[]; onPick?: (key: string) => void }) {
  return (
    <Panel className="min-w-0">
      <h3 className="mb-2 text-sm font-semibold">{title}</h3>
      {rows.length === 0 ? (
        <p className="text-sm text-muted">Quiet.</p>
      ) : (
        <ul className="flex flex-col gap-1.5 text-sm">
          {rows.map((r) => (
            <li key={r.key} className="flex min-w-0 items-center justify-between gap-3">
              <span className="min-w-0 truncate font-mono text-xs" title={r.label}>
                {r.label}
              </span>
              <span className="flex items-center gap-2">
                <Spark values={r.trend} label={`${r.label}, requests per minute`} />
                <span className="w-10 text-right font-mono tabular-nums">{r.count}</span>
                {onPick && (
                  <button type="button" aria-label={`Show requests of ${r.label}`} onClick={() => onPick(r.key)} className="text-muted hover:text-ink">
                    <Link2 size={14} aria-hidden="true" />
                  </button>
                )}
              </span>
            </li>
          ))}
        </ul>
      )}
    </Panel>
  )
}

/** The default monitor view: what is happening right now. Everything refreshes every few seconds. */
export function LiveTab({ onInspect, onAlert, onSubject }: { onInspect: (since: number, until: number) => void; onAlert: (id: number) => void; onSubject: (kind: 'ip' | 'user', key: string) => void }) {
  const [minutes, setMinutes] = useState(60)
  const series = useQuery({ queryKey: ['monitor-series', minutes], queryFn: () => platform.monitorSeries(minutes), refetchInterval: 5000 })
  const hot = useQuery({ queryKey: ['monitor-hot'], queryFn: () => platform.monitorHot(5), refetchInterval: 5000 })
  const heat = useQuery({ queryKey: ['monitor-heat'], queryFn: platform.monitorHeat, refetchInterval: 60_000 })
  const s = series.data
  const updated = series.dataUpdatedAt ? new Date(series.dataUpdatedAt).toLocaleTimeString() : ''
  const totals = useMemo(() => {
    const b = s?.buckets ?? []
    const sum = (f: (x: MonitorBucket) => number) => b.reduce((a, x) => a + f(x), 0)
    return { requests: sum((x) => x.total), errors: sum((x) => x.server), throttled: sum((x) => x.throttled), failed: sum((x) => x.login_failed) }
  }, [s])
  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-center gap-3">
        <Select value={minutes} onChange={(e) => setMinutes(Number(e.target.value))} aria-label="Time range">
          {RANGES.map((r) => (
            <option key={r.minutes} value={r.minutes}>
              {r.label}
            </option>
          ))}
        </Select>
        <span className="flex items-center gap-2 text-sm text-muted" aria-live="off">
          <Radio size={15} aria-hidden="true" className="text-win" /> Live{updated && <span className="font-mono text-xs">updated {updated}</span>}
        </span>
        <span className="text-xs text-muted">Click a bar or point to see the requests of that moment. Triangles mark alerts.</span>
      </div>
      <ErrorText error={series.error} />
      {series.isPending ? (
        <Loading />
      ) : s ? (
        <>
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
            <Stat label="Requests" value={totals.requests} />
            <Stat label="Failed sign ins" value={totals.failed} tone={totals.failed > 20 ? 'text-danger' : ''} />
            <Stat label="Rate limited" value={totals.throttled} />
            <Stat label="Server errors" value={totals.errors} tone={totals.errors > 0 ? 'text-danger' : ''} />
          </div>
          {totals.requests === 0 ? (
            <Empty>No requests in this range yet.</Empty>
          ) : (
            <div className="grid gap-4 xl:grid-cols-2">
              <Card title="Requests" hint="Stacked by what happened to them. A red dot marks an unusual peak.">
                <StackedChart series={s} onInspect={onInspect} onAlert={onAlert} />
              </Card>
              <Card title="What people do" hint="Sign ins, new accounts, flag submissions and instances started.">
                <LineChart series={s} lines={ACTIVITY} label="Activity over time" onInspect={onInspect} />
              </Card>
              <Card title="Who is there" hint="Distinct signed in players and distinct addresses.">
                <LineChart series={s} lines={PEOPLE} label="Players and addresses over time" onInspect={onInspect} />
              </Card>
              <Card title="Response time" hint="Average and slowest answer, in milliseconds.">
                <LineChart series={s} lines={SPEED} label="Response time over time" onInspect={onInspect} />
              </Card>
            </div>
          )}
        </>
      ) : null}
      <div className="grid gap-4 lg:grid-cols-2">
        <HotList title="Busiest players, last 5 minutes" rows={hot.data?.players ?? []} onPick={(key) => onSubject('user', key)} />
        <HotList title="Busiest addresses, last 5 minutes" rows={hot.data?.addresses ?? []} onPick={(key) => onSubject('ip', key)} />
      </div>
      <Card title="The week" hint="Requests per hour, to see when the platform is really used.">
        {heat.data ? <Heatmap from={heat.data.from} hours={heat.data.hours} /> : <Loading />}
      </Card>
    </div>
  )
}
