import { Check } from 'lucide-react'
import { motion } from 'motion/react'
import { planLine } from '../lib/schedule'
import type { CSSProperties } from 'react'
import type { Match, Slot } from '../api/types'
import { Avatar } from './Avatar'

interface Props {
  match: Match
  teamName: (slot: Slot) => string
  onEdit?: (match: Match) => void
  style?: CSSProperties
  highlight?: number | null
  /** League matches are all open at once, so skip the accent ring. */
  quiet?: boolean
  onHoverTeam?: (team: number | null) => void
}

function isReady(match: Match) {
  return match.slots.every((s) => s.kind === 'team')
}

function Row({
  name,
  slot,
  score,
  won,
  forfeit,
  lit,
  onHover,
}: {
  name: string
  slot: Slot
  score: number | null
  won: boolean
  forfeit: boolean
  lit: boolean
  onHover?: (team: number | null) => void
}) {
  const dim = slot.kind !== 'team'
  const team = slot.kind === 'team' ? slot.team : null
  return (
    <div
      onPointerEnter={() => onHover?.(team)}
      onPointerLeave={() => onHover?.(null)}
      className={`flex flex-1 items-center justify-between gap-3 px-3 transition-colors ${lit ? 'bg-accent/15' : ''}`}
    >
      <motion.span
        key={name}
        initial={{ opacity: 0, y: 4 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ type: 'spring', duration: 0.3, bounce: 0 }}
        className={`flex min-w-0 items-center gap-1.5 truncate ${dim ? 'text-muted' : ''} ${won ? 'font-semibold' : ''}`}
      >
        {slot.kind === 'team' && <Avatar name={name} size={20} />}
        <span className="truncate">{name}</span>
        {won && <Check size={14} aria-label="Winner" className="shrink-0 text-win" />}
      </motion.span>
      <span className={`font-mono tabular-nums ${won ? 'font-semibold' : 'text-muted'}`}>{forfeit ? (won ? 'W' : 'FF') : (score ?? '')}</span>
    </div>
  )
}

export function MatchCard({ match, teamName, onEdit, style, highlight = null, quiet = false, onHoverTeam }: Props) {
  const ready = isReady(match)
  const plan = planLine(match)
  const open = !quiet && ready && !match.forfeit && match.score === null
  const body = (
    <>
      {plan && match.winner === null && (
        <span title={plan} className="absolute right-2 top-0.5 max-w-[11rem] truncate text-[10px] font-medium text-accent">
          {plan}
        </span>
      )}
      <Row name={teamName(match.slots[0])} slot={match.slots[0]} score={match.score?.[0] ?? null} won={match.winner === 'A'} forfeit={match.forfeit} lit={highlight !== null && match.slots[0].kind === 'team' && (match.slots[0] as { team: number }).team === highlight} onHover={onHoverTeam} />
      <div className="border-t border-line" />
      <Row name={teamName(match.slots[1])} slot={match.slots[1]} score={match.score?.[1] ?? null} won={match.winner === 'B'} forfeit={match.forfeit} lit={highlight !== null && match.slots[1].kind === 'team' && (match.slots[1] as { team: number }).team === highlight} onHover={onHoverTeam} />
    </>
  )
  const frame = `relative flex h-[76px] w-56 flex-col overflow-hidden rounded-card border bg-surface text-left shadow-[var(--shadow-card)] ${open ? 'border-accent ring-1 ring-accent' : 'border-line'}`
  if (onEdit && ready) {
    return (
      <button
        type="button"
        onClick={() => onEdit(match)}
        className={`${frame} transition-colors hover:bg-line/30`}
        style={style}
        aria-label={`Edit result for ${teamName(match.slots[0])} versus ${teamName(match.slots[1])}`}
      >
        {body}
      </button>
    )
  }
  return (
    <div className={frame} style={style}>
      {body}
    </div>
  )
}
