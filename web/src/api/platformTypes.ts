// Shapes of the platform API (events, accounts, teams, Jeopardy, instances, operations).
// Mirrored by hand from the server, like types.ts.

export type Role = 'visitor' | 'player' | 'referee' | 'monitor' | 'admin'
export type Permission =
  | 'view_public'
  | 'view_scoreboard_public'
  | 'play_event'
  | 'manage_team'
  | 'submit_flag'
  | 'start_instance'
  | 'referee'
  | 'manage_events'
  | 'manage_challenges'
  | 'manage_users'
  | 'manage_settings'
  | 'preview_as_player'
  | 'monitor'
  | 'moderate_bans'
  | 'view_audit'
  | 'view_hidden_scoreboard'
  | 'manage_permissions'

/** One role's row in the permission matrix. A locked role cannot be edited; forced permissions
 *  cannot be switched off for it. */
export interface RolePermissions {
  role: Role
  locked: boolean
  granted: Permission[]
  defaults: Permission[]
  forced: Permission[]
}

export interface PermissionMatrix {
  permissions: Permission[]
  roles: RolePermissions[]
}

export interface MyPermissions {
  role: Role
  permissions: Permission[]
}

export type RegistrationPolicy = 'at_start' | 'at_time' | 'manual'
export type EventState = 'draft' | 'registration_open' | 'live' | 'paused' | 'ended' | 'archived'
export type Verdict = 'correct' | 'incorrect' | 'bad_format'

export interface User {
  id: number
  username: string
  email: string | null
  role: Role
  banned: boolean
  display_name: string
  bio: string
  created_at: number
}

export interface Me extends User {
  team_id: number | null
}

export interface SessionOut {
  token: string
  user: User
}

export interface EventInfo {
  id: number
  kind: 'tournament' | 'jeopardy'
  mode: 'solo' | 'team'
  name: string
  slug: string
  state: EventState
  effective_state: EventState
  registration_opens: number | null
  registration_closes: number | null
  starts_at: number | null
  ends_at: number | null
  ended_at: number | null
  scoreboard_public: boolean
  description: string
  team_min: number
  team_max: number
  allow_invites: boolean
  roster_locked: boolean
  /** When registration stops: with the start, at a closing time, or when an organizer closes it. */
  registration_policy: RegistrationPolicy
  /** The organizer's own switch: true ends registration under every policy. */
  registration_closed: boolean
  /** The server's answer, by the same rule the join button uses. */
  registration_open: boolean
  registration_note: string | null
  /** When the event actually went live. */
  started_at: number | null
  /** True when an organizer started it before its scheduled start. */
  started_early: boolean
  /** Where instances are reached: a published host port, or a private lab through the player's VPN. */
  network_mode: 'published' | 'vpn'
  /** The lab host running this event's instances; null is this server's own runtime. */
  lab_host_id: number | null
  created_at: number
  /** Only on the single event response. */
  registered?: boolean
}

export interface Category {
  id: number
  event_id: number
  name: string
  position: number
}

export interface PlayerHint {
  id: number
  cost: number
  unlocked: boolean
  content: string | null
}

export interface PlayerAttachment {
  id: number
  name: string
  mime: string
  size: number
  sha256: string
  url: string
}

export interface PlayerChallenge {
  id: number
  category_id: number
  title: string
  description: string
  points: number
  solved: boolean
  solved_at: number | null
  solved_by: number
  status: 'open' | 'closed'
  release_at: number | null
  retire_at: number | null
  flag_format: { prefix: string; suffix: string }
  max_attempts: number | null
  attempts_remaining: number | null
  requires_instance: boolean
  /** A Responder: players fix the machine and a Verifier gives the flag. */
  responder?: boolean
  /** Flags in all, and how many this participant found. More than one only for a multi flag challenge. */
  flags_total?: number
  flags_found?: number
  /** Where its instance runs: which VPN profile it needs. */
  lab_host_id: number | null
  /** `flag` (also instance backed) or `dfir`, answered question by question. */
  kind: 'flag' | 'dfir'
  items: { id: number; question: string; points: number; solved: boolean; attempts_remaining: number | null }[]
  hints: PlayerHint[]
  attachments: PlayerAttachment[]
}

export interface ChallengeList {
  categories: Category[]
  challenges: PlayerChallenge[]
}

export interface SubmitResult {
  verdict: Verdict
  correct: boolean
  points: number | null
  attempts_remaining: number | null
  /** Only for a challenge with several flags. */
  flags_found?: number
  flags_total?: number
}

export interface HintUnlock {
  hint_id: number
  cost: number
  content: string
  newly_unlocked: boolean
}

export type InstanceStateName = 'none' | 'starting' | 'running' | 'stopped' | 'expired' | 'failed'

export interface InstanceEndpoint {
  /** `machine`, `verifier` or `service`. */
  role: string
  name: string
  host: string
  port: number | null
  /** `http` for a web page. */
  scheme?: string | null
}

export interface InstanceView {
  id?: number
  state: InstanceStateName
  /** `vpn` means the address is private and needs the player's VPN. */
  network?: 'vpn' | 'published'
  /** True once the service answers. The address is only sent then. */
  ready?: boolean
  /** A machine on the private network: every port is open on the address, so there is no port to show. */
  machine?: boolean
  /** Why the last run failed, when it did. */
  failure?: 'start_failed' | 'never_ready' | 'exited' | null
  host?: string | null
  port?: number | null
  /** The other published services of a machine that offers several, by name. */
  services?: { name: string; port: number }[]
  /** Where the machine and, for a Responder, its Verifier can be reached. */
  endpoints?: InstanceEndpoint[]
  started_at?: number
  expires_at?: number
  remaining_secs?: number
  starts_used: number
  starts_max: number
}

export interface BoardRow {
  rank: number
  participant_id: number
  kind: 'user' | 'team'
  name: string
  avatar_seed: string
  points: number
  solves: number
  last_solve_at: number | null
  solved?: number[] | null
}

export interface Scoreboard {
  event_id: number
  mode: string
  public: boolean
  rows: BoardRow[]
  challenges?: { id: number; title: string; category_id: number }[] | null
  /** Set while the board is frozen: the moment it shows. */
  frozen_at?: number | null
}

export interface SolveLine {
  challenge_id: number
  title: string
  at: number
  hints_cost: number
  points: number
}

export interface Progress {
  participant_id: number
  kind: 'user' | 'team'
  name: string
  rank: number | null
  points: number
  solved_count: number
  total_challenges: number
  solves: SolveLine[]
}

export interface MemberProgress {
  user_id: number
  username: string
  display_name: string
  points: number
  solves: { challenge_id: number; title: string; at: number; points: number }[]
}

export interface TeamProgress {
  team: Progress
  members: MemberProgress[]
}

export interface Member {
  user_id: number
  username: string
  display_name: string
  role: 'owner' | 'member'
  joined_at: number
}

export interface Team {
  id: number
  name: string
  owner_user_id: number
  invite_code: string | null
  created_at: number
  members: Member[]
}

// ---------- administration ----------

export type Scoring =
  | { kind: 'static'; points: number }
  | { kind: 'dynamic'; initial: number; minimum: number; decay: number; curve: 'linear' | 'parabolic' | 'logarithmic' }

export type FlagSpec = { kind: 'static'; value: string } | { kind: 'regex'; pattern: string } | { kind: 'per_team'; expected: string }

export interface InstanceConfig {
  image: string
  port: number
  cpu_millis: number
  memory_mb: number
  pids: number
  ttl_secs: number
  env?: [string, string][]
  read_only?: boolean
}

export interface AdminHint {
  id: number
  challenge_id: number
  content: string
  cost: number
  position: number
}

export interface AdminAttachment {
  id: number
  challenge_id: number
  name: string
  mime: string
  size: number
  sha256: string
}

export interface AdminFlag {
  id: number
  challenge_id: number
  position: number
  name: string
  /** Seen by organizers only. */
  label: string
  /** The whole flag when static, the prefix when dynamic. */
  value: string
  random_len: number
  closing: string
  weight: number
}

export interface FlagInput {
  name: string
  label?: string
  value: string
  random_len?: number
  closing?: string
  weight?: number
}

export interface AdminChallenge {
  id: number
  event_id: number
  category_id: number
  title: string
  description: string
  scoring: Scoring
  flag: FlagSpec
  flag_prefix: string
  flag_suffix: string
  case_sensitive: boolean
  max_attempts: number | null
  state: 'draft' | 'published' | 'retired'
  release_at: number | null
  retire_at: number | null
  position: number
  requires_instance: boolean
  instance_config: InstanceConfig | null
  /** `flag` (also instance backed) or `dfir` (question items, tournament events only). */
  kind: 'flag' | 'dfir'
  created_at: number
  /** `static` (the flag above) or `dynamic` (generated for every instance run). */
  flag_mode: 'static' | 'dynamic'
  multi_flag: boolean
  responder: boolean
  /** The flags of a dynamic or multi flag challenge; empty for a plain static flag. */
  flags: AdminFlag[]
  lab_host_id: number | null
  solves: number
  hints: AdminHint[]
  attachments: AdminAttachment[]
}

export type ChallengeBody = Omit<AdminChallenge, 'id' | 'event_id' | 'created_at' | 'lab_host_id' | 'solves' | 'hints' | 'attachments' | 'state' | 'kind' | 'flag' | 'flag_mode' | 'multi_flag' | 'responder' | 'flags'> & {
  flag?: FlagSpec
  kind?: 'flag' | 'dfir'
  flag_mode?: 'static' | 'dynamic'
  responder?: boolean
  flags?: FlagInput[]
}

export interface JeopardySettings {
  tie_breaks: ('earliest_last_solve' | 'most_solves' | 'first_blood')[]
  rate_max_attempts: number
  rate_window_secs: number
  rate_lockout_secs: number
  /** From this moment the public scoreboard stops moving until the event ends; null means no freeze. */
  freeze_at: number | null
}

export interface EventBody {
  name: string
  kind: 'tournament' | 'jeopardy'
  mode: 'solo' | 'team'
  description?: string
  registration_opens?: number | null
  registration_closes?: number | null
  starts_at?: number | null
  ends_at?: number | null
  scoreboard_public?: boolean
  team_min?: number
  team_max?: number
  allow_invites?: boolean
  roster_locked?: boolean
  registration_policy?: RegistrationPolicy
}

export interface Participant {
  id: number
  user_id: number | null
  team_id: number | null
  name: string
  members: number
  registered_at: number
}

export interface Page<T> {
  items: T[]
  total: number
  page: number
  per_page: number
}

export interface SubmissionLine {
  id: number
  challenge_id: number
  challenge_title: string
  participant_kind: string
  participant_id: number
  user_id: number
  username: string | null
  ip: string
  /** Empty for a correct flag, see `hidden`. */
  value: string
  hidden: boolean
  verdict: Verdict
  at: number
}

/** One run of a Verifier, as the lab host recorded it. Organizers only. */
export interface VerifyLogEntry {
  at: number
  millis: number
  checks: { id: string; state: 'fixed' | 'failed' | 'broken'; detail: string }[]
}

export interface AdminInstance {
  id: number
  challenge_id: number
  challenge_title: string | null
  participant: { kind: string; id: number; team: string | null }
  user_id: number
  username: string | null
  state: InstanceStateName
  host_port: number | null
  started_at: number
  expires_at: number
  stopped_at: number | null
}

// ---------- monitoring and operations ----------

export interface LogRow {
  id: number
  at: number
  method: string
  route: string
  path: string
  status: number
  ip: string
  user_id: number | null
  username: string | null
  ms: number
}

export interface LogPage {
  total: number
  page: number
  per_page: number
  requests: LogRow[]
}

export interface Count {
  key: string
  count: number
}

export interface MonitorSummary {
  since: number
  total: number
  server_errors: number
  throttled: number
  forbidden: number
  top_ips: Count[]
  top_routes: Count[]
  slowest_routes: Count[]
}

export interface RateRule {
  id: number
  endpoint: string
  max_requests: number
  window_secs: number
  scope: 'ip' | 'user'
  strikes_to_ban: number | null
  ban_secs: number
  enabled: boolean
  created_at: number
}

export type RateRuleBody = Omit<RateRule, 'id' | 'created_at'>

export interface Ban {
  id: number
  kind: 'ip' | 'user'
  subject: string
  reason: string
  auto: boolean
  created_by: number | null
  created_at: number
  expires_at: number | null
  lifted_at: number | null
  lifted_by: number | null
  active: boolean
}

export interface AuditEntry {
  id: number
  at: number
  actor_user_id: number | null
  actor_name: string | null
  actor_role: string
  action: string
  target_kind: string
  target_id: string
  detail: string
  /** The address the action came from, when there was a request. */
  ip: string | null
}

export interface AuditPage {
  /** Every recorded action with how often it happened, for the filter. */
  actions: [string, number][]
  total: number
  page: number
  per_page: number
  entries: AuditEntry[]
}

export interface RunInfo {
  at: number
  detail: string
  error: string | null
}

export interface Health {
  status: 'ok' | 'degraded'
  time: number
  version: string
  uptime_secs: number
  database: { ok: boolean; ping_ms: number; size_bytes: number }
  traffic: { window_secs: number; requests: number; server_errors: number; throttled: number; avg_ms: number }
  instances: { enabled: boolean; runtime_ok: boolean | null; live: number; expired_pending: number; orphans: number }
  labs?: { id: number; name: string; enabled: boolean; reachable: boolean | null; last_checked_at: number | null; last_error: string | null; live_instances: number; live_events?: number }[]
  backups: { configured: boolean }
  vpn?: { enabled: boolean; gateway_ok?: boolean; labs?: number; clients?: number; error?: string | null }
  jobs: { reaper: RunInfo | null; backup: RunInfo | null; maintenance: RunInfo | null }
  events: {
    id: number
    slug: string
    name: string
    kind: string
    state: string
    effective_state: string
    participants: number
    live_instances: number
    submissions_last_10m: number
  }[]
  recent_errors: { at: number; method: string; route: string; status: number; ms: number }[]
}

export interface BackupFile {
  name: string
  size: number
}

export interface BackupListing {
  configured: boolean
  every_secs: number | null
  keep: number | null
  backups: BackupFile[]
}

export interface CleanupReport {
  instances_ended: number
  log_rows: number
  expired_sessions: number
}

// ---------- VPN ----------

/** What the home page shows. In custom mode the organizer's own sanitized HTML and CSS replace the default page. */
export interface HomeContent {
  mode: 'default' | 'custom'
  headline: string
  subheadline: string
  signin_label: string
  signup_label: string
  welcome: string
  continue_label: string
  banner: string
  about_title: string
  about_text: string
  show_events: boolean
  show_scoreboards: boolean
  footer: string
  custom_html: string
  custom_css: string
}

/** An event whose scoreboard anyone may read. */
export interface PublicEvent {
  name: string
  slug: string
  kind: 'jeopardy' | 'tournament'
  mode?: 'solo' | 'team'
  effective_state: string
  starts_at: number | null
  ends_at: number | null
  scoreboard_public?: boolean
}

export interface EventRules {
  title: string
  body: string
}

/** Whether the player's tunnel is up right now. `has_profile` is false until they download one. */
export interface VpnConnection {
  has_profile: boolean
  connected: boolean
}

/** The one instance a player has going, in whichever event it is. */
export interface MyInstance {
  instance: {
    id: number
    challenge_id: number
    title: string
    state: 'starting' | 'running'
    event_slug: string
    remaining_secs: number
  } | null
}

export interface VpnStatus {
  enabled: boolean
  /** False when the gateway for this host cannot be reached or does not exist. */
  available?: boolean
  reason?: string
  host?: string
  port?: number
  has_profile?: boolean
  tunnel_ip?: string | null
  issued_at?: number | null
  expires_at?: number | null
}

export interface AdminVpn {
  enabled: boolean
  /** This server runs a gateway of its own. False when every instance lives on a lab host. */
  local?: boolean
  hosts?: {
    id: number
    name: string
    enabled: boolean
    vpn: { host: string; port: number } | null
    gateway: { running: boolean; networks: string[]; error: string | null }
    labs: number
    clients: number
  }[]
  host?: string
  port?: number
  gateway?: { running: boolean; networks: string[]; error: string | null }
  tunnel?: { net: string; capacity: number; used: number }
  lab_pool?: { net: string; capacity: number; used: number }
  clients?: number
  labs?: {
    id: number
    lab_host_id?: number
    event_id: number
    participant: { kind: string; id: number; name: string | null }
    network: string
    subnet: string
    live_instances: number
    last_used_at: number
  }[]
}

export interface VpnSyncReport {
  labs: number
  clients: number
  allows: number
  disconnected: number
}

/** One of the player's running or starting instances, with the challenge it belongs to. */
export interface InstanceListItem {
  id: number
  challenge_id: number
  title: string
  state: InstanceStateName
  network: 'vpn' | 'published'
  host: string | null
  port: number | null
  machine?: boolean
  services?: { name: string; port: number }[]
  remaining_secs: number
}

export interface ScoreSeries {
  participant_id: number
  name: string
  avatar_seed: string
  rank: number
  points: number
  steps: { at: number; total: number }[]
}

export interface ScoreTimeline {
  event_id: number
  mode: string
  from: number
  to: number
  series: ScoreSeries[]
}

// ---------- lab hosts ----------

export interface LabHost {
  id: number
  name: string
  api_url: string
  fingerprint: string | null
  public_host: string
  enabled: boolean
  created_at: number
  last_checked_at: number | null
  last_ok_at: number | null
  last_error: string | null
  /** The host's own VPN gateway as it last reported itself; null when it has none. */
  vpn: { host: string; port: number; only: boolean } | null
  /** Events set to run their instances here. */
  events: number
  live_instances: number
}

export interface LabHealth {
  name: string
  vpn?: { host: string; port: number; pool: string } | null
  vpn_only?: boolean
  version: string
  api: string
  runtime: string
  mode: 'manifest' | 'open'
  running: number
  max_instances: number
  artifacts: number
  uptime_secs: number
  now: number
  /** What the lab host can run: docker, vm. */
  runtimes?: string[]
  /** How virtual machines run: kvm, or tcg (slow emulation). */
  accel?: 'kvm' | 'tcg' | null
}

export interface LabArtifact {
  artifact: string
  runtime: string
  port: number
  max_ttl_secs: number
  env_allow: string[]
  ports?: { name: string; port: number }[]
  /** False while a virtual machine image is still being unpacked. */
  ready?: boolean
  note?: string | null
}

export interface LabCheck {
  ok: boolean
  ms: number
  error?: string
  health?: LabHealth
  artifacts?: LabArtifact[]
  notes?: string[]
}

export interface LabInstance {
  id: string
  name: string
  instance_id: string
  status: string
  host_port: number
  created_at: number
  expires_at: number
  meta: Record<string, string>
  runtime?: string
  ports?: { name: string; guest: number; host: number }[]
  /** False when the platform has no live run for it: an orphan. */
  known: boolean
}

export interface LabBody {
  name?: string
  api_url?: string
  token?: string
  fingerprint?: string
  public_host?: string
  enabled?: boolean
}

/** One question of a DFIR challenge. */
export interface DfirItem {
  id: number
  question: string
  answer: FlagSpec
  case_sensitive: boolean
  points: number
  max_attempts: number | null
}

export type DfirItemBody = Omit<DfirItem, 'id'>

export interface DuelItem {
  id: number
  question: string
  points: number
  solved: boolean
  attempts_left: number | null
}

export interface DuelChallenge {
  id: number
  title: string
  category: string
  description: string
  /** The accepted shape of a flag; none for DFIR questions. */
  flag_format: { prefix: string; suffix: string } | null
  attachments: { id: number; name: string; size: number }[]
  kind: 'flag' | 'instance' | 'dfir'
  points: number
  /** Points each side has earned on this challenge. */
  earned: [number, number]
  solved: boolean
  attempts_left: number | null
  items: DuelItem[]
}

export interface DuelAnswer {
  id: number
  challenge_id: number
  item_id: number | null
  side_team_id: number
  user_id: number
  verdict: 'correct' | 'incorrect'
  points: number
  value: string
  ip: string
  at: number
}

/** A match decided by challenges, as a player or an organizer sees it. */
export interface MatchView {
  match_id: number
  status: 'waiting' | 'open' | 'needs_decision' | 'final'
  closes_at: number | null
  you: 0 | 1 | null
  sides: [{ id: number; name: string; score: number }, { id: number; name: string; score: number }]
  challenges: DuelChallenge[]
  answers?: DuelAnswer[]
}

export interface AnswerResult {
  correct: boolean
  points: number
  attempts_left: number | null
  finished: boolean
}

/** Someone an organizer can add to an event: an account in a solo event, a team otherwise. */
export interface Candidate {
  id: number
  name: string
  detail: string
}
