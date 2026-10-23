export type Format = 'single_elimination' | 'double_elimination' | 'round_robin' | 'group_knockout'
export type Status = 'draft' | 'live' | 'done'
export type Stage = 'winners' | 'losers' | 'grand_final' | 'grand_reset' | 'third_place' | 'league'
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
  forfeit: boolean
  group: number | null
  scheduled_at: number | null
  venue: string | null
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
  third_place: boolean
  groups: number
  advance: number
  status: Status
  created_at: string
  teams: Team[]
  bracket: Bracket | null
  champion: number | null
  progress: { played: number; total: number }
}

export interface ApiToken {
  id: number
  label: string
  created_at: number
}

export type FontChoice = 'sans' | 'serif' | 'mono'

export type Scheme = 'system' | 'light' | 'dark'
export type Density = 'comfortable' | 'compact'
export type Tint = 'neutral' | 'warm' | 'cool'

export interface Appearance {
  site_name: string
  accent_light: string
  accent_dark: string
  radius: number
  font: FontChoice
  scheme: Scheme
  density: Density
  tagline: string
  tint: Tint
  avatars: boolean
  logo: string | null
}

export type EventKind = 'started' | 'reset' | 'result' | 'corrected' | 'forfeit' | 'cleared' | 'scheduled'

export interface LogEvent {
  id: number
  at: number
  kind: EventKind
  match_id: number | null
  data: {
    teams?: number
    team_a?: number | null
    team_b?: number | null
    score_a?: number | null
    score_b?: number | null
    winner?: 'A' | 'B' | null
    forfeit?: boolean
    scheduled_at?: number | null
    venue?: string | null
  }
}
