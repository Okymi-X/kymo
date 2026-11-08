import { useMemo, useSyncExternalStore } from 'react'
import type { ApiToken, LadderLine, LadderSeries, PendingReport, ReportInfo, Webhook, Appearance, LogEvent, Format, Seeding, Side, Tournament } from './types'

const BASE = '/api/v1'
const TOKEN_KEY = 'kymo.token'
const SCOPE_KEY = 'kymo.referee'
const WHO_KEY = 'kymo.who'
const SESSION_KEYS = [TOKEN_KEY, SCOPE_KEY, WHO_KEY]

/** Who is signed in beyond the token: a player account with its role. The legacy organizer
 *  password and referee tokens have no account, so they store nothing here. */
export interface Who {
  id: number
  username: string
  role: 'player' | 'referee' | 'monitor' | 'admin'
}

const listeners = new Set<() => void>()

function readStored(key: string): string | null {
  try {
    return localStorage.getItem(key)
  } catch {
    return null
  }
}

// Preserve a session from a tab that was open before auth state moved to shared storage.
try {
  if (localStorage.getItem(TOKEN_KEY) === null) {
    for (const key of SESSION_KEYS) {
      const value = sessionStorage.getItem(key)
      if (value !== null) localStorage.setItem(key, value)
    }
  }
  SESSION_KEYS.forEach((key) => sessionStorage.removeItem(key))
} catch {
  /* storage unavailable */
}

if (typeof window !== 'undefined') {
  window.addEventListener('storage', (event) => {
    if (event.key === null || SESSION_KEYS.includes(event.key)) listeners.forEach((l) => l())
  })
}

function readToken(): string | null {
  return readStored(TOKEN_KEY)
}

/** Tournament ids a referee token covers, or null for the organizer. */
function readScope(): number[] | null {
  try {
    const raw = readStored(SCOPE_KEY)
    return raw === null ? null : (JSON.parse(raw) as number[])
  } catch {
    return null
  }
}

function readWho(): Who | null {
  try {
    const raw = readStored(WHO_KEY)
    return raw === null ? null : (JSON.parse(raw) as Who)
  } catch {
    return null
  }
}

export const session = {
  token: readToken,
  scope: readScope,
  who: readWho,
  set(token: string, scope: number[] | null = null, who: Who | null = null) {
    try {
      localStorage.setItem(TOKEN_KEY, token)
      if (scope === null) localStorage.removeItem(SCOPE_KEY)
      else localStorage.setItem(SCOPE_KEY, JSON.stringify(scope))
      if (who === null) localStorage.removeItem(WHO_KEY)
      else localStorage.setItem(WHO_KEY, JSON.stringify(who))
    } catch {
      /* storage unavailable: sign-in cannot be persisted */
    }
    listeners.forEach((l) => l())
  },
  clear() {
    try {
      SESSION_KEYS.forEach((key) => localStorage.removeItem(key))
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

/** The signed in player account, or null for visitors, the legacy organizer and referee tokens. */
export function useWho(): Who | null {
  const raw = useSyncExternalStore(session.subscribe, () => readStored(WHO_KEY))
  return useMemo(() => {
    try {
      return raw === null ? null : (JSON.parse(raw) as Who)
    } catch {
      return null
    }
  }, [raw])
}

/** Signed in at all, with any kind of session. */
export function useSignedIn(): boolean {
  return useSyncExternalStore(session.subscribe, () => session.token() !== null)
}

/** Signed in as the organizer, with every tool: the organizer password or an admin account.
 *  A referee is not an admin. */
export function useIsAdmin(): boolean {
  return useSyncExternalStore(session.subscribe, () => {
    if (session.token() === null) return false
    const who = readWho()
    return who ? who.role === 'admin' : readStored(SCOPE_KEY) === null
  })
}

/** Signed in as a referee, who can only record results and schedules for some tournaments. */
export function useIsReferee(): boolean {
  return useSyncExternalStore(session.subscribe, () => readStored(SCOPE_KEY) !== null && session.token() !== null)
}

/** Whether the signed in account may use the monitor: monitors and admins. */
export function useCanMonitor(): boolean {
  const who = useWho()
  const admin = useIsAdmin()
  return admin || who?.role === 'monitor'
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

/** What a person should read for a failed request. The server's own words are kept when they
 *  already say what is wrong; the rest are replaced by something that tells them what to do. */
export function friendlyMessage(status: number, serverMessage: string | undefined, hadSession: boolean, path: string): string {
  // A failed sign in says nothing about which half was wrong, so an unknown name reads the same.
  if (status === 401 && path === '/auth/player-login') return 'Username or password is incorrect.'
  if (status === 401 && path === '/auth/login') return 'That password is not correct.'
  if (status === 401 && hadSession && !path.startsWith('/auth/')) return 'Your session has ended. Sign in again.'
  // A 503 carries a message written for people (the VPN or the container platform is down); a bare 500 does not.
  if (status >= 500 && status !== 503) return 'Something went wrong on the server. Try again in a moment.'
  if (status === 404 && (serverMessage === undefined || serverMessage === 'not found')) return 'That could not be found.'
  return serverMessage ?? 'The request failed. Try again.'
}

export async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const headers: Record<string, string> = {}
  const token = session.token()
  if (token) headers.authorization = `Bearer ${token}`
  if (body !== undefined) headers['content-type'] = 'application/json'
  let res: Response
  try {
    res = await fetch(BASE + path, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    })
  } catch {
    // The browser could not reach the server at all (offline, server stopped, blocked).
    throw new ApiError(0, 'Cannot reach the server. Check your connection and try again.')
  }
  // Only end the session that was actually rejected. A slow request made with an earlier
  // token (before a sign out) must not sign out whoever signed in since.
  if (res.status === 401 && token && session.token() === token) session.clear()
  if (res.status === 204) return undefined as T
  const data: unknown = await res.json().catch(() => null)
  if (!res.ok) {
    throw new ApiError(res.status, friendlyMessage(res.status, (data as { error?: string } | null)?.error, token !== null, path))
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
  rename: (id: number, name: string, description?: string) => request<Tournament>('PUT', admin(id), { name, ...(description === undefined ? {} : { description }) }),
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
  ladder: () => request<LadderLine[]>('GET', '/ladder'),
  ladderTimeline: () => request<LadderSeries[]>('GET', '/ladder/timeline'),
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
  setLive: (id: number, match: number, live: boolean) =>
    request<Tournament>('PUT', `${admin(id)}/matches/${match}/live`, { live }),
  setForfeit: (id: number, match: number, winner: Side) =>
    request<Tournament>('PUT', `${admin(id)}/matches/${match}/forfeit`, { winner }),
  setSchedule: (id: number, match: number, at: number | null, venue: string | null) =>
    request<Tournament>('PUT', `${admin(id)}/matches/${match}/schedule`, { at, venue }),
  clearResult: (id: number, match: number) => request<Tournament>('DELETE', `${admin(id)}/matches/${match}/result`),
}
