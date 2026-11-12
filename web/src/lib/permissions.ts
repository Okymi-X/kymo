import type { Permission, Role } from '../api/platformTypes'

export type PermissionGroup = 'Everyone' | 'Playing' | 'Running events' | 'Administration'

export interface PermissionInfo {
  label: string
  description: string
  group: PermissionGroup
  /** Handing this out lets the role do serious damage or hand out more, so granting it asks first. */
  sensitive?: boolean
}

export const PERMISSION_INFO: Record<Permission, PermissionInfo> = {
  view_public: { label: 'View public pages', description: 'Events, tournaments and published results.', group: 'Everyone' },
  view_scoreboard_public: { label: 'View published scoreboards', description: 'See a scoreboard once it is published.', group: 'Everyone' },
  play_event: { label: 'Play events', description: 'Register, open the challenge board, download files and unlock hints.', group: 'Playing' },
  manage_team: { label: 'Manage a team', description: 'Create or join a team, invite, leave and remove members.', group: 'Playing' },
  submit_flag: { label: 'Submit flags', description: 'Send answers to challenges.', group: 'Playing' },
  start_instance: { label: 'Start instances', description: 'Start and restart challenge containers and download a VPN profile.', group: 'Playing' },
  referee: { label: 'Record results', description: 'Enter and correct match results and schedules.', group: 'Running events' },
  manage_events: { label: 'Manage events', description: 'Create, edit, start and end events and tournaments, and see participants.', group: 'Running events' },
  manage_challenges: { label: 'Manage challenges', description: 'Write challenges and flags, release them, and see unpublished events.', group: 'Running events', sensitive: true },
  view_hidden_scoreboard: { label: 'See unpublished scoreboards', description: 'View a scoreboard before it is published.', group: 'Running events' },
  preview_as_player: { label: 'Preview as a player', description: 'Look at an event the way a player does.', group: 'Running events' },
  monitor: { label: 'Monitor traffic', description: 'Read the request log, summaries and submission history. Exempt from rate limits and bans.', group: 'Administration' },
  moderate_bans: { label: 'Moderate', description: 'Change rate limits and ban or unban addresses and accounts.', group: 'Administration' },
  view_audit: { label: 'Read the audit log', description: 'See who did what.', group: 'Administration' },
  manage_users: { label: 'Manage users', description: 'Create accounts, change roles, ban and unban.', group: 'Administration', sensitive: true },
  manage_settings: { label: 'Manage settings', description: 'Appearance, sign-ups, API tokens, operations, backups and lab hosts.', group: 'Administration', sensitive: true },
  manage_permissions: { label: 'Manage permissions', description: 'Change this table.', group: 'Administration', sensitive: true },
}

export const GROUPS: PermissionGroup[] = ['Everyone', 'Playing', 'Running events', 'Administration']

/** The list with one permission switched on or off, in the server's order. */
export function withPermission(all: Permission[], granted: Permission[], permission: Permission, on: boolean): Permission[] {
  const set = new Set(granted)
  if (on) set.add(permission)
  else set.delete(permission)
  return all.filter((p) => set.has(p))
}

export function changes(before: Permission[], after: Permission[]): { granted: Permission[]; revoked: Permission[] } {
  const was = new Set(before)
  const now = new Set(after)
  return {
    granted: after.filter((p) => !was.has(p)),
    revoked: before.filter((p) => !now.has(p)),
  }
}

export function sensitiveGrants(granted: Permission[]): Permission[] {
  return granted.filter((p) => PERMISSION_INFO[p].sensitive)
}

export const ROLE_LABEL: Record<Role, string> = {
  visitor: 'Visitor',
  player: 'Player',
  referee: 'Referee',
  monitor: 'Monitor',
  admin: 'Admin',
}
