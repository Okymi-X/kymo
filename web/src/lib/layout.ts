import type { Match, Stage } from '../api/types'

export const CARD_W = 224
export const CARD_H = 76
export const COL_GAP = 48
export const ROW_GAP = 16
export const LABEL_H = 28
export const TITLE_H = 28
export const GROUP_GAP = 40

export interface Placed {
  match: Match
  x: number
  y: number
}

export interface Link {
  key: string
  path: string
  /** The same connection drawn as a smooth curve. */
  curve: string
}

export interface ColumnHead {
  key: string
  x: number
  y: number
  stage: Stage
  round: number
}

export interface GroupTitle {
  text: string
  x: number
  y: number
}

export interface BracketLayout {
  placed: Placed[]
  links: Link[]
  heads: ColumnHead[]
  titles: GroupTitle[]
  width: number
  height: number
}

const stageRank: Record<Stage, number> = { winners: 0, losers: 1, grand_final: 2, grand_reset: 3, third_place: 4, league: 5 }

/** The third place match shares the final's column, below it. */
const columnKey = (m: Match) => `${m.stage === 'third_place' ? 'winners' : m.stage}-${m.round}`

const THIRD_PLACE_CAPTION = 20

const GROUPS: { title: string; stages: Stage[] }[] = [
  { title: 'Winners bracket', stages: ['winners', 'third_place'] },
  { title: 'Losers bracket', stages: ['losers'] },
  { title: 'Finals', stages: ['grand_final', 'grand_reset'] },
]

/**
 * Lays the whole bracket on one canvas. Winners and losers stack on the left, finals sit to
 * their right. A match is placed at the mean height of the matches that feed it, so the
 * winner lines up between the pair it came from, including into the grand final.
 */
export function layoutBracket(matches: Match[]): BracketLayout {
  const feeders = new Map<number, number[]>()
  for (const m of matches) {
    if (m.next_win) feeders.set(m.next_win.id, [...(feeders.get(m.next_win.id) ?? []), m.id])
  }
  const titled = matches.some((m) => m.stage === 'losers')

  const placed = new Map<number, Placed>()
  const heads: ColumnHead[] = []
  const titles: GroupTitle[] = []
  let cursor = 0
  let sideWidth = 0

  GROUPS.forEach((group, groupIndex) => {
    const inGroup = matches
      .filter((m) => group.stages.includes(m.stage))
      .sort((a, b) => stageRank[a.stage] - stageRank[b.stage] || a.round - b.round || a.position - b.position)
    if (inGroup.length === 0) return

    const isFinals = groupIndex === 2
    const x0 = isFinals ? sideWidth + COL_GAP : 0
    const top = isFinals ? 0 : cursor
    if (titled) titles.push({ text: group.title, x: x0, y: top })
    const headY = top + (titled ? TITLE_H : 0)
    const y0 = headY + LABEL_H

    const keys = [...new Set(inGroup.map(columnKey))]
    keys.forEach((key, col) => {
      const x = x0 + col * (CARD_W + COL_GAP)
      const column = inGroup.filter((m) => columnKey(m) === key)
      heads.push({ key, x, y: headY, stage: column[0].stage, round: column[0].round })
      let floor = y0
      column.forEach((match, index) => {
        const above = (feeders.get(match.id) ?? []).map((id) => placed.get(id)).filter((p): p is Placed => !!p)
        const wanted = above.length ? above.reduce((sum, p) => sum + p.y, 0) / above.length : y0 + index * (CARD_H + ROW_GAP)
        const y = Math.max(wanted, floor + (match.stage === 'third_place' ? THIRD_PLACE_CAPTION : 0))
        floor = y + CARD_H + ROW_GAP
        placed.set(match.id, { match, x, y })
      })
    })

    const bottom = Math.max(...inGroup.map((m) => (placed.get(m.id)?.y ?? 0) + CARD_H))
    if (!isFinals) {
      cursor = bottom + GROUP_GAP
      sideWidth = Math.max(sideWidth, keys.length * (CARD_W + COL_GAP) - COL_GAP)
    }
  })

  const links: Link[] = []
  for (const from of placed.values()) {
    const to = from.match.next_win ? placed.get(from.match.next_win.id) : undefined
    if (!to) continue
    const x1 = from.x + CARD_W
    const y1 = from.y + CARD_H / 2
    const x2 = to.x
    const y2 = to.y + CARD_H / 2
    const mid = x2 - COL_GAP / 2
    links.push({ key: `${from.match.id}-${to.match.id}`, path: `M${x1} ${y1}H${mid}V${y2}H${x2}`, curve: `M${x1} ${y1}C${mid} ${y1} ${mid} ${y2} ${x2} ${y2}` })
  }

  const all = [...placed.values()]
  return {
    placed: all,
    links,
    heads,
    titles,
    width: all.reduce((max, p) => Math.max(max, p.x + CARD_W), 0),
    height: all.reduce((max, p) => Math.max(max, p.y + CARD_H), 0),
  }
}

/** Reference projector area for the stage bracket slide, in px (a 1080p screen minus header and sidebar). */
export const STAGE_AREA = { w: 1440, h: 880 }
/** Below this zoom a card is too small to read from across a room. */
export const STAGE_MIN_SCALE = 0.55

/** Zoom at which the whole bracket fits the stage area. */
export function stageScale(matches: Match[]): number {
  const { width, height } = layoutBracket(matches)
  if (width <= 0 || height <= 0) return 1
  return Math.min(1, STAGE_AREA.w / width, STAGE_AREA.h / height)
}

export const stageFits = (matches: Match[]) => stageScale(matches) >= STAGE_MIN_SCALE
