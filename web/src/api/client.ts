import { useSyncExternalStore } from 'react'
import type { ApiToken, Appearance, LogEvent, Format, Seeding, Side, Tournament } from './types'

const BASE = '/api/v1'
const TOKEN_KEY = 'kymo.token'

const listeners = new Set<() => void>()

function readToken(): string | null {
  try {
    return sessionStorage.getItem(TOKEN_KEY)
  } catch {
    return null
  }
}

export const session = {
  token: readToken,
  set(token: string) {
    try {
      sessionStorage.setItem(TOKEN_KEY, token)
    } catch {
      /* storage unavailable: session lasts until reload */
    }
    listeners.forEach((l) => l())
  },
  clear() {
    try {
      sessionStorage.removeItem(TOKEN_KEY)
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

export function useIsAdmin(): boolean {
  return useSyncExternalStore(session.subscribe, () => session.token() !== null)
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
  create: (name: string, format: Format, options: { thirdPlace: boolean; groups: number; advance: number }) =>
    request<Tournament>('POST', '/tournaments', {
      name,
      format,
      third_place: options.thirdPlace,
      groups: options.groups,
      advance: options.advance,
    }),
  rename: (id: number, name: string) => request<Tournament>('PUT', admin(id), { name }),
  remove: (id: number) => request<void>('DELETE', admin(id)),
  setTeams: (id: number, teams: { name: string; score: number }[]) =>
    request<Tournament>('PUT', `${admin(id)}/teams`, { teams }),
  start: (id: number, seeding: Seeding) => request<Tournament>('POST', `${admin(id)}/start`, { seeding }),
  reset: (id: number) => request<Tournament>('POST', `${admin(id)}/reset`),
  setResult: (id: number, match: number, a: number, b: number) =>
    request<Tournament>('PUT', `${admin(id)}/matches/${match}/result`, { score_a: a, score_b: b }),
  getAppearance: () => request<Appearance>('GET', '/settings'),
  saveAppearance: (a: Appearance) => request<Appearance>('PUT', '/admin/settings', a),
  resetAppearance: () => request<Appearance>('DELETE', '/admin/settings'),
  listTokens: () => request<ApiToken[]>('GET', '/admin/tokens'),
  createToken: (label: string) => request<{ label: string; token: string }>('POST', '/admin/tokens', { label }),
  revokeToken: (id: number) => request<void>('DELETE', `/admin/tokens/${id}`),
  setForfeit: (id: number, match: number, winner: Side) =>
    request<Tournament>('PUT', `${admin(id)}/matches/${match}/forfeit`, { winner }),
  setSchedule: (id: number, match: number, at: number | null, venue: string | null) =>
    request<Tournament>('PUT', `${admin(id)}/matches/${match}/schedule`, { at, venue }),
  clearResult: (id: number, match: number) => request<Tournament>('DELETE', `${admin(id)}/matches/${match}/result`),
}
