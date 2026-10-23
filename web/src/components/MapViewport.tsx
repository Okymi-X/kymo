import { LocateFixed, Maximize, Minus, Plus } from 'lucide-react'
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from 'react'
import {
  PAD,
  centerOn,
  clampScale,
  clampView,
  fitAllScale,
  minimapScale,
  releaseVelocity,
  visibleRect,
  widthScale,
  zoomAt,
  type Sample,
  type Size,
  type View,
} from '../lib/mapview'

export interface MapRect {
  x: number
  y: number
  w: number
  h: number
  hot?: boolean
}

interface Props {
  width: number
  height: number
  /** Everything on the map, drawn as small blocks in the minimap. */
  rects: MapRect[]
  /** A place the "Locate" button flies to, for example the followed team. */
  focus?: MapRect | null
  label: string
  hideControls?: boolean
  /** Fill the parent and start zoomed out to show everything, for the projector and embeds. */
  fill?: boolean
  children: ReactNode
}

const MINIMAP = { w: 220, h: 150 }
const KEY_STEP = 64
const DRAG_THRESHOLD = 4

const reducedMotion = () => typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches

function Minimap({ rects, content, view, viewport, onJump }: { rects: MapRect[]; content: Size; view: View; viewport: Size; onJump: (x: number, y: number) => void }) {
  const m = minimapScale(content, MINIMAP)
  const visible = visibleRect(view, viewport)
  const jump = (e: ReactPointerEvent<HTMLDivElement>) => {
    const box = e.currentTarget.getBoundingClientRect()
    onJump((e.clientX - box.left) / m, (e.clientY - box.top) / m)
  }
  return (
    <div
      data-map-ui
      role="presentation"
      aria-hidden="true"
      onPointerDown={(e) => {
        e.currentTarget.setPointerCapture(e.pointerId)
        jump(e)
      }}
      onPointerMove={(e) => e.buttons === 1 && jump(e)}
      className="absolute bottom-3 right-3 cursor-pointer overflow-hidden rounded-control border border-line bg-surface/90 shadow-[var(--shadow-lift)] backdrop-blur-md"
      style={{ width: content.w * m, height: content.h * m }}
    >
      {rects.map((r, i) => (
        <span
          key={i}
          className={`absolute rounded-[1px] ${r.hot ? 'bg-accent' : 'bg-muted/45'}`}
          style={{ left: r.x * m, top: r.y * m, width: Math.max(1.5, r.w * m), height: Math.max(1.5, r.h * m) }}
        />
      ))}
      <span
        className="absolute rounded-[2px] border border-accent bg-accent/10"
        style={{ left: visible.x * m, top: visible.y * m, width: visible.w * m, height: visible.h * m }}
      />
    </div>
  )
}

/** A pannable, zoomable map: drag to move, Ctrl and wheel or pinch to zoom, arrows and +/- from the keyboard. */
/** Space under the content so the control bar never covers the last row of cards. */
const CONTROLS_ROOM = 48

export function MapViewport({ width, height, rects, focus = null, label, hideControls = false, fill = false, children }: Props) {
  const box = useRef<HTMLDivElement>(null)
  const [vp, setVp] = useState<Size>({ w: 0, h: 0 })
  const vpRef = useRef(vp)
  /** Screen height left under the map's top edge, so the controls and minimap stay on screen. */
  const [room, setRoom] = useState(Infinity)
  const [view, setView] = useState<View>({ x: PAD, y: PAD, s: 1 })
  const viewRef = useRef(view)
  const manual = useRef(false)
  const [glide, setGlide] = useState(false)
  const [dragging, setDragging] = useState(false)
  const content = useMemo<Size>(() => ({ w: width, h: height }), [width, height])
  const contentRef = useRef(content)
  contentRef.current = content

  const pointers = useRef(new Map<number, { x: number; y: number }>())
  const drag = useRef<{ x: number; y: number; ox: number; oy: number; moved: boolean; samples: Sample[] } | null>(null)
  const pinch = useRef<{ dist: number; s: number } | null>(null)
  const swallowClick = useRef(false)
  const raf = useRef<number | null>(null)

  const commit = useCallback((next: View, animate = false) => {
    const clamped = clampView(next, contentRef.current, vpRef.current)
    viewRef.current = clamped
    setView(clamped)
    setGlide(animate)
    return clamped
  }, [])

  const stopMomentum = useCallback(() => {
    if (raf.current !== null) cancelAnimationFrame(raf.current)
    raf.current = null
  }, [])

  useLayoutEffect(() => {
    const el = box.current
    if (!el) return
    const measure = () => {
      const next = { w: el.clientWidth, h: el.clientHeight }
      vpRef.current = next
      setVp(next)
      setRoom(window.innerHeight - (el.getBoundingClientRect().top + window.scrollY) - 24)
    }
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(el)
    window.addEventListener('resize', measure)
    return () => {
      observer.disconnect()
      window.removeEventListener('resize', measure)
    }
  }, [])

  const defaultView = useCallback((): View => {
    const viewport = vpRef.current
    if (fill) {
      const s = fitAllScale(contentRef.current, viewport)
      return { s, x: (viewport.w - contentRef.current.w * s) / 2, y: (viewport.h - contentRef.current.h * s) / 2 }
    }
    return { x: PAD, y: PAD, s: widthScale(contentRef.current.w, viewport.w) }
  }, [fill])

  useEffect(() => {
    if (!manual.current && vp.w > 0) commit(defaultView())
  }, [vp.w, vp.h, width, height, commit, defaultView])

  useEffect(() => stopMomentum, [stopMomentum])

  const baseScale = vp.w > 0 ? widthScale(width, vp.w) : 1
  // Leave room for the header, title and tabs above so the whole map, minimap included, fits on screen.
  const maxHeight = typeof window === 'undefined' ? 700 : window.innerWidth < 640 ? Math.max(320, window.innerHeight * 0.65) : Math.max(280, Math.min(window.innerHeight - 340, 900, room))
  const boxHeight = fill ? undefined : Math.round(Math.min(Math.max(height * baseScale + 2 * PAD + (hideControls ? 0 : CONTROLS_ROOM), 220), maxHeight))

  const zoomBy = useCallback(
    (factor: number, cx = vpRef.current.w / 2, cy = vpRef.current.h / 2, animate = true) => {
      manual.current = true
      stopMomentum()
      commit(zoomAt(viewRef.current, factor, cx, cy), animate)
    },
    [commit, stopMomentum],
  )

  const showAll = () => {
    manual.current = true
    stopMomentum()
    const viewport = vpRef.current
    const s = fitAllScale(content, viewport)
    commit({ s, x: (viewport.w - content.w * s) / 2, y: (viewport.h - content.h * s) / 2 }, true)
  }

  const resetView = () => {
    manual.current = false
    stopMomentum()
    commit(defaultView(), true)
  }

  const actualSize = () => {
    manual.current = true
    zoomBy(1 / viewRef.current.s)
  }

  const locate = () => {
    if (!focus) return
    manual.current = true
    stopMomentum()
    commit(centerOn(focus, Math.max(viewRef.current.s, 0.9), vpRef.current), true)
  }

  useEffect(() => {
    const el = box.current
    if (!el) return
    const onWheel = (e: WheelEvent) => {
      const rect = el.getBoundingClientRect()
      if (e.ctrlKey || e.metaKey) {
        e.preventDefault()
        manual.current = true
        stopMomentum()
        commit(zoomAt(viewRef.current, Math.exp(-e.deltaY * 0.0125), e.clientX - rect.left, e.clientY - rect.top))
        return
      }
      const v = viewRef.current
      const dx = e.shiftKey ? e.deltaY : e.deltaX
      const dy = e.shiftKey ? 0 : e.deltaY
      const next = clampView({ ...v, x: v.x - dx, y: v.y - dy }, contentRef.current, vpRef.current)
      // At an edge, or when everything is visible, let the page scroll instead of trapping the wheel.
      if (next.x === v.x && next.y === v.y) return
      e.preventDefault()
      manual.current = true
      stopMomentum()
      commit(next)
    }
    el.addEventListener('wheel', onWheel, { passive: false })
    return () => el.removeEventListener('wheel', onWheel)
  }, [commit, stopMomentum])

  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    if ((e.pointerType === 'mouse' && e.button !== 0) || (e.target as HTMLElement).closest('[data-map-ui]')) return
    stopMomentum()
    pointers.current.set(e.pointerId, { x: e.clientX, y: e.clientY })
    if (pointers.current.size === 1) {
      const v = viewRef.current
      drag.current = { x: e.clientX, y: e.clientY, ox: v.x, oy: v.y, moved: false, samples: [{ t: performance.now(), x: e.clientX, y: e.clientY }] }
    } else if (pointers.current.size === 2) {
      const [a, b] = [...pointers.current.values()]
      pinch.current = { dist: Math.hypot(a.x - b.x, a.y - b.y), s: viewRef.current.s }
      manual.current = true
      swallowClick.current = true
    }
  }

  const onPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (!pointers.current.has(e.pointerId)) return
    pointers.current.set(e.pointerId, { x: e.clientX, y: e.clientY })
    if (pinch.current && pointers.current.size >= 2) {
      const [a, b] = [...pointers.current.values()]
      const rect = e.currentTarget.getBoundingClientRect()
      const target = clampScale(pinch.current.s * (Math.hypot(a.x - b.x, a.y - b.y) / pinch.current.dist))
      commit(zoomAt(viewRef.current, target / viewRef.current.s, (a.x + b.x) / 2 - rect.left, (a.y + b.y) / 2 - rect.top))
      return
    }
    const d = drag.current
    if (!d) return
    const dx = e.clientX - d.x
    const dy = e.clientY - d.y
    if (!d.moved && Math.hypot(dx, dy) > DRAG_THRESHOLD) {
      d.moved = true
      e.currentTarget.setPointerCapture(e.pointerId)
      manual.current = true
      setDragging(true)
    }
    if (d.moved) {
      commit({ ...viewRef.current, x: d.ox + dx, y: d.oy + dy })
      d.samples.push({ t: performance.now(), x: e.clientX, y: e.clientY })
      if (d.samples.length > 24) d.samples.shift()
    }
  }

  const endPointer = (e: ReactPointerEvent<HTMLDivElement>) => {
    pointers.current.delete(e.pointerId)
    if (pinch.current && pointers.current.size < 2) pinch.current = null
    const d = drag.current
    if (pointers.current.size > 0 || !d) return
    drag.current = null
    setDragging(false)
    if (!d.moved) return
    swallowClick.current = true
    window.setTimeout(() => (swallowClick.current = false), 60)
    if (reducedMotion()) return
    const { vx, vy } = releaseVelocity(d.samples)
    if (Math.hypot(vx, vy) < 0.15) return
    let [px, py, last] = [vx, vy, performance.now()]
    const tick = (now: number) => {
      const dt = Math.min(48, now - last)
      last = now
      const decay = Math.exp(-dt / 325)
      px *= decay
      py *= decay
      const v = viewRef.current
      const next = commit({ ...v, x: v.x + px * dt, y: v.y + py * dt })
      if (next.x === v.x) px = 0
      if (next.y === v.y) py = 0
      raf.current = Math.hypot(px, py) > 0.02 ? requestAnimationFrame(tick) : null
    }
    raf.current = requestAnimationFrame(tick)
  }

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const step = e.shiftKey ? KEY_STEP * 3 : KEY_STEP
    const v = viewRef.current
    const pan = (dx: number, dy: number) => {
      e.preventDefault()
      manual.current = true
      stopMomentum()
      commit({ ...v, x: v.x + dx, y: v.y + dy }, true)
    }
    if (e.key === 'ArrowLeft') pan(step, 0)
    else if (e.key === 'ArrowRight') pan(-step, 0)
    else if (e.key === 'ArrowUp') pan(0, step)
    else if (e.key === 'ArrowDown') pan(0, -step)
    else if (e.key === '+' || e.key === '=') (e.preventDefault(), zoomBy(1.25))
    else if (e.key === '-' || e.key === '_') (e.preventDefault(), zoomBy(0.8))
    else if (e.key === '0') (e.preventDefault(), resetView())
    else if (e.key.toLowerCase() === 'f') (e.preventDefault(), showAll())
  }

  const overflow = content.w * view.s > vp.w - 2 * PAD + 8 || content.h * view.s > vp.h - 2 * PAD + 8
  const buttonClass = 'flex size-8 items-center justify-center rounded-[calc(var(--radius-control)-2px)] text-muted transition-colors hover:bg-line/60 hover:text-ink'

  return (
    <div
      ref={box}
      data-map
      role="region"
      tabIndex={0}
      aria-label={label}
      aria-describedby="map-help"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={endPointer}
      onPointerCancel={endPointer}
      onClickCapture={(e) => {
        if (swallowClick.current) {
          e.stopPropagation()
          e.preventDefault()
          swallowClick.current = false
        }
      }}
      onKeyDown={onKeyDown}
      style={{ height: boxHeight }}
      className={`relative touch-none select-none overflow-hidden rounded-card border border-line bg-surface/40 [overscroll-behavior:contain] ${fill ? 'h-full min-h-[18rem]' : ''} ${dragging ? 'cursor-grabbing' : 'cursor-grab'}`}
    >
      <p id="map-help" className="sr-only">
        Drag to move the map. Hold Control and scroll, or pinch, to zoom. Arrow keys move, plus and minus zoom, zero resets, F shows everything.
      </p>
      <div
        data-map-canvas
        style={{
          width,
          height,
          transform: `translate3d(${view.x}px, ${view.y}px, 0) scale(${view.s})`,
          transformOrigin: '0 0',
          transition: glide && !dragging && !reducedMotion() ? 'transform 240ms cubic-bezier(0.23, 1, 0.32, 1)' : 'none',
          willChange: 'transform',
        }}
        className="absolute left-0 top-0"
      >
        {children}
      </div>

      {!hideControls && (
        <>
          <div data-map-ui role="group" aria-label="Map controls" className="no-print absolute bottom-3 left-3 flex items-center gap-0.5 rounded-control border border-line bg-surface/90 p-0.5 shadow-[var(--shadow-lift)] backdrop-blur-md">
            <button type="button" className={buttonClass} aria-label="Zoom out" onClick={() => zoomBy(0.8)}>
              <Minus size={16} aria-hidden="true" />
            </button>
            <button type="button" className="min-w-12 rounded-[calc(var(--radius-control)-2px)] px-1 text-center font-mono text-xs tabular-nums text-muted hover:bg-line/60 hover:text-ink" aria-label="Actual size" title="Actual size" onClick={actualSize}>
              {Math.round(view.s * 100)}%
            </button>
            <button type="button" className={buttonClass} aria-label="Zoom in" onClick={() => zoomBy(1.25)}>
              <Plus size={16} aria-hidden="true" />
            </button>
            <span className="mx-0.5 h-5 w-px bg-line" aria-hidden="true" />
            <button type="button" className={buttonClass} aria-label="Show everything" title="Show everything" onClick={showAll}>
              <Maximize size={15} aria-hidden="true" />
            </button>
            <button type="button" className="rounded-[calc(var(--radius-control)-2px)] px-2 text-xs font-medium text-muted hover:bg-line/60 hover:text-ink" aria-label="Reset view" onClick={resetView}>
              Reset
            </button>
            <span className="ml-1 mr-2 hidden whitespace-nowrap text-xs text-muted lg:inline" aria-hidden="true">
              Drag to move · Ctrl + scroll to zoom
            </span>
            {focus && (
              <button type="button" className={buttonClass} aria-label="Locate my team" title="Locate my team" onClick={locate}>
                <LocateFixed size={16} aria-hidden="true" />
              </button>
            )}
          </div>
          {overflow && <Minimap rects={rects} content={content} view={view} viewport={vp} onJump={(x, y) => {
                manual.current = true
                stopMomentum()
                commit(centerOn({ x, y, w: 0, h: 0 }, viewRef.current.s, vpRef.current))
              }} />}
        </>
      )}
    </div>
  )
}
