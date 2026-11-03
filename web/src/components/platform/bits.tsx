import { Check, Copy } from 'lucide-react'
import { useEffect, useState, type ReactNode } from 'react'
import { Link } from 'react-router'
import { useIsAdmin, useSignedIn, useWho, useCanMonitor } from '../../api/client'
import type { EventState } from '../../api/platformTypes'
import { stateLabel } from '../../lib/platform'
import { Button } from '../ui'

const stateTone: Record<EventState, string> = {
  draft: 'text-muted',
  registration_open: 'text-accent',
  live: 'text-win',
  paused: 'text-danger',
  ended: 'text-muted',
  archived: 'text-muted',
}

export function StatePill({ state }: { state: EventState }) {
  return <span className={`rounded-full border border-line px-2.5 py-0.5 text-xs font-medium ${stateTone[state]}`}>{stateLabel[state]}</span>
}

export function Pill({ children, tone = 'text-muted' }: { children: ReactNode; tone?: string }) {
  return <span className={`rounded-full border border-line px-2.5 py-0.5 text-xs font-medium ${tone}`}>{children}</span>
}

export function Panel({ children, className = '' }: { children: ReactNode; className?: string }) {
  return <section className={`rounded-card border border-line bg-surface p-5 ${className}`}>{children}</section>
}

export function Empty({ children }: { children: ReactNode }) {
  return <p className="rounded-card border border-dashed border-line p-8 text-center text-muted">{children}</p>
}

export function Loading({ label = 'Loading' }: { label?: string }) {
  return (
    <p role="status" className="py-10 text-center text-muted">
      {label}...
    </p>
  )
}

/** Ticks once a second; used for countdowns. */
export function useNow(intervalMs = 1000): number {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), intervalMs)
    return () => window.clearInterval(id)
  }, [intervalMs])
  return now
}

export function CopyButton({ text, label = 'Copy' }: { text: string; label?: string }) {
  const [done, setDone] = useState(false)
  const copy = () => {
    void navigator.clipboard?.writeText(text).then(
      () => {
        setDone(true)
        window.setTimeout(() => setDone(false), 1500)
      },
      () => undefined,
    )
  }
  return (
    <Button onClick={copy} aria-label={`${label} ${text}`}>
      {done ? <Check size={15} aria-hidden="true" /> : <Copy size={15} aria-hidden="true" />}
      {done ? 'Copied' : label}
    </Button>
  )
}

/** Keeps a page for people who are signed in (and optionally with a role), otherwise explains. */
export function Gate({ need, children }: { need: 'player' | 'admin' | 'monitor'; children: ReactNode }) {
  const signedIn = useSignedIn()
  const who = useWho()
  const admin = useIsAdmin()
  const monitor = useCanMonitor()
  if (!signedIn) {
    return (
      <Panel className="mx-auto max-w-md text-center">
        <p className="mb-4 text-muted">You need to sign in to see this page.</p>
        <Link to="/signin" className="font-medium text-accent hover:underline">
          Sign in
        </Link>
      </Panel>
    )
  }
  const ok = need === 'player' ? who !== null : need === 'admin' ? admin : monitor
  if (!ok) {
    return (
      <Panel className="mx-auto max-w-md text-center">
        <p className="text-muted">
          {need === 'player' ? 'This page is for player accounts. Sign in with a player account.' : 'Your account does not have access to this page.'}
        </p>
      </Panel>
    )
  }
  return <>{children}</>
}

/** A small stat tile. */
export function Stat({ label, value, tone }: { label: string; value: ReactNode; tone?: string }) {
  return (
    <div className="rounded-card border border-line bg-surface p-4">
      <p className="text-xs font-medium text-muted">{label}</p>
      <p className={`mt-1 font-mono text-2xl tabular-nums ${tone ?? ''}`}>{value}</p>
    </div>
  )
}

/** Shared table look: a bordered, horizontally scrollable card. */
export function DataTable({ caption, head, children }: { caption: string; head: string[]; children: ReactNode }) {
  return (
    <div className="overflow-x-auto rounded-card border border-line bg-surface">
      <table className="w-full min-w-[36rem] text-sm">
        <caption className="sr-only">{caption}</caption>
        <thead>
          <tr className="border-b border-line text-left text-xs text-muted">
            {head.map((h) => (
              <th key={h} scope="col" className="px-4 py-3 font-medium">
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>{children}</tbody>
      </table>
    </div>
  )
}

export const TD = 'border-b border-line px-4 py-3 align-top last:border-b-0'

export function Toggle({ label, checked, onChange, disabled }: { label: string; checked: boolean; onChange: (v: boolean) => void; disabled?: boolean }) {
  return (
    <label className="flex items-center gap-2 text-sm">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} disabled={disabled} className="size-4 accent-[var(--color-accent)]" />
      {label}
    </label>
  )
}
