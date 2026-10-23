export interface View {
  /** Position of the content's top left corner inside the viewport, in pixels. */
  x: number
  y: number
  /** Zoom factor, 1 is actual size. */
  s: number
}

export interface Size {
  w: number
  h: number
}

export const MIN_SCALE = 0.15
export const MAX_SCALE = 2
export const PAD = 16

export const clampScale = (s: number) => Math.min(MAX_SCALE, Math.max(MIN_SCALE, s))

/** Keeps the content reachable: smaller than the viewport it sits at the padding, larger it can pan to its edges. */
export function clampView(view: View, content: Size, viewport: Size): View {
  const axis = (pos: number, contentSize: number, viewSize: number) => {
    const scaled = contentSize * view.s
    if (scaled <= viewSize - 2 * PAD) return PAD
    return Math.min(PAD, Math.max(viewSize - scaled - PAD, pos))
  }
  return { s: view.s, x: axis(view.x, content.w, viewport.w), y: axis(view.y, content.h, viewport.h) }
}

/** Zooms by `factor` while the content point under (cx, cy) stays where it is. */
export function zoomAt(view: View, factor: number, cx: number, cy: number): View {
  const s = clampScale(view.s * factor)
  const ratio = s / view.s
  return { s, x: cx - (cx - view.x) * ratio, y: cy - (cy - view.y) * ratio }
}

/** Scale that shows the full width, never enlarging and never going below `floor`. */
export function widthScale(contentW: number, viewportW: number, floor = 0.6): number {
  if (contentW <= 0 || viewportW <= 0) return 1
  return Math.min(1, Math.max(floor, Math.floor((viewportW / contentW) * 100) / 100))
}

/** Scale that shows the entire content at once. */
export function fitAllScale(content: Size, viewport: Size): number {
  if (content.w <= 0 || content.h <= 0) return 1
  return clampScale(Math.min(1, (viewport.w - 2 * PAD) / content.w, (viewport.h - 2 * PAD) / content.h))
}

/** A view that puts the content rectangle's center in the middle of the viewport. */
export function centerOn(rect: { x: number; y: number; w: number; h: number }, s: number, viewport: Size): View {
  return { s, x: viewport.w / 2 - (rect.x + rect.w / 2) * s, y: viewport.h / 2 - (rect.y + rect.h / 2) * s }
}

/** Minimap scale so the whole content fits a box. */
export function minimapScale(content: Size, box: Size): number {
  if (content.w <= 0 || content.h <= 0) return 1
  return Math.min(box.w / content.w, box.h / content.h)
}

/** Content rectangle currently visible, in content coordinates. */
export function visibleRect(view: View, viewport: Size) {
  return { x: -view.x / view.s, y: -view.y / view.s, w: viewport.w / view.s, h: viewport.h / view.s }
}

export interface Sample {
  t: number
  x: number
  y: number
}

/** Release velocity in px per millisecond from the last 100ms of pointer samples. */
export function releaseVelocity(samples: Sample[]): { vx: number; vy: number } {
  const last = samples.at(-1)
  if (!last) return { vx: 0, vy: 0 }
  const first = samples.find((p) => last.t - p.t <= 100) ?? last
  const dt = last.t - first.t
  if (dt <= 0) return { vx: 0, vy: 0 }
  return { vx: (last.x - first.x) / dt, vy: (last.y - first.y) / dt }
}
