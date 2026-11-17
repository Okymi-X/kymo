// The audit log stores short action codes. People read these instead.

export type AuditKind = 'security' | 'moderation' | 'config' | 'event' | 'other'

const KNOWN: Record<string, { label: string; kind: AuditKind }> = {
  organizer_login: { label: 'Organizer signed in', kind: 'security' },
  auto_ban: { label: 'Banned automatically', kind: 'moderation' },
  participant_disqualify: { label: 'Disqualified from an event', kind: 'moderation' },
  participant_reinstate: { label: 'Reinstated in an event', kind: 'moderation' },
  strikes_forgiven: { label: 'Strikes forgiven', kind: 'moderation' },
  flag_sharing_review: { label: 'Flag sharing reported', kind: 'moderation' },
  flag_shared: { label: 'Flag shared', kind: 'moderation' },
  event_lab: { label: 'Event lab host changed', kind: 'config' },
  challenge_lab: { label: 'Challenge lab host changed', kind: 'config' },
  lab_create: { label: 'Lab host added', kind: 'config' },
  'home.update': { label: 'Home page edited', kind: 'config' },
  duel_finalized: { label: 'Match finalized', kind: 'event' },
}

/** "ban_create" becomes "Ban create" when nothing better is known. */
const humanize = (code: string) => {
  const text = code.replace(/[_.]+/g, ' ').trim()
  return text.charAt(0).toUpperCase() + text.slice(1)
}

export function describeAction(code: string): { label: string; kind: AuditKind } {
  return KNOWN[code] ?? { label: humanize(code), kind: /ban|block|forbid/.test(code) ? 'moderation' : 'other' }
}

export const KIND_TONE: Record<AuditKind, string> = {
  security: 'bg-accent',
  moderation: 'bg-danger',
  config: 'bg-muted',
  event: 'bg-win',
  other: 'bg-line',
}

/** What an entry was about, in a few words. */
export function describeTarget(kind: string, id: string): string {
  if (kind === 'session') return id === 'organizer password' ? 'with the organizer password' : `as ${id}`
  const noun = kind.replace(/_/g, ' ')
  return id ? `${noun.charAt(0).toUpperCase()}${noun.slice(1)} ${id}` : noun
}
