import { describe, expect, it } from 'vitest'
import { canGoLive, readiness, type ReadinessInput } from './readiness'

const event = (extra: Partial<ReadinessInput['event']> = {}): ReadinessInput['event'] => ({
  kind: 'jeopardy',
  mode: 'team',
  starts_at: 1000,
  ends_at: 2000,
  registration_opens: 500,
  team_min: 1,
  team_max: 4,
  network_mode: 'vpn',
  lab_host_id: 1,
  ...extra,
})
const line = (extra: Partial<ReadinessInput['challenges'][number]> = {}) => ({ state: 'published' as const, requires_instance: false, release_at: null, retire_at: null, ...extra })
const ids = (input: ReadinessInput) => readiness(input).map((c) => c.id)

describe('a jeopardy event', () => {
  it('with a category and a published challenge has nothing missing', () => {
    const checks = readiness({ event: event(), categories: 1, challenges: [line()] })
    expect(checks).toEqual([{ id: 'ready', level: 'ok', text: 'Nothing is missing.', view: 'overview' }])
    expect(canGoLive(checks)).toBe(true)
  })
  it('needs a category and a published challenge before it can go live', () => {
    const checks = readiness({ event: event(), categories: 0, challenges: [line({ state: 'draft' })] })
    expect(checks.map((c) => c.id)).toEqual(['categories', 'published'])
    expect(checks[1]?.text).toMatch(/still a draft/)
    expect(canGoLive(checks)).toBe(false)
  })
  it('warns about drafts that players will not see', () => {
    expect(ids({ event: event(), categories: 1, challenges: [line(), line({ state: 'draft' }), line({ state: 'draft' })] })).toEqual(['drafts'])
  })
  it('warns when instances publish ports on this server', () => {
    expect(ids({ event: event({ network_mode: 'published', lab_host_id: null }), categories: 1, challenges: [line({ requires_instance: true })] })).toEqual(['exposed'])
    expect(ids({ event: event({ network_mode: 'published', lab_host_id: 3 }), categories: 1, challenges: [line({ requires_instance: true })] })).toEqual(['ready'])
  })
  it('warns about a challenge released after the event ended', () => {
    expect(ids({ event: event(), categories: 1, challenges: [line({ release_at: 2500 })] })).toEqual(['release'])
  })
})

describe('dynamic flags without an instance', () => {
  const dyn = { flag_mode: 'dynamic' as const, kind: 'flag' as const }
  it('block going live when no template file carries the flag', () => {
    const checks = readiness({ event: event(), categories: 1, challenges: [line({ ...dyn, attachments: [] })] })
    expect(checks.map((c) => c.id)).toEqual(['template'])
    expect(canGoLive(checks)).toBe(false)
  })
  it('are fine with a template file, an instance, or static flags', () => {
    const file = { id: 1, challenge_id: 1, name: 'a.txt', mime: 'text/plain', size: 1, sha256: '', template: true }
    expect(ids({ event: event(), categories: 1, challenges: [line({ ...dyn, attachments: [file] })] })).toEqual(['ready'])
    expect(ids({ event: event(), categories: 1, challenges: [line({ ...dyn, requires_instance: true, attachments: [] })] })).toEqual(['ready'])
    expect(ids({ event: event(), categories: 1, challenges: [line({ flag_mode: 'static', attachments: [] })] })).toEqual(['ready'])
  })
})

describe('Responder challenges', () => {
  it('block going live without the private VPN and a lab host', () => {
    const published = [line({ responder: true, requires_instance: true })]
    expect(ids({ event: event({ network_mode: 'published', lab_host_id: 2 }), categories: 1, challenges: published })).toEqual(['responder'])
    expect(ids({ event: event({ network_mode: 'vpn', lab_host_id: null }), categories: 1, challenges: published })).toEqual(['responder'])
    expect(canGoLive(readiness({ event: event({ network_mode: 'published' }), categories: 1, challenges: published }))).toBe(false)
  })
  it('are fine on the VPN with a lab host, from the event or the challenge', () => {
    expect(ids({ event: event({ network_mode: 'vpn', lab_host_id: 2 }), categories: 1, challenges: [line({ responder: true, requires_instance: true })] })).toEqual(['ready'])
    expect(ids({ event: event({ network_mode: 'vpn', lab_host_id: null }), categories: 1, challenges: [line({ responder: true, requires_instance: true, lab_host_id: 4 })] })).toEqual(['ready'])
  })
})

describe('coding challenges', () => {
  it('block going live when no lab host can run them', () => {
    const checks = readiness({ event: event({ lab_host_id: null }), categories: 1, challenges: [line({ kind: 'code' })] })
    expect(checks.map((c) => c.id)).toEqual(['lab'])
    expect(canGoLive(checks)).toBe(false)
  })
  it('are fine with the event or the challenge having a lab host', () => {
    expect(ids({ event: event({ lab_host_id: 2 }), categories: 1, challenges: [line({ kind: 'code' })] })).toEqual(['ready'])
    expect(ids({ event: event({ lab_host_id: null }), categories: 1, challenges: [line({ kind: 'code', lab_host_id: 3 })] })).toEqual(['ready'])
  })
})

describe('the schedule', () => {
  it('warns when a time is missing and blocks when the end comes first', () => {
    expect(ids({ event: event({ starts_at: null, ends_at: null }), categories: 1, challenges: [line()] })).toEqual(['starts', 'ends'])
    const bad = readiness({ event: event({ starts_at: 2000, ends_at: 1000 }), categories: 1, challenges: [line()] })
    expect(bad.map((c) => c.id)).toContain('order')
    expect(canGoLive(bad)).toBe(false)
  })
  it('warns when registration opens after the start', () => {
    expect(ids({ event: event({ registration_opens: 1500 }), categories: 1, challenges: [line()] })).toEqual(['registration'])
  })
  it('blocks a team range that cannot be met, but only for team events', () => {
    expect(ids({ event: event({ team_min: 5, team_max: 2 }), categories: 1, challenges: [line()] })).toEqual(['teams'])
    expect(ids({ event: event({ team_min: 5, team_max: 2, mode: 'solo' }), categories: 1, challenges: [line()] })).toEqual(['ready'])
  })
})

describe('a tournament', () => {
  it('needs no challenges', () => {
    expect(ids({ event: event({ kind: 'tournament' }), categories: 0, challenges: [] })).toEqual(['ready'])
  })
})
