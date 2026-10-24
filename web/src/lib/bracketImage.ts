import type { Bracket, Match, Slot } from '../api/types'
import { CARD_H, CARD_W, layoutBracket } from './layout'
import { roundLabel } from './labels'

const PAD = 24
const FONT = "system-ui, -apple-system, 'Segoe UI', sans-serif"

interface Palette {
  bg: string
  surface: string
  line: string
  ink: string
  muted: string
  accent: string
}

const LIGHT: Palette = { bg: '#fafafa', surface: '#ffffff', line: '#d9d9de', ink: '#1d1d1f', muted: '#6e6e73', accent: '#0a6fe0' }
const DARK: Palette = { bg: '#0b0b0c', surface: '#141416', line: '#2a2a2e', ink: '#f5f5f7', muted: '#98989d', accent: '#4da3ff' }

const esc = (text: string) => text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')

/** Fits a name into a card row by cutting it, since SVG text does not wrap or clip by itself. */
function fit(name: string, max = 22): string {
  return name.length > max ? `${name.slice(0, max - 1)}…` : name
}

const isVoidReset = (m: Match) => m.stage === 'grand_reset' && m.slots.every((s) => s.kind !== 'team')

export interface BracketImage {
  svg: string
  width: number
  height: number
}

/** Draws the whole bracket as a standalone SVG: the same layout as the map, on a plain background. */
export function bracketImage(bracket: Bracket, teamName: (slot: Slot) => string, title: string, dark = false): BracketImage {
  const c = dark ? DARK : LIGHT
  const layout = layoutBracket(bracket.matches.filter((m) => !isVoidReset(m)))
  const top = 44
  const width = layout.width + PAD * 2
  const height = layout.height + PAD * 2 + top
  const totals = new Map<string, number>()
  for (const m of bracket.matches) totals.set(m.stage, Math.max(totals.get(m.stage) ?? 0, m.round))
  const ox = PAD
  const oy = PAD + top

  const parts: string[] = []
  parts.push(`<rect width="${width}" height="${height}" fill="${c.bg}"/>`)
  parts.push(`<text x="${PAD}" y="${PAD + 18}" font-family="${FONT}" font-size="20" font-weight="600" fill="${c.ink}">${esc(title)}</text>`)
  for (const t of layout.titles) {
    parts.push(`<text x="${ox + t.x}" y="${oy + t.y + 14}" font-family="${FONT}" font-size="11" font-weight="600" fill="${c.muted}" letter-spacing="1">${esc(t.text.toUpperCase())}</text>`)
  }
  for (const h of layout.heads) {
    const label = roundLabel(h.stage, h.round, totals.get(h.stage) ?? 0, bracket.format)
    parts.push(`<text x="${ox + h.x}" y="${oy + h.y + 16}" font-family="${FONT}" font-size="12" fill="${c.muted}">${esc(label)}</text>`)
  }
  for (const link of layout.links) {
    parts.push(`<path d="${link.team !== undefined ? link.curve : link.path}" transform="translate(${ox} ${oy})" fill="none" stroke="${c.line}" stroke-width="${link.team !== undefined ? 1 : 1.5}"/>`)
  }
  for (const p of layout.placed) {
    const x = ox + p.x
    const y = oy + p.y
    parts.push(`<rect x="${x}" y="${y}" width="${CARD_W}" height="${CARD_H}" rx="8" fill="${c.surface}" stroke="${c.line}"/>`)
    parts.push(`<line x1="${x}" y1="${y + CARD_H / 2}" x2="${x + CARD_W}" y2="${y + CARD_H / 2}" stroke="${c.line}"/>`)
    ;([0, 1] as const).forEach((side) => {
      const slot = p.match.slots[side]
      const won = p.match.winner === (side === 0 ? 'A' : 'B')
      const name = slot.kind === 'team' ? teamName(slot) : slot.kind === 'bye' ? 'Bye' : 'TBD'
      const score = p.match.forfeit ? (won ? 'W' : 'FF') : p.match.score ? String(p.match.score[side]) : ''
      const cy = y + (side === 0 ? CARD_H / 4 : (CARD_H * 3) / 4) + 4
      const color = slot.kind === 'team' ? c.ink : c.muted
      parts.push(`<text x="${x + 12}" y="${cy}" font-family="${FONT}" font-size="13" font-weight="${won ? 700 : 400}" fill="${color}">${esc(fit(name))}</text>`)
      if (score) parts.push(`<text x="${x + CARD_W - 12}" y="${cy}" text-anchor="end" font-family="monospace" font-size="13" font-weight="${won ? 700 : 400}" fill="${won ? c.accent : c.muted}">${esc(score)}</text>`)
    })
  }
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">${parts.join('')}</svg>`
  return { svg, width, height }
}
