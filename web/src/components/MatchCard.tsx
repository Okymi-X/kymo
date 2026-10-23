import { Check } from 'lucide-react'
import { motion } from 'motion/react'
import type { CSSProperties } from 'react'
import type { Match, Slot } from '../api/types'
import { Avatar } from './Avatar'

interface Props {
  match: Match
  teamName: (slot: Slot) => string
  onEdit?: (match: Match) => void
  style?: CSSProperties
}

function isReady(match: Match) {
  return match.slots.every((s) => s.kind === 'team')
}

function Row({ name, slot, score, won, forfeit }: { name: string; slot: Slot; score: number | null; won: boolean; forfeit: boolean }) {
  const dim = slot.kind !== 'team'
  return (
    <div className="flex flex-1 items-center justify-between gap-3 px-3">
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

export function MatchCard({ match, teamName, onEdit, style }: Props) {
  const ready = isReady(match)
  const open = ready && match.winner === null
  const body = (
    <>
      <Row name={teamName(match.slots[0])} slot={match.slots[0]} score={match.score?.[0] ?? null} won={match.winner === 'A'} forfeit={match.forfeit} />
      <div className="border-t border-line" />
      <Row name={teamName(match.slots[1])} slot={match.slots[1]} score={match.score?.[1] ?? null} won={match.winner === 'B'} forfeit={match.forfeit} />
    </>
  )
  const frame = `flex h-[76px] w-56 flex-col overflow-hidden rounded-card border bg-surface text-left ${open ? 'border-accent ring-1 ring-accent' : 'border-line'}`
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
