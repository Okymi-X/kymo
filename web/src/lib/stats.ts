import type { Match, Side, Slot, Stage, Team, Tournament } from '../api/types'

export type TeamStatus = 'champion' | 'active' | 'out' | 'seeded'

export interface TeamRow {
  team: Team
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

export function computeTeamRows(t: Tournament): TeamRow[] {
  const rows = new Map<number, TeamRow>(
    t.teams.map((team) => [team.id, { team, played: 0, wins: 0, draws: 0, losses: 0, points: 0, pointsFor: 0, pointsAgainst: 0, status: 'seeded' as TeamStatus, depth: 0, thirdPlaceWin: false }]),
  )
  const matches = t.bracket?.matches ?? []
  for (const m of matches.filter(decisive)) {
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
      if (t.format === 'round_robin') row.points += won ? 3 : drew ? 1 : 0
      if (m.score) {
        row.pointsFor += m.score[i]
        row.pointsAgainst += m.score[1 - i]
      }
    })
  }
  const lossesToExit = t.format === 'double_elimination' ? 2 : 1
  for (const row of rows.values()) {
    const lostInBracket = matches
      .filter((m) => decisive(m) && m.winner !== null && m.stage !== 'third_place' && m.stage !== 'league')
      .filter((m) => {
        const loser = teamOf(m.slots[sideIndex(m.winner as Side) === 0 ? 1 : 0])
        return loser === row.team.id
      }).length
    if (t.bracket === null) row.status = 'seeded'
    else if (t.champion === row.team.id) row.status = 'champion'
    else if (lostInBracket >= lossesToExit) row.status = 'out'
    else row.status = 'active'
  }
  const order: Record<TeamStatus, number> = { champion: 0, active: 1, seeded: 1, out: 2 }
  const diff = (r: TeamRow) => r.pointsFor - r.pointsAgainst
  const compare =
    t.format === 'round_robin'
      ? (a: TeamRow, b: TeamRow) => b.points - a.points || diff(b) - diff(a) || b.pointsFor - a.pointsFor || a.team.id - b.team.id
      : (a: TeamRow, b: TeamRow) =>
          b.depth - a.depth ||
          b.wins - a.wins ||
          Number(b.thirdPlaceWin) - Number(a.thirdPlaceWin) ||
          diff(b) - diff(a) ||
          a.team.seed - b.team.seed
  return [...rows.values()].sort((a, b) => order[a.status] - order[b.status] || compare(a, b))
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
