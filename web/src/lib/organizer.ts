import type { Permission } from '../api/platformTypes'

/** Everything an organizer does lives under this prefix, apart from the player site. */
export const ORG = '/organizer'

export const orgEvents = `${ORG}/events`
export const orgEvent = (id: number, view?: string) => `${orgEvents}/${id}${view ? `?view=${view}` : ''}`

export interface OrgLink {
  to: string
  label: string
  /** The permission that opens it. The server checks again on every request. */
  needs: Permission
  group: 'Run' | 'People' | 'Platform'
}

export const ORG_LINKS: OrgLink[] = [
  { to: orgEvents, label: 'Events', needs: 'manage_events', group: 'Run' },
  { to: `${ORG}/monitor`, label: 'Monitor', needs: 'monitor', group: 'Run' },
  { to: `${ORG}/users`, label: 'Users', needs: 'manage_users', group: 'People' },
  { to: `${ORG}/permissions`, label: 'Permissions', needs: 'manage_permissions', group: 'People' },
  { to: `${ORG}/labs`, label: 'Lab hosts', needs: 'manage_settings', group: 'Platform' },
  { to: `${ORG}/ops`, label: 'Operations', needs: 'manage_settings', group: 'Platform' },
  { to: `${ORG}/appearance`, label: 'Appearance', needs: 'manage_settings', group: 'Platform' },
  { to: `${ORG}/tokens`, label: 'API tokens', needs: 'manage_settings', group: 'Platform' },
]

/** What the old addresses (before the console had its own area) turn into, or null for any other path. */
export function legacyOrganizerPath(pathname: string): string | null {
  if (pathname === '/monitor') return `${ORG}/monitor`
  if (pathname === '/settings') return `${ORG}/appearance`
  if (pathname === '/tokens') return `${ORG}/tokens`
  if (pathname === '/login') return `${ORG}/login`
  if (pathname === '/admin') return ORG
  if (pathname.startsWith('/admin/')) return ORG + pathname.slice('/admin'.length)
  return null
}

/** Roles that may sign in to the console. A player never does. */
export const STAFF_ROLES = ['referee', 'monitor', 'admin'] as const
export const isStaffRole = (role: string): boolean => (STAFF_ROLES as readonly string[]).includes(role)

/** Only same site paths inside the console, so a crafted link cannot send an organizer elsewhere. */
export function safeOrganizerNext(raw: string | null): string {
  if (!raw || !raw.startsWith(`${ORG}/`) && raw !== ORG) return ORG
  if (raw.startsWith('//') || raw.includes('\\') || [...raw].some((ch) => ch.charCodeAt(0) < 32)) return ORG
  return raw === `${ORG}/login` ? ORG : raw
}
