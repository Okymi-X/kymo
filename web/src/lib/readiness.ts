// "Is this event ready to go live?" in words, from what the organizer has already set up. Advice
// only: the server decides what it refuses, and an organizer may go live with warnings.

import type { AdminChallenge, EventInfo } from '../api/platformTypes'

export type CheckLevel = 'ok' | 'warn' | 'block'
export type CheckView = 'overview' | 'settings' | 'challenges' | 'infrastructure' | 'participants'

export interface Check {
  id: string
  level: CheckLevel
  text: string
  /** The tab that fixes it. */
  view: CheckView
}

type ChallengeLine = Pick<AdminChallenge, 'state' | 'requires_instance' | 'release_at' | 'retire_at'> & Partial<Pick<AdminChallenge, 'flag_mode' | 'kind' | 'attachments' | 'lab_host_id' | 'responder'>>

export interface ReadinessInput {
  event: Pick<EventInfo, 'kind' | 'mode' | 'starts_at' | 'ends_at' | 'registration_opens' | 'team_min' | 'team_max' | 'network_mode' | 'lab_host_id'>
  categories: number
  challenges: ChallengeLine[]
}

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`

export function readiness({ event, categories, challenges }: ReadinessInput): Check[] {
  const out: Check[] = []
  const add = (id: string, level: CheckLevel, text: string, view: CheckView) => out.push({ id, level, text, view })

  if (event.kind === 'jeopardy') {
    const published = challenges.filter((c) => c.state === 'published')
    const drafts = challenges.filter((c) => c.state === 'draft')
    if (categories === 0) add('categories', 'block', 'Add a category: challenges are grouped by category.', 'challenges')
    if (published.length === 0) {
      add('published', 'block', drafts.length > 0 ? `${plural(drafts.length, 'challenge is', 'challenges are')} still a draft. Publish at least one.` : 'Create and publish at least one challenge.', 'challenges')
    } else if (drafts.length > 0) {
      add('drafts', 'warn', `${plural(drafts.length, 'challenge is', 'challenges are')} still a draft and hidden from players.`, 'challenges')
    }
    const instanced = published.filter((c) => c.requires_instance).length
    if (instanced > 0 && event.network_mode === 'published' && event.lab_host_id === null) {
      add('exposed', 'warn', `${plural(instanced, 'challenge starts', 'challenges start')} instances on this server and publishes their ports. A lab host with the private VPN keeps them off the platform.`, 'infrastructure')
    }
    // Code only runs on a lab host: the challenge's own or the event's.
    const stranded = published.filter((c) => c.kind === 'code' && (c.lab_host_id ?? event.lab_host_id) === null).length
    if (stranded > 0) {
      add('lab', 'block', `${plural(stranded, 'coding challenge needs', 'coding challenges need')} a lab host to run on, and the event has none.`, 'infrastructure')
    }
    // A Responder's machine and Verifier live on a private network behind a lab host's VPN.
    const unreachable = published.filter((c) => c.responder && (event.network_mode !== 'vpn' || (c.lab_host_id ?? event.lab_host_id) === null)).length
    if (unreachable > 0) {
      add('responder', 'block', `${plural(unreachable, 'Responder challenge needs', 'Responder challenges need')} the event on the private VPN with a lab host, or no player can start it.`, 'infrastructure')
    }
    // A dynamic flag with no instance is only delivered through a template file.
    const homeless = published.filter((c) => c.kind !== 'dfir' && c.flag_mode === 'dynamic' && !c.requires_instance && !(c.attachments ?? []).some((a) => a.template)).length
    if (homeless > 0) {
      add('template', 'block', `${plural(homeless, 'challenge has', 'challenges have')} dynamic flags but starts nothing and has no per-player template file, so players cannot find their flag.`, 'challenges')
    }
    if (event.ends_at !== null && published.some((c) => c.release_at !== null && c.release_at >= (event.ends_at as number))) {
      add('release', 'warn', 'A challenge is released after the event ends, so nobody will see it.', 'challenges')
    }
  }

  if (event.starts_at === null) add('starts', 'warn', 'No start time: you will have to start the event by hand.', 'settings')
  if (event.ends_at === null) add('ends', 'warn', 'No end time: you will have to end the event by hand.', 'settings')
  if (event.starts_at !== null && event.ends_at !== null && event.ends_at <= event.starts_at) add('order', 'block', 'The event ends before it starts.', 'settings')
  if (event.registration_opens !== null && event.starts_at !== null && event.registration_opens > event.starts_at) {
    add('registration', 'warn', 'Registration opens after the event starts.', 'settings')
  }
  if (event.mode === 'team' && event.team_min > event.team_max) add('teams', 'block', 'The smallest team is larger than the largest.', 'settings')

  if (out.length === 0) add('ready', 'ok', 'Nothing is missing.', 'overview')
  return out
}

/** True when nothing stops the event from going live. */
export const canGoLive = (checks: Check[]): boolean => !checks.some((c) => c.level === 'block')
