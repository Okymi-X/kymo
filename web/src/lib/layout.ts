import type { Match, Stage } from '../api/types'

export const CARD_W = 224
export const CARD_H = 76
export const COL_GAP = 48
export const ROW_GAP = 16
export const LABEL_H = 28

export interface Placed {
  match: Match
  x: number
  y: number
}

export interface Link {
  key: string
  path: string
}

export interface ColumnHead {
  key: string
  x: number
  stage: Stage
  round: number
}

export interface SectionLayout {
  placed: Placed[]
  links: Link[]
  heads: ColumnHead[]
  width: number
  height: number
}

const stageRank: Record<Stage, number> = { winners: 0, losers: 1, grand_final: 2, grand_reset: 3 }

/**
 * Places matches in columns by round. A match sits at the mean height of the matches
 * that feed it, so winners lines up between the pair it came from.
 */
export function layoutSection(matches: Match[]): SectionLayout {
  const ordered = [...matches].sort(
    (a, b) => stageRank[a.stage] - stageRank[b.stage] || a.round - b.round || a.position - b.position,
  )
  const columnKeys = [...new Set(ordered.map((m) => `${m.stage}-${m.round}`))]
  const feeders = new Map<number, number[]>()
  for (const m of ordered) {
    if (m.next_win) feeders.set(m.next_win.id, [...(feeders.get(m.next_win.id) ?? []), m.id])
  }

  const placed = new Map<number, Placed>()
  const heads: ColumnHead[] = []
  columnKeys.forEach((key, col) => {
    const x = col * (CARD_W + COL_GAP)
    const inColumn = ordered.filter((m) => `${m.stage}-${m.round}` === key)
    heads.push({ key, x, stage: inColumn[0].stage, round: inColumn[0].round })
    let floor = 0
    inColumn.forEach((match, index) => {
      const above = (feeders.get(match.id) ?? []).map((id) => placed.get(id)).filter((p): p is Placed => !!p)
      const wanted = above.length
        ? above.reduce((sum, p) => sum + p.y, 0) / above.length
        : index * (CARD_H + ROW_GAP)
      const y = Math.max(wanted, floor)
      floor = y + CARD_H + ROW_GAP
      placed.set(match.id, { match, x, y })
    })
  })

  const links: Link[] = []
  for (const from of placed.values()) {
    const to = from.match.next_win ? placed.get(from.match.next_win.id) : undefined
    if (!to) continue
    const x1 = from.x + CARD_W
    const y1 = from.y + CARD_H / 2
    const x2 = to.x
    const y2 = to.y + CARD_H / 2
    const mid = x1 + COL_GAP / 2
    links.push({ key: `${from.match.id}-${to.match.id}`, path: `M${x1} ${y1}H${mid}V${y2}H${x2}` })
  }

  const all = [...placed.values()]
  return {
    placed: all,
    links,
    heads,
    width: columnKeys.length * (CARD_W + COL_GAP) - COL_GAP,
    height: all.reduce((max, p) => Math.max(max, p.y + CARD_H), 0),
  }
}
