import type { Match, Side, Slot, Stage, Team, Tournament } from '../api/types'

export type TeamStatus = 'champion' | 'active' | 'out' | 'seeded'

export interface TeamRow {
  team: Team
  /** Group index for groups then knockout, otherwise null. */
  group: number | null
  played: number
  wins: number
  draws: number
  losses: number
  /** League points, 3 per win and 1 per draw. Zero in knockout formats. */
  points: number
  pointsFor: number
  pointsAgainst: number
  status: TeamStatus
  /** Matches played in the main bracket, the third place match excluded. Deeper runs rank higher. */
  depth: number
  thirdPlaceWin: boolean
}

export interface HistoryEntry {
  match: Match
  opponent: number | null
  result: 'win' | 'loss' | 'draw' | 'next' | 'waiting'
  own: number | null
  other: number | null
  forfeit: boolean
}

export type MatchState = 'ready' | 'waiting' | 'played'

const teamOf = (slot: Slot): number | null => (slot.kind === 'team' ? slot.team : null)
const sideIndex = (side: Side) => (side === 'A' ? 0 : 1)

/** A match people play: no bye, and not a reset that was never needed. */
export function isRealMatch(m: Match): boolean {
  return !m.slots.some((s) => s.kind === 'bye')
}

export function matchState(m: Match): MatchState {
  if (m.score !== null || m.forfeit) return 'played'
  return m.slots.every((s) => s.kind === 'team') ? 'ready' : 'waiting'
}

const decisive = (m: Match) => m.slots.every((s) => s.kind === 'team') && (m.score !== null || m.forfeit)

function tally(t: Tournament, keep: (m: Match) => boolean): Map<number, TeamRow> {
  const rows = new Map<number, TeamRow>()
  for (const team of t.teams) {
    rows.set(team.id, { team, group: null, played: 0, wins: 0, draws: 0, losses: 0, points: 0, pointsFor: 0, pointsAgainst: 0, status: 'seeded', depth: 0, thirdPlaceWin: false })
  }
  for (const m of (t.bracket?.matches ?? []).filter((x) => decisive(x) && keep(x))) {
    const winner = m.winner
    const ids = [teamOf(m.slots[0]), teamOf(m.slots[1])]
    ids.forEach((id, i) => {
      const row = id === null ? undefined : rows.get(id)
      if (!row) return
      const won = winner !== null && sideIndex(winner) === i
      const drew = winner === null
      row.played += 1
      if (m.stage === 'third_place') row.thirdPlaceWin = row.thirdPlaceWin || won
      else if (m.stage !== 'league') row.depth += 1
      row.wins += won ? 1 : 0
      row.draws += drew ? 1 : 0
      row.losses += won || drew ? 0 : 1
      if (m.stage === 'league') row.points += won ? 3 : drew ? 1 : 0
      if (m.score) {
        row.pointsFor += m.score[i]
        row.pointsAgainst += m.score[1 - i]
      }
    })
  }
  for (const m of t.bracket?.matches ?? []) {
    if (m.stage !== 'league' || m.group === null) continue
    for (const slot of m.slots) {
      const row = slot.kind === 'team' ? rows.get(slot.team) : undefined
      if (row) row.group = m.group
    }
  }
  return rows
}

const diffOf = (r: TeamRow) => r.pointsFor - r.pointsAgainst

/** Same order as the engine: points, difference, goals, then the organizer's tiebreak list, then id. */
const leagueOrderFor = (t: Tournament) => {
  const rank = (id: number) => {
    const i = (t.bracket?.tiebreak ?? []).indexOf(id)
    return i === -1 ? Number.MAX_SAFE_INTEGER : i
  }
  return (a: TeamRow, b: TeamRow) =>
    b.points - a.points || diffOf(b) - diffOf(a) || b.pointsFor - a.pointsFor || rank(a.team.id) - rank(b.team.id) || a.team.id - b.team.id
}

/** Teams level on points, difference and goals, with at least one match played. Each set has two or more teams. */
export function tiedSets(rows: TeamRow[]): TeamRow[][] {
  const sets = new Map<string, TeamRow[]>()
  for (const r of rows) {
    if (r.played === 0) continue
    const key = `${r.points}:${diffOf(r)}:${r.pointsFor}`
    sets.set(key, [...(sets.get(key) ?? []), r])
  }
  return [...sets.values()].filter((set) => set.length > 1)
}

const knockoutOrder = (a: TeamRow, b: TeamRow) =>
  b.depth - a.depth ||
  b.wins - a.wins ||
  Number(b.thirdPlaceWin) - Number(a.thirdPlaceWin) ||
  diffOf(b) - diffOf(a) ||
  a.team.seed - b.team.seed

function statusOf(t: Tournament, row: TeamRow): TeamStatus {
  const matches = t.bracket?.matches ?? []
  if (t.bracket === null) return 'seeded'
  if (t.champion === row.team.id) return 'champion'
  const knockout = matches.filter((m) => m.stage !== 'league')
  if (t.format === 'group_knockout') {
    const groupsDone = matches.filter((m) => m.stage === 'league').every((m) => m.score !== null || m.forfeit)
    const qualified = knockout.some((m) => m.slots.some((s) => s.kind === 'team' && s.team === row.team.id))
    if (groupsDone && !qualified) return 'out'
  }
  const lossesToExit = t.format === 'double_elimination' ? 2 : 1
  const lost = knockout.filter((m) => decisive(m) && m.winner !== null && m.stage !== 'third_place').filter((m) => {
    const loser = teamOf(m.slots[sideIndex(m.winner as Side) === 0 ? 1 : 0])
    return loser === row.team.id
  }).length
  if (t.format !== 'round_robin' && lost >= lossesToExit) return 'out'
  return 'active'
}

export function computeTeamRows(t: Tournament): TeamRow[] {
  const rows = tally(t, () => true)
  for (const row of rows.values()) row.status = statusOf(t, row)
  const order: Record<TeamStatus, number> = { champion: 0, active: 1, seeded: 1, out: 2 }
  const compare = t.format === 'round_robin' ? leagueOrderFor(t) : knockoutOrder
  return [...rows.values()].sort((a, b) => order[a.status] - order[b.status] || compare(a, b))
}

export interface GroupTable {
  group: number
  rows: TeamRow[]
}

/** Group stage tables, counting only league matches inside each group. */
export function computeGroupTables(t: Tournament): GroupTable[] {
  const overall = new Map(computeTeamRows(t).map((r) => [r.team.id, r.status]))
  const rows = tally(t, (m) => m.stage === 'league')
  const groups = new Map<number, TeamRow[]>()
  for (const row of rows.values()) {
    if (row.group === null) continue
    row.status = overall.get(row.team.id) ?? 'active'
    groups.set(row.group, [...(groups.get(row.group) ?? []), row])
  }
  return [...groups.entries()].sort((a, b) => a[0] - b[0]).map(([group, list]) => ({ group, rows: list.sort(leagueOrderFor(t)) }))
}

export function teamHistory(t: Tournament, teamId: number): HistoryEntry[] {
  const out: HistoryEntry[] = []
  for (const m of t.bracket?.matches ?? []) {
    if (!isRealMatch(m)) continue
    const index = m.slots.findIndex((s) => teamOf(s) === teamId)
    if (index === -1) continue
    const opponent = teamOf(m.slots[1 - index])
    const state = matchState(m)
    const won = m.winner !== null && sideIndex(m.winner) === index
    const played = state === 'played'
    out.push({
      match: m,
      opponent,
      result: played ? (m.winner === null ? 'draw' : won ? 'win' : 'loss') : state === 'ready' ? 'next' : 'waiting',
      own: m.score ? m.score[index] : null,
      other: m.score ? m.score[1 - index] : null,
      forfeit: m.forfeit,
    })
  }
  return out
}

const csvCell = (value: string | number) => {
  const text = String(value)
  // Leading = + - @ would run as a formula in spreadsheets.
  const safe = /^[=+\-@]/.test(text) ? `'${text}` : text
  return /[",\n]/.test(safe) ? `"${safe.replace(/"/g, '""')}"` : safe
}

export function standingsCsv(rows: TeamRow[]): string {
  const head = ['Rank', 'Team', 'Seed', 'Qualifier score', 'Played', 'Wins', 'Draws', 'Losses', 'Points for', 'Points against', 'League points', 'Status']
  const lines = rows.map((r, i) =>
    [i + 1, r.team.name, r.team.seed, r.team.score, r.played, r.wins, r.draws, r.losses, r.pointsFor, r.pointsAgainst, r.points, r.status].map(csvCell).join(','),
  )
  return [head.join(','), ...lines].join('\n')
}

export function resultsCsv(t: Tournament, label: (m: Match) => string): string {
  const names = new Map(t.teams.map((x) => [x.id, x.name]))
  const name = (s: Slot) => (s.kind === 'team' ? (names.get(s.team) ?? '') : s.kind === 'bye' ? 'Bye' : 'TBD')
  const head = ['Round', 'Team A', 'Team B', 'Score A', 'Score B', 'Winner', 'Forfeit']
  const lines = (t.bracket?.matches ?? []).filter(isRealMatch).map((m) =>
    [
      label(m),
      name(m.slots[0]),
      name(m.slots[1]),
      m.score?.[0] ?? '',
      m.score?.[1] ?? '',
      m.winner === null ? '' : name(m.slots[sideIndex(m.winner)]),
      m.forfeit ? 'yes' : '',
    ]
      .map(csvCell)
      .join(','),
  )
  return [head.join(','), ...lines].join('\n')
}

export const stageOrder: Record<Stage, number> = { winners: 0, losers: 1, grand_final: 2, grand_reset: 3, third_place: 4, league: 5 }
