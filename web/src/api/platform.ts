import { ApiError, request, session } from './client'
import type { ChallengeSettings, Tournament } from './types'
import type {
  AdminChallenge,
  AdminVpn,
  AdminInstance,
  VerifyLogEntry,
  AuditPage,
  Ban,
  BackupFile,
  BackupListing,
  Category,
  ChallengeBody,
  ChallengeList,
  CleanupReport,
  EventBody,
  EventInfo,
  Health,
  HintUnlock,
  InstanceListItem,
  InstanceView,
  JeopardySettings,
  LogPage,
  Me,
  MonitorSummary,
  Page,
  Participant,
  Progress,
  RateRule,
  RateRuleBody,
  Role,
  Scoreboard,
  SessionOut,
  LabBody,
  LabCheck,
  LabHost,
  LabInstance,
  ScoreTimeline,
  SubmissionLine,
  SubmitResult,
  Team,
  TeamProgress,
  User,
  Verdict,
  VpnStatus,
  VpnSyncReport,
  MyPermissions,
  Permission,
  PermissionMatrix,
  AnswerResult,
  DfirItem,
  DfirItemBody,
  MatchView,
  Candidate,
} from './platformTypes'

const BASE = '/api/v1'
const enc = encodeURIComponent

/** Builds `?a=1&b=2` from the defined values only. */
export function query(params: Record<string, string | number | boolean | undefined | null>): string {
  const q = new URLSearchParams()
  for (const [k, v] of Object.entries(params)) if (v !== undefined && v !== null && v !== '') q.set(k, String(v))
  const text = q.toString()
  return text ? `?${text}` : ''
}

/** Fetches a file that needs the session (the server wants a bearer token) and saves it. */
export async function downloadFile(path: string, fallbackName: string, method: 'GET' | 'POST' = 'GET'): Promise<void> {
  const token = session.token()
  const res = await fetch(BASE + path, { method, headers: token ? { authorization: `Bearer ${token}` } : {} })
  if (!res.ok) {
    const data: unknown = await res.json().catch(() => null)
    throw new ApiError(res.status, (data as { error?: string } | null)?.error ?? 'Download failed')
  }
  const disposition = res.headers.get('content-disposition') ?? ''
  const name = /filename="([^"]+)"/.exec(disposition)?.[1] ?? fallbackName
  const url = URL.createObjectURL(await res.blob())
  const link = document.createElement('a')
  link.href = url
  link.download = name
  document.body.append(link)
  link.click()
  link.remove()
  URL.revokeObjectURL(url)
}

/** A virtual machine's console as an image the page can show: fetched with the session, because an
 *  image tag cannot send it. The caller revokes the address when done. */
export async function fetchImage(path: string): Promise<string> {
  const token = session.token()
  const res = await fetch(BASE + path, { headers: token ? { authorization: `Bearer ${token}` } : {} })
  if (!res.ok) {
    const data: unknown = await res.json().catch(() => null)
    throw new ApiError(res.status, (data as { error?: string } | null)?.error ?? 'Could not load the image')
  }
  return URL.createObjectURL(await res.blob())
}

const ev = (slug: string) => `/events/${enc(slug)}`
const ch = (slug: string, id: number) => `${ev(slug)}/challenges/${id}`

export const platform = {
  // accounts
  signUpsOpen: () => request<{ open: boolean; contact: string }>('GET', '/registration'),
  register: (username: string, password: string, email?: string) =>
    request<SessionOut>('POST', '/auth/register', { username, password, ...(email ? { email } : {}) }),
  playerLogin: (username: string, password: string) => request<SessionOut>('POST', '/auth/player-login', { username, password }),
  me: () => request<Me>('GET', '/me'),
  updateMe: (body: { display_name?: string; bio?: string; current_password?: string; new_password?: string }) =>
    request<User>('PUT', '/me', body),

  // events
  events: () => request<EventInfo[]>('GET', '/events'),
  event: (slug: string) => request<EventInfo>('GET', ev(slug)),
  register_for: (slug: string) => request<{ registered: boolean }>('POST', `${ev(slug)}/register`),
  unregister: (slug: string) => request<void>('DELETE', `${ev(slug)}/register`),

  // playing
  challenges: (slug: string) => request<ChallengeList>('GET', `${ev(slug)}/challenges`),
  submitItem: (slug: string, id: number, item: number, flag: string) => request<SubmitResult>('POST', `${ch(slug, id)}/items/${item}/submit`, { flag }),
  submit: (slug: string, id: number, flag: string) => request<SubmitResult>('POST', `${ch(slug, id)}/submit`, { flag }),
  unlockHint: (slug: string, id: number, hint: number) => request<HintUnlock>('POST', `${ch(slug, id)}/hints/${hint}/unlock`),
  download: (slug: string, id: number, attachment: number, name: string) =>
    downloadFile(`${ch(slug, id)}/attachments/${attachment}`, name),
  instance: (slug: string, id: number) => request<InstanceView>('GET', `${ch(slug, id)}/instance`),
  startInstance: (slug: string, id: number) => request<InstanceView>('POST', `${ch(slug, id)}/instance`),
  restartInstance: (slug: string, id: number) => request<InstanceView>('POST', `${ch(slug, id)}/instance/restart`),
  stopInstance: (slug: string, id: number) => request<InstanceView>('DELETE', `${ch(slug, id)}/instance`),
  instances: (slug: string) => request<{ instances: InstanceListItem[] }>('GET', `${ev(slug)}/instances`),
  scoreTimeline: (slug: string, top: number) => request<ScoreTimeline>('GET', `${ev(slug)}/scoreboard/timeline?top=${top}`),
  scoreboard: (slug: string) => request<Scoreboard>('GET', `${ev(slug)}/scoreboard?matrix=true`),
  progress: (slug: string) => request<Progress>('GET', `${ev(slug)}/progress`),

  // VPN
  vpnStatus: (labHost: number | null = null) => request<VpnStatus>('GET', labHost === null ? '/vpn' : `/vpn?lab_host=${labHost}`),
  vpnProfile: (labHost: number | null = null) => downloadFile(labHost === null ? '/vpn/profile' : `/vpn/profile?lab_host=${labHost}`, 'kymo.ovpn', 'POST'),
  labs: () => request<{ labs: LabHost[] }>('GET', '/admin/labs'),
  createLab: (body: LabBody) => request<{ lab: LabHost; check: LabCheck }>('POST', '/admin/labs', body),
  updateLab: (id: number, body: LabBody) => request<{ lab: LabHost }>('PUT', `/admin/labs/${id}`, body),
  deleteLab: (id: number) => request<void>('DELETE', `/admin/labs/${id}`),
  testLab: (id: number) => request<LabCheck>('POST', `/admin/labs/${id}/test`),
  labInstances: (id: number) => request<{ instances: LabInstance[] }>('GET', `/admin/labs/${id}/instances`),
  labScreen: (id: number, name: string) => fetchImage(`/admin/labs/${id}/instances/${encodeURIComponent(name)}/screen`),
  reconcileLab: (id: number) => request<{ removed: string[] }>('POST', `/admin/labs/${id}/reconcile`),
  /** The mode is sent with the host because the two must agree: the VPN needs a gateway there. */
  setEventLab: (event: number, lab: number | null, mode?: 'published' | 'vpn') =>
    request<{ event_id: number; lab_host_id: number | null; network_mode: string }>('PUT', `/admin/events/${event}/lab`, { lab_host_id: lab, ...(mode ? { network_mode: mode } : {}) }),
  adminVpn: () => request<AdminVpn>('GET', '/admin/vpn'),
  syncVpn: () => request<VpnSyncReport>('POST', '/admin/vpn/sync'),
  revokeVpnClient: (userId: number) => request<void>('DELETE', `/admin/vpn/clients/${userId}`),
  setNetworkMode: (event: number, mode: 'published' | 'vpn') =>
    request<{ event_id: number; network_mode: string }>('PUT', `/admin/events/${event}/network`, { mode }),

  // teams
  myTeam: () => request<Team>('GET', '/teams/mine'),
  createTeam: (name: string) => request<Team>('POST', '/teams', { name }),
  joinTeam: (invite_code: string) => request<Team>('POST', '/teams/join', { invite_code }),
  leaveTeam: (id: number) => request<void>('POST', `/teams/${id}/leave`),
  rotateCode: (id: number) => request<{ invite_code: string }>('POST', `/teams/${id}/invite-code/rotate`),
  kick: (team: number, user: number) => request<void>('DELETE', `/teams/${team}/members/${user}`),
  teamProgress: (id: number, slug: string) => request<TeamProgress>('GET', `/teams/${id}/progress${query({ event: slug })}`),

  // administration
  adminEvents: () => request<EventInfo[]>('GET', '/admin/events'),
  createEvent: (body: EventBody) => request<EventInfo>('POST', '/admin/events', body),
  updateEvent: (id: number, body: Partial<EventBody>) => request<EventInfo>('PUT', `/admin/events/${id}`, body),
  setEventState: (id: number, state: string) => request<EventInfo>('POST', `/admin/events/${id}/state`, { state }),
  deleteEvent: (id: number) => request<void>('DELETE', `/admin/events/${id}`),
  participants: (id: number) => request<Participant[]>('GET', `/admin/events/${id}/participants`),
  setScoreboardPublic: (id: number, value: boolean) => request<unknown>('POST', `/admin/events/${id}/scoreboard`, { public: value }),
  categories: (id: number) => request<Category[]>('GET', `/admin/events/${id}/categories`),
  createCategory: (id: number, name: string) => request<Category>('POST', `/admin/events/${id}/categories`, { name }),
  deleteCategory: (id: number) => request<void>('DELETE', `/admin/categories/${id}`),
  adminChallenges: (id: number) => request<AdminChallenge[]>('GET', `/admin/events/${id}/challenges`),
  createChallenge: (event: number, body: ChallengeBody) => request<AdminChallenge>('POST', `/admin/events/${event}/challenges`, body),
  updateChallenge: (id: number, body: ChallengeBody) => request<AdminChallenge>('PUT', `/admin/challenges/${id}`, body),
  deleteChallenge: (id: number) => request<void>('DELETE', `/admin/challenges/${id}`),
  publishChallenge: (id: number) => request<AdminChallenge>('POST', `/admin/challenges/${id}/publish`),
  retireChallenge: (id: number) => request<AdminChallenge>('POST', `/admin/challenges/${id}/retire`),
  testFlag: (id: number, flag: string) => request<{ verdict: Verdict }>('POST', `/admin/challenges/${id}/test`, { flag }),
  addHint: (challenge: number, content: string, cost: number) =>
    request<unknown>('POST', `/admin/challenges/${challenge}/hints`, { content, cost }),
  deleteHint: (id: number) => request<void>('DELETE', `/admin/hints/${id}`),
  uploadAttachment: async (challenge: number, file: File) => {
    const data = new Uint8Array(await file.arrayBuffer())
    let binary = ''
    for (const byte of data) binary += String.fromCharCode(byte)
    return request<unknown>('POST', `/admin/challenges/${challenge}/attachments`, {
      name: file.name,
      mime: file.type || 'application/octet-stream',
      data_base64: btoa(binary),
    })
  },
  deleteAttachment: (id: number) => request<void>('DELETE', `/admin/attachments/${id}`),
  jeopardySettings: (id: number) => request<JeopardySettings>('GET', `/admin/events/${id}/jeopardy`),
  saveJeopardySettings: (id: number, body: JeopardySettings) => request<JeopardySettings>('PUT', `/admin/events/${id}/jeopardy`, body),
  submissions: (id: number, params: { page?: number; verdict?: string; user_id?: number; challenge_id?: number; ip?: string }) =>
    request<Page<SubmissionLine>>('GET', `/admin/events/${id}/submissions${query({ per_page: 25, ...params })}`),
  adminInstances: (id: number) => request<{ instances: AdminInstance[] }>('GET', `/admin/events/${id}/instances`),
  stopAdminInstance: (id: number) => request<unknown>('DELETE', `/admin/instances/${id}`),
  adminInstanceFlags: (id: number) => request<{ flags: { name: string; label: string; value: string }[] }>('GET', `/admin/instances/${id}/flags`),
  adminVerifyLog: (id: number) => request<{ log: VerifyLogEntry[] }>('GET', `/admin/instances/${id}/verify-log`),
  exportScoreboard: (id: number, slug: string) => downloadFile(`/admin/events/${id}/scoreboard.csv`, `${slug}-scoreboard.csv`),
  exportEvent: (id: number, slug: string) => downloadFile(`/admin/events/${id}/export`, `${slug}.kymo-event.json`),
  importEvent: (bundle: string, keepTimes: boolean, keepState: boolean) =>
    request<EventInfo>('POST', `/admin/events/import${query({ keep_times: keepTimes || undefined, keep_state: keepState || undefined })}`, JSON.parse(bundle)),

  users: (params: { q?: string; page?: number }) => request<Page<User>>('GET', `/admin/users${query({ per_page: 25, ...params })}`),
  updateUser: (id: number, body: { role?: Role; banned?: boolean; display_name?: string; bio?: string; email?: string; new_password?: string }) => request<User>('PUT', `/admin/users/${id}`, body),
  registrationOpen: () => request<{ open: boolean }>('GET', '/admin/registration'),
  revealSubmission: (eventId: number, id: number) => request<{ value: string }>('GET', `/admin/events/${eventId}/submissions/${id}/value`),
  createUser: (body: { username: string; email?: string; role?: Role; password?: string }) =>
    request<{ user: User; temporary_password: string | null }>('POST', '/admin/users', body),
  dfirItems: (challenge: number) => request<DfirItem[]>('GET', `/admin/challenges/${challenge}/items`),
  saveDfirItems: (challenge: number, items: DfirItemBody[]) => request<DfirItem[]>('PUT', `/admin/challenges/${challenge}/items`, { items }),
  saveChallengeSettings: (tournament: number, body: ChallengeSettings) => request<Tournament>('PUT', `/admin/tournaments/${tournament}/challenge-settings`, body),
  fillRoster: (tournament: number) => request<Tournament>('POST', `/admin/tournaments/${tournament}/roster/from-registrations`),
  staffMatch: (tournament: number, match: number) => request<MatchView>('GET', `/admin/tournaments/${tournament}/matches/${match}/duel`),
  assignMatch: (tournament: number, match: number, challengeIds: number[]) =>
    request<MatchView>('PUT', `/admin/tournaments/${tournament}/matches/${match}/challenges`, { challenge_ids: challengeIds }),
  finalizeMatch: (tournament: number, match: number, winner?: 'a' | 'b') =>
    request<MatchView>('POST', `/admin/tournaments/${tournament}/matches/${match}/finalize`, winner ? { winner } : {}),
  restartMatch: (tournament: number, match: number) => request<void>('POST', `/admin/tournaments/${tournament}/matches/${match}/restart`),
  myMatches: (slug: string) => request<{ matches: number[] }>('GET', `/tournaments/${encodeURIComponent(slug)}/play/mine`),
  playMatch: (slug: string, match: number) => request<MatchView>('GET', `/tournaments/${encodeURIComponent(slug)}/matches/${match}/play`),
  answerMatch: (slug: string, match: number, challenge: number, value: string, itemId?: number) =>
    request<AnswerResult>('POST', `/tournaments/${encodeURIComponent(slug)}/matches/${match}/challenges/${challenge}/submit`, { value, item_id: itemId }),
  candidates: (event: number, q: string) => request<Candidate[]>('GET', `/admin/events/${event}/candidates${query({ q })}`),
  addParticipant: (event: number, body: { user_id?: number; team_id?: number }) => request<Participant[]>('POST', `/admin/events/${event}/participants`, body),
  removeParticipant: (event: number, participant: number) => request<Participant[]>('DELETE', `/admin/events/${event}/participants/${participant}`),
  setRegistrationClosed: (id: number, closed: boolean) => request<EventInfo>('PUT', `/admin/events/${id}/registration`, { closed }),
  permissionMatrix: () => request<PermissionMatrix>('GET', '/admin/permissions'),
  savePermissions: (roles: Partial<Record<Role, Permission[]>>) => request<PermissionMatrix>('PUT', '/admin/permissions', { roles }),
  resetPermissions: () => request<PermissionMatrix>('POST', '/admin/permissions/reset'),
  myPermissions: () => request<MyPermissions>('GET', '/permissions/mine'),
  contact: () => request<{ contact: string }>('GET', '/admin/contact'),
  setContact: (contact: string) => request<{ contact: string }>('PUT', '/admin/contact', { contact }),
  setRegistrationOpen: (open: boolean) => request<{ open: boolean }>('PUT', '/admin/registration', { open }),

  // monitor
  requests: (params: { page?: number; ip?: string; user_id?: number; route?: string; status?: number }) =>
    request<LogPage>('GET', `/admin/monitor/requests${query({ per_page: 50, ...params })}`),
  monitorSummary: (minutes: number) => request<MonitorSummary>('GET', `/admin/monitor/summary${query({ minutes })}`),
  rules: () => request<{ rules: RateRule[] }>('GET', '/admin/monitor/rules'),
  createRule: (body: RateRuleBody) => request<RateRule>('POST', '/admin/monitor/rules', body),
  updateRule: (id: number, body: RateRuleBody) => request<RateRule>('PUT', `/admin/monitor/rules/${id}`, body),
  deleteRule: (id: number) => request<void>('DELETE', `/admin/monitor/rules/${id}`),
  bans: () => request<{ bans: Ban[] }>('GET', '/admin/monitor/bans'),
  createBan: (body: { kind: 'ip' | 'user'; subject: string; reason: string; duration_secs?: number }) =>
    request<Ban>('POST', '/admin/monitor/bans', body),
  liftBan: (id: number) => request<Ban>('DELETE', `/admin/monitor/bans/${id}`),
  audit: (params: { page?: number; action?: string; q?: string; per_page?: number }) => request<AuditPage>('GET', `/admin/monitor/audit${query({ per_page: 25, ...params })}`),

  // operations
  health: () => request<Health>('GET', '/admin/ops/health'),
  backups: () => request<BackupListing>('GET', '/admin/ops/backups'),
  createBackup: () => request<BackupFile>('POST', '/admin/ops/backups'),
  downloadBackup: () => downloadFile('/admin/ops/backup/download', 'kymo-backup.db'),
  cleanup: () => request<CleanupReport>('POST', '/admin/ops/cleanup'),
}
