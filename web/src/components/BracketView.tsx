import { useLayoutEffect, useMemo, useRef, useState } from 'react'
import type { Bracket, Match, Slot, Stage } from '../api/types'
import { roundLabel } from '../lib/labels'
import { CARD_H, CARD_W, layoutBracket, type BracketLayout } from '../lib/layout'
import { MapViewport, type MapRect } from './MapViewport'
import { MatchCard } from './MatchCard'

interface Props {
  bracket: Bracket
  /** A team to keep highlighted, for example the one a visitor follows. */
  pinned?: number | null
  /** Stay inside the page column instead of using the full window width. */
  contained?: boolean
  /** Hide the map controls, for the projector screen. */
  hideControls?: boolean
  /** Fill the parent and show the whole bracket, for the projector screen and embeds. */
  fill?: boolean
  teamName: (slot: Slot) => string
  onEdit?: (match: Match) => void
}

const isVoidReset = (m: Match) => m.stage === 'grand_reset' && m.slots.every((s) => s.kind !== 'team')

const involves = (m: Match, team: number | null) => team !== null && m.slots.some((s) => s.kind === 'team' && s.team === team)

function Canvas({
  bracket,
  layout,
  teamName,
  onEdit,
  highlight,
  onHoverTeam,
}: { bracket: Bracket; layout: BracketLayout; highlight: number | null; onHoverTeam: (team: number | null) => void } & Pick<Props, 'teamName' | 'onEdit'>) {
  const totals = useMemo(() => {
    const out = new Map<Stage, number>()
    for (const m of bracket.matches) out.set(m.stage, Math.max(out.get(m.stage) ?? 0, m.round))
    return out
  }, [bracket])
  return (
    <div className="relative" style={{ width: layout.width, height: layout.height }}>
      {layout.titles.map((t) => (
        <h3 key={t.text} className="absolute text-xs font-medium uppercase tracking-wider text-muted" style={{ left: t.x, top: t.y }}>
          {t.text}
        </h3>
      ))}
      {layout.heads.map((head) => (
        <h4 key={head.key} className="absolute text-xs font-medium text-muted" style={{ left: head.x, top: head.y, width: CARD_W }}>
          {roundLabel(head.stage, head.round, totals.get(head.stage) ?? 0, bracket.format)}
        </h4>
      ))}
      <svg className="absolute left-0 top-0 text-muted/40" width={layout.width} height={layout.height} fill="none" aria-hidden="true">
        {layout.links.map((link) => (
          <path key={link.key} d={link.path} stroke="currentColor" strokeWidth={1.5} strokeLinejoin="round" />
        ))}
      </svg>
      {layout.placed
        .filter((p) => p.match.stage === 'third_place')
        .map((p) => (
          <h4 key={`cap-${p.match.id}`} className="absolute text-xs font-medium text-muted" style={{ left: p.x, top: p.y - 18, width: CARD_W }}>
            Third place
          </h4>
        ))}
      {layout.placed.map((p) => (
        <MatchCard key={p.match.id} match={p.match} teamName={teamName} onEdit={onEdit} highlight={highlight} onHoverTeam={onHoverTeam} style={{ position: 'absolute', left: p.x, top: p.y }} />
      ))}
    </div>
  )
}

/** The bracket as a map: drag it around, zoom to a spot, jump with the minimap. */
export function BracketView({ bracket, teamName, onEdit, pinned = null, contained = false, hideControls = false, fill = false }: Props) {
  const [column, setColumn] = useState(0)
  const [hovered, setHovered] = useState<number | null>(null)
  const columnRef = useRef<HTMLDivElement>(null)

  const layout = useMemo(() => layoutBracket(bracket.matches.filter((m) => !isVoidReset(m))), [bracket])

  useLayoutEffect(() => {
    const el = columnRef.current
    if (!el) return
    const measure = () => setColumn(el.clientWidth)
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(el)
    return () => observer.disconnect()
  }, [])

  const wide = !contained && !fill && column > 0 && layout.width > column
  const highlight = hovered ?? pinned

  const rects = useMemo<MapRect[]>(() => layout.placed.map((p) => ({ x: p.x, y: p.y, w: CARD_W, h: CARD_H, hot: involves(p.match, highlight) })), [layout, highlight])

  const focus = useMemo<MapRect | null>(() => {
    const mine = layout.placed.filter((p) => involves(p.match, pinned))
    const last = mine.sort((a, b) => b.x - a.x)[0]
    return last ? { x: last.x, y: last.y, w: CARD_W, h: CARD_H } : null
  }, [layout, pinned])

  return (
    <>
      <div ref={columnRef} aria-hidden="true" className="h-0" />
      <div className={wide ? 'relative left-1/2 w-[min(calc(100vw-3rem),1680px)] -translate-x-1/2' : fill ? 'h-full' : ''}>
        <MapViewport width={layout.width} height={layout.height} rects={rects} focus={focus} label="Bracket map" hideControls={hideControls} fill={fill}>
          <Canvas bracket={bracket} layout={layout} teamName={teamName} onEdit={onEdit} highlight={highlight} onHoverTeam={setHovered} />
        </MapViewport>
      </div>
    </>
  )
}
