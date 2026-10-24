import { describe, expect, it } from 'vitest'
import type { Match, Slot, Tournament } from '../api/types'
import { computeGroupTables, computeTeamRows, matchState, resultsCsv, standingsCsv, teamHistory, tiedSets } from './stats'

const team = (id: number): Slot => ({ kind: 'team', team: id })
const empty: Slot = { kind: 'empty' }

const match = (id: number, a: Slot, b: Slot, score: [number, number] | null, winner: 'A' | 'B' | null, extra: Partial<Match> = {}): Match => ({
  id,
  stage: 'winners',
  round: 1,
  position: id,
  slots: [a, b],
  score,
  winner,
  forfeit: false,
  games: [],
  group: null,
  scheduled_at: null,
  venue: null,
  next_win: null,
  ...extra,
})

const tournament = (matches: Match[], over: Partial<Tournament> = {}): Tournament => ({
  id: 1,
  name: 'T',
  slug: 't',
  format: 'single_elimination',
  third_place: false,
  groups: 2,
  advance: 2,
  rounds: 0,
  best_of: 1,
  accent: null,
  status: 'live',
  created_at: '',
  teams: [1, 2, 3, 4].map((id) => ({ id, name: `Team ${id}`, seed: id, score: 100 - id })),
  team_count: 4,
  bracket: { format: 'single_elimination', matches, tiebreak: [], best_of: 1 },
  champion: null,
  progress: { played: 0, total: 0 },
  ...over,
})

describe('computeTeamRows', () => {
  const t = tournament([
    match(0, team(1), team(4), [3, 1], 'A'),
    match(1, team(2), team(3), [0, 2], 'B'),
    match(2, team(1), team(3), null, null),
  ])
  const rows = computeTeamRows(t)
  const row = (id: number) => rows.find((r) => r.team.id === id)!

  it('counts wins, losses and points', () => {
    expect(row(1)).toMatchObject({ played: 1, wins: 1, losses: 0, pointsFor: 3, pointsAgainst: 1 })
    expect(row(2)).toMatchObject({ played: 1, wins: 0, losses: 1, pointsFor: 0, pointsAgainst: 2 })
  })

  it('marks single elimination losers as out and winners as active', () => {
    expect(row(4).status).toBe('out')
    expect(row(2).status).toBe('out')
    expect(row(1).status).toBe('active')
  })

  it('lists active teams before eliminated ones', () => {
    expect(rows.map((r) => r.status).slice(0, 2)).toEqual(['active', 'active'])
    expect(rows.at(-1)!.status).toBe('out')
  })

  it('needs two losses in double elimination', () => {
    const d = tournament([match(0, team(1), team(2), [0, 1], 'B')], { format: 'double_elimination' })
    expect(computeTeamRows(d).find((r) => r.team.id === 1)!.status).toBe('active')
  })

  it('does not eliminate a team for losing the third place match', () => {
    const third = match(0, team(3), team(4), [0, 1], 'B', { stage: 'third_place' })
    expect(computeTeamRows(tournament([third])).find((r) => r.team.id === 3)!.status).toBe('active')
  })

  it('marks the champion and ignores forfeits for points', () => {
    const f = tournament([match(0, team(1), team(2), null, 'A', { forfeit: true })], { champion: 1 })
    const r = computeTeamRows(f)
    expect(r[0]).toMatchObject({ status: 'champion', wins: 1, pointsFor: 0 })
  })

  it('shows seeded before a tournament starts', () => {
    const draft = tournament([], { bracket: null })
    expect(computeTeamRows(draft).every((r) => r.status === 'seeded')).toBe(true)
  })
})

describe('ranking after a third place match', () => {
  it('keeps the runner-up above the third place winner', () => {
    const t = tournament(
      [
        match(0, team(1), team(4), [1, 0], 'A'),
        match(1, team(2), team(3), [1, 0], 'A'),
        match(2, team(2), team(1), [100, 0], 'A'),
        match(3, team(3), team(4), [100, 0], 'A', { stage: 'third_place' }),
      ],
      { champion: 2, status: 'done' },
    )
    expect(computeTeamRows(t).map((r) => r.team.id)).toEqual([2, 1, 3, 4])
  })
})

describe('round robin table', () => {
  const league = (matches: Match[]) =>
    tournament(matches, { format: 'round_robin', bracket: { format: 'round_robin', matches, tiebreak: [], best_of: 1 } })
  const lm = (id: number, a: number, b: number, score: [number, number] | null, winner: 'A' | 'B' | null) =>
    match(id, team(a), team(b), score, winner, { stage: 'league' })

  it('awards 3 points for a win and 1 for a draw', () => {
    const rows = computeTeamRows(league([lm(0, 1, 2, [2, 2], null), lm(1, 1, 3, [1, 0], 'A'), lm(2, 2, 3, [0, 0], null)]))
    const row = (id: number) => rows.find((r) => r.team.id === id)!
    expect(row(1)).toMatchObject({ points: 4, wins: 1, draws: 1, losses: 0 })
    expect(row(2)).toMatchObject({ points: 2, draws: 2 })
    expect(row(3)).toMatchObject({ points: 1, losses: 1, draws: 1 })
  })

  it('orders by points then goal difference and never marks a team out', () => {
    const rows = computeTeamRows(league([lm(0, 1, 2, [3, 0], 'A'), lm(1, 3, 4, [1, 0], 'A')]))
    expect(rows.map((r) => r.team.id).slice(0, 2)).toEqual([1, 3])
    expect(rows.every((r) => r.status !== 'out')).toBe(true)
  })

  it('finds teams that are level and applies the organizer tiebreak order', () => {
    const draws = [lm(0, 1, 2, [1, 1], null), lm(1, 1, 3, [1, 1], null), lm(2, 2, 3, [1, 1], null)]
    const plain = league(draws)
    expect(computeTeamRows(plain).map((r) => r.team.id).slice(0, 3)).toEqual([1, 2, 3])
    expect(tiedSets(computeTeamRows(plain))).toHaveLength(1)
    const decided = { ...plain, bracket: { ...plain.bracket!, tiebreak: [3, 2] } }
    expect(computeTeamRows(decided).map((r) => r.team.id).slice(0, 3)).toEqual([3, 2, 1])
  })

  it('ranks equal points by goal difference, not by games played', () => {
    const rows = computeTeamRows(league([lm(0, 1, 2, [3, 0], 'A'), lm(1, 3, 4, [1, 0], 'A'), lm(2, 3, 5, [0, 1], 'B')]))
    const order = rows.map((r) => r.team.id)
    const [one, three] = [rows.find((r) => r.team.id === 1)!, rows.find((r) => r.team.id === 3)!]
    expect([one.points, three.points]).toEqual([3, 3])
    expect(three.played).toBeGreaterThan(one.played)
    expect(order.indexOf(1)).toBeLessThan(order.indexOf(3))
  })

  it('treats a scored draw as played and reports it in a team path', () => {
    const t = league([lm(0, 1, 2, [1, 1], null)])
    expect(matchState(t.bracket!.matches[0])).toBe('played')
    expect(teamHistory(t, 1)[0].result).toBe('draw')
  })
})

describe('matchState and teamHistory', () => {
  it('distinguishes ready, waiting and played', () => {
    expect(matchState(match(0, team(1), team(2), null, null))).toBe('ready')
    expect(matchState(match(0, team(1), empty, null, null))).toBe('waiting')
    expect(matchState(match(0, team(1), team(2), [1, 0], 'A'))).toBe('played')
  })

  it('builds a team path with opponents and results', () => {
    const t = tournament([match(0, team(1), team(4), [3, 1], 'A'), match(2, team(1), empty, null, null)])
    const history = teamHistory(t, 1)
    expect(history.map((h) => h.result)).toEqual(['win', 'waiting'])
    expect(history[0]).toMatchObject({ opponent: 4, own: 3, other: 1 })
  })

  it('skips byes', () => {
    const t = tournament([match(0, team(1), { kind: 'bye' }, null, 'A')])
    expect(teamHistory(t, 1)).toEqual([])
  })
})

describe('csv export', () => {
  it('quotes commas and neutralizes spreadsheet formulas', () => {
    const t = tournament([], { teams: [{ id: 1, name: '=cmd, "x"', seed: 1, score: 5 }] })
    const csv = standingsCsv(computeTeamRows(t))
    expect(csv.split('\n')[1]).toContain('"\'=cmd, ""x"""')
  })

  it('writes one line per real match', () => {
    const t = tournament([match(0, team(1), team(2), [2, 1], 'A'), match(1, team(3), { kind: 'bye' }, null, 'A')])
    const lines = resultsCsv(t, () => 'Round 1').split('\n')
    expect(lines).toHaveLength(2)
    expect(lines[1]).toBe('Round 1,Team 1,Team 2,2,1,Team 1,')
  })
})

describe('groups then knockout', () => {
  const gm = (id: number, a: number, b: number, score: [number, number] | null, winner: 'A' | 'B' | null, group: number) =>
    match(id, team(a), team(b), score, winner, { stage: 'league', group })
  const build = (matches: Match[], over: Partial<Tournament> = {}) =>
    tournament(matches, { format: 'group_knockout', bracket: { format: 'group_knockout', matches, tiebreak: [], best_of: 1 }, ...over })

  it('keeps one table per group and counts only group matches', () => {
    const tables = computeGroupTables(
      build([
        gm(0, 1, 2, [2, 0], 'A', 0),
        gm(1, 3, 4, [1, 1], null, 1),
        match(2, team(1), team(3), [5, 0], 'A', { stage: 'winners' }),
      ]),
    )
    expect(tables.map((x) => x.group)).toEqual([0, 1])
    expect(tables[0].rows[0]).toMatchObject({ points: 3, played: 1 })
    expect(tables[1].rows.map((r) => r.points)).toEqual([1, 1])
    expect(tables[0].rows[0].pointsFor).toBe(2)
  })

  it('marks teams that missed the knockout as out once the groups are done', () => {
    const t = build([
      gm(0, 1, 2, [2, 0], 'A', 0),
      gm(1, 3, 4, [1, 0], 'A', 1),
      match(2, team(1), team(3), null, null, { stage: 'winners' }),
    ])
    const status = (id: number) => computeTeamRows(t).find((r) => r.team.id === id)!.status
    expect(status(1)).toBe('active')
    expect(status(3)).toBe('active')
    expect(status(2)).toBe('out')
    expect(status(4)).toBe('out')
  })

  it('does not eliminate anyone while group matches are still open', () => {
    const t = build([gm(0, 1, 2, [2, 0], 'A', 0), gm(1, 1, 3, null, null, 0), match(2, empty, empty, null, null, { stage: 'winners' })])
    expect(computeTeamRows(t).every((r) => r.status !== 'out')).toBe(true)
  })
})
