import { useLayoutEffect, useMemo, useRef, useState, type WheelEvent } from 'react'
import { Minus, Plus } from 'lucide-react'
import type { Bracket, Match, Slot, Stage } from '../api/types'
import { roundLabel } from '../lib/labels'
import { CARD_W, layoutBracket, type BracketLayout } from '../lib/layout'
import { Button } from './ui'
import { MatchCard } from './MatchCard'

interface Props {
  bracket: Bracket
  /** A team to keep highlighted, for example the one a visitor follows. */
  pinned?: number | null
  /** Stay inside the page column instead of using the full window width. */
  contained?: boolean
  /** Hide the zoom buttons, for the projector screen. */
  hideControls?: boolean
  teamName: (slot: Slot) => string
  onEdit?: (match: Match) => void
}

const MIN_ZOOM = 0.5
const MIN_FIT = 0.6
const MAX_ZOOM = 1.5
const STEP = 0.1

const isVoidReset = (m: Match) => m.stage === 'grand_reset' && m.slots.every((s) => s.kind !== 'team')

function Canvas({
  bracket,
  layout,
  zoom,
  teamName,
  onEdit,
  highlight,
  onHoverTeam,
}: { bracket: Bracket; layout: BracketLayout; zoom: number; highlight: number | null; onHoverTeam: (team: number | null) => void } & Omit<Props, 'bracket'>) {
  const totals = useMemo(() => {
    const out = new Map<Stage, number>()
    for (const m of bracket.matches) out.set(m.stage, Math.max(out.get(m.stage) ?? 0, m.round))
    return out
  }, [bracket])
  const { width, height } = layout
  return (
    <div className="overflow-x-auto pb-4">
      <div style={{ width: width * zoom, height: height * zoom }}>
        <div className="relative origin-top-left" style={{ width, height, transform: `scale(${zoom})` }}>
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
          <svg className="absolute left-0 top-0 text-muted/40" width={width} height={height} fill="none" aria-hidden="true">
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
      </div>
    </div>
  )
}

export function BracketView({ bracket, teamName, onEdit, pinned = null, contained = false, hideControls = false }: Props) {
  const [manual, setManual] = useState<number | null>(null)
  const [available, setAvailable] = useState(0)
  const [column, setColumn] = useState(0)
  const [hovered, setHovered] = useState<number | null>(null)
  const hostRef = useRef<HTMLDivElement>(null)
  const columnRef = useRef<HTMLDivElement>(null)
  const clamp = (value: number) => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, Math.round(value * 10) / 10))

  const layout = useMemo(() => layoutBracket(bracket.matches.filter((m) => !isVoidReset(m))), [bracket])

  useLayoutEffect(() => {
    const host = hostRef.current
    if (!host) return
    const column = columnRef.current
    const measure = () => {
      setAvailable(host.clientWidth)
      setColumn(column?.clientWidth ?? 0)
    }
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(host)
    if (column) observer.observe(column)
    return () => observer.disconnect()
  }, [])

  const wide = !contained && column > 0 && layout.width > column
  const fit = available > 0 && layout.width > available ? Math.max(MIN_FIT, Math.floor((available / layout.width) * 100) / 100) : 1
  const zoom = manual ?? fit

  const onWheel = (e: WheelEvent) => {
    if (!e.ctrlKey) return
    setManual(clamp(zoom - Math.sign(e.deltaY) * STEP))
  }

  return (
    <div className="flex flex-col gap-6" onWheel={onWheel}>
      {!hideControls && (
      <div className="flex items-center gap-1" role="group" aria-label="Zoom">
        <Button variant="ghost" aria-label="Zoom out" onClick={() => setManual(clamp(zoom - STEP))} disabled={zoom <= MIN_ZOOM}>
          <Minus size={16} />
        </Button>
        <button
          type="button"
          className="min-w-14 text-center font-mono text-xs tabular-nums text-muted hover:text-ink"
          onClick={() => setManual(null)}
          aria-label="Fit bracket to width"
          title="Fit to width"
        >
          {Math.round(zoom * 100)}%
        </button>
        <Button variant="ghost" aria-label="Zoom in" onClick={() => setManual(clamp(zoom + STEP))} disabled={zoom >= MAX_ZOOM}>
          <Plus size={16} />
        </Button>
      </div>
      )}
      <div ref={columnRef} aria-hidden="true" className="h-0" />
      <div ref={hostRef} className={wide ? 'relative left-1/2 w-[min(calc(100vw-3rem),1680px)] -translate-x-1/2' : ''}>
        <Canvas bracket={bracket} layout={layout} zoom={zoom} teamName={teamName} onEdit={onEdit} highlight={hovered ?? pinned} onHoverTeam={setHovered} />
      </div>
    </div>
  )
}
