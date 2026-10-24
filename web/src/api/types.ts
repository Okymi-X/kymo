export type Format = 'single_elimination' | 'double_elimination' | 'round_robin' | 'group_knockout' | 'swiss'
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
  /** Games of a best of series as [a, b] scores. Empty for a single game. */
  games: [number, number][]
  /** Unix seconds when the organizer marked the match as being played now. */
  started_at: number | null
  group: number | null
  scheduled_at: number | null
  venue: string | null
  next_win: Dest | null
}

export interface Bracket {
  format: Format
  matches: Match[]
  /** Swiss only: rounds the event plays. */
  swiss_rounds?: number
  /** Organizer decided order for teams level on points, difference and goals. */
  tiebreak: number[]
  /** Games per match: 1, 3, 5 or 7. */
  best_of: number
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
  /** Swiss rounds, zero when the usual count is used. */
  rounds: number
  /** Games per match: 1, 3, 5 or 7. */
  best_of: number
  /** Rules or notes from the organizer, shown under the title. */
  description: string
  /** Optional #rrggbb accent for this tournament's pages. */
  accent: string | null
  status: Status
  created_at: string
  teams: Team[]
  /** Full roster size. On the public list `teams` holds only the first few. */
  team_count: number
  bracket: Bracket | null
  champion: number | null
  progress: { played: number; total: number }
}

/** A chat or automation endpoint. Only the host is known to the browser, the full URL is a secret. */
export interface Webhook {
  id: number
  label: string
  host: string
}

/** What a team captain sees on their private reporting link. */
export interface ReportInfo {
  tournament: string
  slug: string
  team: string
  match_id: number | null
  opponent: string | null
  round: number | null
  /** The score already submitted for the current match, own then opponent. */
  reported: [number, number] | null
}

/** A captain's score waiting for approval. Scores are in the match's own order, side A then side B. */
export interface PendingReport {
  id: number
  tournament_id: number
  tournament: string
  match_id: number
  team: string
  score_a: number
  score_b: number
  created_at: number
}

/** One team's totals across every tournament: 3 per win, 1 per draw, 10 per title. */
export interface LadderLine {
  name: string
  tournaments: number
  played: number
  wins: number
  draws: number
  titles: number
  points: number
}

export interface ApiToken {
  id: number
  label: string
  created_at: number
  expires_at: number | null
  /** A referee token only records results and schedules for the tournaments in `scope`. */
  referee: boolean
  scope: number[] | null
}

export type FontChoice = 'sans' | 'serif' | 'mono'

export type Scheme = 'system' | 'light' | 'dark'
export type Density = 'comfortable' | 'compact'
export type LineStyle = 'square' | 'curved'
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
  lines: LineStyle
}

export type EventKind = 'started' | 'reset' | 'result' | 'corrected' | 'forfeit' | 'cleared' | 'scheduled' | 'live'

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
