export type Format = 'single_elimination' | 'double_elimination'
export type Status = 'draft' | 'live' | 'done'
export type Stage = 'winners' | 'losers' | 'grand_final' | 'grand_reset'
export type Side = 'A' | 'B'
export type Seeding = 'manual' | 'random' | 'score'

export type Slot = { kind: 'empty' } | { kind: 'bye' } | { kind: 'team'; team: number }

export interface Dest {
  id: number
  side: Side
}

export interface Match {
  id: number
  stage: Stage
  round: number
  position: number
  slots: [Slot, Slot]
  score: [number, number] | null
  winner: Side | null
  next_win: Dest | null
}

export interface Bracket {
  format: Format
  matches: Match[]
}

export interface Team {
  id: number
  name: string
  seed: number
  score: number
}

export interface Tournament {
  id: number
  name: string
  slug: string
  format: Format
  status: Status
  created_at: string
  teams: Team[]
  bracket: Bracket | null
  champion: number | null
}
