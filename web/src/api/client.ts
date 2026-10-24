import { useSyncExternalStore } from 'react'
import type { ApiToken, PendingReport, ReportInfo, Webhook, Appearance, LogEvent, Format, Seeding, Side, Tournament } from './types'

const BASE = '/api/v1'
const TOKEN_KEY = 'kymo.token'
const SCOPE_KEY = 'kymo.referee'

const listeners = new Set<() => void>()

function readToken(): string | null {
  try {
    return sessionStorage.getItem(TOKEN_KEY)
  } catch {
    return null
  }
}

/** Tournament ids a referee token covers, or null for the organizer. */
function readScope(): number[] | null {
  try {
    const raw = sessionStorage.getItem(SCOPE_KEY)
    return raw === null ? null : (JSON.parse(raw) as number[])
  } catch {
    return null
  }
}

export const session = {
  token: readToken,
  scope: readScope,
  set(token: string, scope: number[] | null = null) {
    try {
      sessionStorage.setItem(TOKEN_KEY, token)
      if (scope === null) sessionStorage.removeItem(SCOPE_KEY)
      else sessionStorage.setItem(SCOPE_KEY, JSON.stringify(scope))
    } catch {
      /* storage unavailable: session lasts until reload */
    }
    listeners.forEach((l) => l())
  },
  clear() {
    try {
      sessionStorage.removeItem(TOKEN_KEY)
      sessionStorage.removeItem(SCOPE_KEY)
    } catch {
      /* nothing stored */
    }
    listeners.forEach((l) => l())
  },
  subscribe(listener: () => void) {
    listeners.add(listener)
    return () => listeners.delete(listener)
  },
}

/** Signed in as the organizer, with every tool. A referee is not an admin. */
export function useIsAdmin(): boolean {
  return useSyncExternalStore(session.subscribe, () => session.token() !== null && sessionStorage.getItem(SCOPE_KEY) === null)
}

/** Signed in as a referee, who can only record results and schedules for some tournaments. */
export function useIsReferee(): boolean {
  return useSyncExternalStore(session.subscribe, () => sessionStorage.getItem(SCOPE_KEY) !== null && session.token() !== null)
}

/** Whether this visitor can enter results for a tournament: the organizer, or a referee covering it. */
export function useCanScore(tournamentId: number | undefined): boolean {
  const admin = useIsAdmin()
  const referee = useIsReferee()
  return admin || (referee && tournamentId !== undefined && (session.scope() ?? []).includes(tournamentId))
}

export class ApiError extends Error {
  readonly status: number
  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const headers: Record<string, string> = {}
  const token = session.token()
  if (token) headers.authorization = `Bearer ${token}`
  if (body !== undefined) headers['content-type'] = 'application/json'
  const res = await fetch(BASE + path, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  })
  if (res.status === 401 && token) session.clear()
  if (res.status === 204) return undefined as T
  const data: unknown = await res.json().catch(() => null)
  if (!res.ok) {
    const message = (data as { error?: string } | null)?.error ?? 'Request failed'
    throw new ApiError(res.status, message)
  }
  return data as T
}

const admin = (id: number) => `/admin/tournaments/${id}`

export const api = {
  login: (password: string) => request<{ token: string }>('POST', '/auth/login', { password }),
  logout: () => request<void>('POST', '/auth/logout'),
  list: () => request<Tournament[]>('GET', '/tournaments'),
  get: (slug: string) => request<Tournament>('GET', `/tournaments/${encodeURIComponent(slug)}`),
  loadDemo: () => request<{ added: number }>('POST', '/admin/demo'),
  activity: (slug: string) => request<LogEvent[]>('GET', `/tournaments/${encodeURIComponent(slug)}/activity`),
  create: (name: string, format: Format, options: { thirdPlace: boolean; groups: number; advance: number; rounds: number; bestOf: number }) =>
    request<Tournament>('POST', '/tournaments', {
      name,
      format,
      third_place: options.thirdPlace,
      groups: options.groups,
      advance: options.advance,
      rounds: options.rounds,
      best_of: options.bestOf,
    }),
  rename: (id: number, name: string) => request<Tournament>('PUT', admin(id), { name }),
  remove: (id: number) => request<void>('DELETE', admin(id)),
  setTeams: (id: number, teams: { name: string; score: number }[]) =>
    request<Tournament>('PUT', `${admin(id)}/teams`, { teams }),
  start: (id: number, seeding: Seeding) => request<Tournament>('POST', `${admin(id)}/start`, { seeding }),
  listWebhooks: () => request<Webhook[]>('GET', '/admin/webhooks'),
  createWebhook: (label: string, url: string) => request<Webhook>('POST', '/admin/webhooks', { label, url }),
  deleteWebhook: (id: number) => request<void>('DELETE', `/admin/webhooks/${id}`),
  testWebhook: (id: number) => request<{ status: number }>('POST', `/admin/webhooks/${id}/test`),
  me: () => request<{ role: 'organizer' | 'referee'; tournaments: number[] | null }>('GET', '/auth/me'),
  teamCode: (id: number, team: number) => request<{ code: string }>('POST', `${admin(id)}/teams/${team}/code`),
  reportInfo: (code: string) => request<ReportInfo>('GET', `/report/${code}`),
  submitReport: (code: string, own: number, other: number) => request<void>('POST', `/report/${code}`, { own, other }),
  listReports: () => request<PendingReport[]>('GET', '/admin/reports'),
  approveReport: (id: number) => request<Tournament>('POST', `/admin/reports/${id}/approve`),
  rejectReport: (id: number) => request<void>('POST', `/admin/reports/${id}/reject`),
  importSetup: (setup: unknown) => request<Tournament>('POST', '/admin/import', setup),
  setAccent: (id: number, accent: string | null) => request<Tournament>('PUT', `${admin(id)}/accent`, { accent }),
  setTiebreak: (id: number, order: number[]) => request<Tournament>('PUT', `${admin(id)}/tiebreak`, { order }),
  reset: (id: number) => request<Tournament>('POST', `${admin(id)}/reset`),
  setGames: (id: number, match: number, games: [number, number][]) =>
    request<Tournament>('PUT', `${admin(id)}/matches/${match}/games`, { games }),
  setResult: (id: number, match: number, a: number, b: number) =>
    request<Tournament>('PUT', `${admin(id)}/matches/${match}/result`, { score_a: a, score_b: b }),
  getAppearance: () => request<Appearance>('GET', '/settings'),
  saveAppearance: (a: Appearance) => request<Appearance>('PUT', '/admin/settings', a),
  resetAppearance: () => request<Appearance>('DELETE', '/admin/settings'),
  listTokens: () => request<ApiToken[]>('GET', '/admin/tokens'),
  createToken: (label: string, expiresDays: number | null, tournaments: number[] | null = null) =>
    request<{ label: string; token: string }>('POST', '/admin/tokens', { label, expires_days: expiresDays, ...(tournaments ? { tournaments } : {}) }),
  revokeToken: (id: number) => request<void>('DELETE', `/admin/tokens/${id}`),
  setForfeit: (id: number, match: number, winner: Side) =>
    request<Tournament>('PUT', `${admin(id)}/matches/${match}/forfeit`, { winner }),
  setSchedule: (id: number, match: number, at: number | null, venue: string | null) =>
    request<Tournament>('PUT', `${admin(id)}/matches/${match}/schedule`, { at, venue }),
  clearResult: (id: number, match: number) => request<Tournament>('DELETE', `${admin(id)}/matches/${match}/result`),
}
