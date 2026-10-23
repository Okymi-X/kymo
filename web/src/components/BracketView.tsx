import { useMemo, useState, type WheelEvent } from 'react'
import { Minus, Plus } from 'lucide-react'
import type { Bracket, Match, Slot, Stage } from '../api/types'
import { roundLabel } from '../lib/labels'
import { CARD_W, LABEL_H, layoutSection } from '../lib/layout'
import { Button } from './ui'
import { MatchCard } from './MatchCard'

interface Props {
  bracket: Bracket
  teamName: (slot: Slot) => string
  onEdit?: (match: Match) => void
}

const MIN_ZOOM = 0.5
const MAX_ZOOM = 1.5
const STEP = 0.1

const isVoidReset = (m: Match) => m.stage === 'grand_reset' && m.slots.every((s) => s.kind !== 'team')

function Section({
  title,
  matches,
  totals,
  bracket,
  zoom,
  teamName,
  onEdit,
}: {
  title?: string
  matches: Match[]
  totals: Map<Stage, number>
  bracket: Bracket
  zoom: number
  teamName: Props['teamName']
  onEdit?: Props['onEdit']
}) {
  const layout = useMemo(() => layoutSection(matches), [matches])
  if (matches.length === 0) return null
  const width = layout.width
  const height = layout.height + LABEL_H
  return (
    <section aria-label={title}>
      {title && <h3 className="mb-3 text-xs font-medium uppercase tracking-wider text-muted">{title}</h3>}
      <div className="overflow-x-auto pb-4">
        <div style={{ width: width * zoom, height: height * zoom }}>
          <div className="relative origin-top-left" style={{ width, height, transform: `scale(${zoom})` }}>
            {layout.heads.map((head) => (
              <h4 key={head.key} className="absolute top-0 text-xs font-medium text-muted" style={{ left: head.x, width: CARD_W }}>
                {roundLabel(head.stage, head.round, totals.get(head.stage) ?? 0, bracket.format)}
              </h4>
            ))}
            <svg className="absolute left-0 text-muted/40" style={{ top: LABEL_H }} width={width} height={layout.height} fill="none" aria-hidden="true">
              {layout.links.map((link) => (
                <path key={link.key} d={link.path} stroke="currentColor" strokeWidth={1.5} strokeLinejoin="round" />
              ))}
            </svg>
            {layout.placed.map((p) => (
              <MatchCard
                key={p.match.id}
                match={p.match}
                teamName={teamName}
                onEdit={onEdit}
                style={{ position: 'absolute', left: p.x, top: p.y + LABEL_H }}
              />
            ))}
          </div>
        </div>
      </div>
    </section>
  )
}

export function BracketView({ bracket, teamName, onEdit }: Props) {
  const [zoom, setZoom] = useState(1)
  const clamp = (value: number) => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, Math.round(value * 10) / 10))

  const { sections, totals } = useMemo(() => {
    const totals = new Map<Stage, number>()
    for (const m of bracket.matches) totals.set(m.stage, Math.max(totals.get(m.stage) ?? 0, m.round))
    const visible = bracket.matches.filter((m) => !isVoidReset(m))
    const pick = (...stages: Stage[]) => visible.filter((m) => stages.includes(m.stage))
    const double = bracket.format === 'double_elimination'
    return {
      totals,
      sections: [
        { title: double ? 'Winners bracket' : undefined, matches: pick('winners') },
        { title: 'Losers bracket', matches: pick('losers') },
        { title: 'Finals', matches: pick('grand_final', 'grand_reset') },
      ],
    }
  }, [bracket])

  const onWheel = (e: WheelEvent) => {
    if (!e.ctrlKey) return
    setZoom((z) => clamp(z - Math.sign(e.deltaY) * STEP))
  }

  return (
    <div className="flex flex-col gap-6" onWheel={onWheel}>
      <div className="flex items-center gap-1" role="group" aria-label="Zoom">
        <Button variant="ghost" aria-label="Zoom out" onClick={() => setZoom(clamp(zoom - STEP))} disabled={zoom <= MIN_ZOOM}>
          <Minus size={16} />
        </Button>
        <button type="button" className="w-12 text-center font-mono text-xs tabular-nums text-muted hover:text-ink" onClick={() => setZoom(1)} aria-label="Reset zoom">
          {Math.round(zoom * 100)}%
        </button>
        <Button variant="ghost" aria-label="Zoom in" onClick={() => setZoom(clamp(zoom + STEP))} disabled={zoom >= MAX_ZOOM}>
          <Plus size={16} />
        </Button>
      </div>
      <div className="flex flex-col gap-10">
        {sections.map((s, i) => (
          <Section key={i} {...s} totals={totals} bracket={bracket} zoom={zoom} teamName={teamName} onEdit={onEdit} />
        ))}
      </div>
    </div>
  )
}
