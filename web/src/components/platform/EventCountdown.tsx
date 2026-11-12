import type { EventInfo } from '../../api/platformTypes'
import { eventCountdown, eventCountdownTarget, eventEndedAgo } from '../../lib/platform'
import { useNow } from './bits'

const tones = {
  success: 'text-win',
  warning: 'text-warning',
  danger: 'text-danger',
  ended: 'text-muted',
}

/** Counts to the start or the end of an event or tournament; anything with this schedule works. */
export function EventCountdown({ event, compact = false }: { event: Pick<EventInfo, 'effective_state' | 'starts_at' | 'ends_at' | 'ended_at'>; compact?: boolean }) {
  const now = useNow()
  const nowSeconds = Math.floor(now / 1000)
  const target = eventCountdownTarget(event, nowSeconds)
  if (!target) return null

  const remaining = target.kind === 'starts' || target.kind === 'ends' ? target.at - now / 1000 : null
  const label = target.kind === 'starts' ? 'Starts in' : target.kind === 'ends' ? 'Ends in' : 'Ended'
  const value =
    target.kind === 'ended'
      ? target.at === null
        ? null
        : eventEndedAgo(target.at, nowSeconds)
      : eventCountdown(remaining ?? 0)
  const tone =
    target.kind === 'ended'
      ? 'ended'
      : target.kind === 'ends' && remaining !== null && remaining < 120
        ? 'danger'
        : target.kind === 'ends' && remaining !== null && remaining < 300
          ? 'warning'
          : 'success'
  const accessibleText = value ? `${label} ${value}` : label

  return (
    <div
      role="timer"
      aria-label={accessibleText}
      className={`inline-flex max-w-full flex-col items-end self-end text-right ${compact ? 'gap-0.5' : 'gap-1'} ${tones[tone]}`}
    >
      <span className={compact ? 'text-xs font-medium' : 'text-sm font-semibold'}>{label}</span>
      {value && (
        <span className={`break-words font-mono tabular-nums tracking-tight ${compact ? 'text-base font-semibold' : 'text-xl font-black sm:text-2xl'}`}>
          {value}
        </span>
      )}
    </div>
  )
}
