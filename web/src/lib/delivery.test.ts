import { describe, expect, it } from 'vitest'
import type { LabArtifact, LabCatalog } from '../api/platformTypes'
import { artifactsFor, challengeType, deliveryOf, deliveryProblems, displayText, manifestDefaults, undeliveredFlags } from './delivery'

const box = (extra: Partial<LabArtifact> = {}): LabArtifact => ({ artifact: 'web:1', runtime: 'docker', port: 8000, max_ttl_secs: 3600, env_allow: [], ...extra })
const machine = (extra: Partial<LabArtifact> = {}): LabArtifact => ({ artifact: 'kymera', runtime: 'vm', port: 443, max_ttl_secs: 7200, env_allow: [], memory_mb: 2048, cpu_millis: 2000, ...extra })
const catalog = (artifacts: LabArtifact[], extra: Partial<LabCatalog> = {}): LabCatalog => ({ ok: true, mode: 'manifest', runtimes: ['docker', 'vm'], artifacts, ...extra })

describe('the way a challenge is delivered', () => {
  it('follows the instance switch and the machine choice', () => {
    expect(deliveryOf(false, true)).toBe('none')
    expect(deliveryOf(true, false)).toBe('container')
    expect(deliveryOf(true, true)).toBe('vm')
  })
  it('offers machines to a machine challenge and containers to a container one', () => {
    const all = [box(), machine(), box({ artifact: 'p:1', runtime: 'podman' })]
    expect(artifactsFor('vm', all).map((a) => a.artifact)).toEqual(['kymera'])
    expect(artifactsFor('container', all).map((a) => a.artifact)).toEqual(['web:1', 'p:1'])
    expect(artifactsFor('none', all)).toEqual([])
  })
})

describe('what the lab host would refuse', () => {
  it('a machine needs a lab host that can run machines', () => {
    expect(deliveryProblems({ delivery: 'vm', image: 'kymera', labId: null, catalog: null }).labHost).toMatch(/lab host/)
    expect(deliveryProblems({ delivery: 'vm', image: 'kymera', labId: 1, catalog: catalog([machine()], { runtimes: ['docker'] }) }).labHost).toMatch(/cannot run virtual machines/)
    expect(deliveryProblems({ delivery: 'vm', image: 'kymera', labId: 1, catalog: catalog([machine()]) })).toEqual({})
  })
  it('a name without a manifest is refused in manifest mode and fine in open mode', () => {
    expect(deliveryProblems({ delivery: 'container', image: 'nope:1', labId: 1, catalog: catalog([box()]) }).image).toMatch(/no manifest/)
    expect(deliveryProblems({ delivery: 'container', image: 'nope:1', labId: 1, catalog: catalog([box()], { mode: 'open' }) })).toEqual({})
  })
  it('a machine picked as a container, and the other way round, is caught', () => {
    expect(deliveryProblems({ delivery: 'container', image: 'kymera', labId: 1, catalog: catalog([machine()]) }).image).toMatch(/virtual machine/)
    expect(deliveryProblems({ delivery: 'vm', image: 'web:1', labId: 1, catalog: catalog([box()]) }).image).toMatch(/container/)
  })
  it('says nothing when the lab host could not be read', () => {
    expect(deliveryProblems({ delivery: 'container', image: 'x', labId: 1, catalog: { ok: false, error: 'down' } })).toEqual({})
    expect(deliveryProblems({ delivery: 'none', image: '', labId: null, catalog: null })).toEqual({})
  })
})

describe('machine resources', () => {
  const fallback = { port: '8000', cpu: '500', memory: '256', pids: '128' }
  it('take the manifest values, so a small default cannot shrink the machine', () => {
    expect(manifestDefaults(machine(), fallback)).toEqual({ port: '443', cpu: '2000', memory: '2048', pids: '128' })
  })
  it('keep the defaults for what the manifest leaves out', () => {
    expect(manifestDefaults(box({ port: 0 }), fallback)).toEqual(fallback)
  })
})

describe('texts and flag names', () => {
  it('tells how players reach a machine and a service', () => {
    expect(displayText(machine())).toMatch(/every port/)
    expect(displayText(box())).toMatch(/port of the service/)
    expect(displayText(machine({ display: 'service' }))).toMatch(/port of the service/)
  })
  it('lists the flag names the manifest does not deliver', () => {
    expect(undeliveredFlags(['user', 'root', 'extra'], machine({ flags: ['user', 'root'] }))).toEqual(['extra'])
    expect(undeliveredFlags(['anything'], machine())).toEqual([])
    expect(undeliveredFlags(['anything'], undefined)).toEqual([])
  })
})

describe('challengeType', () => {
  const base = { kind: 'flag' as const, responder: false, multi_flag: false, flag_mode: 'static' as const, requires_instance: false, flag: { kind: 'static' as const } }
  it('names every kind of challenge the editor can make', () => {
    expect(challengeType(base)).toBe('Flag')
    expect(challengeType({ ...base, flag: { kind: 'regex' } })).toBe('Pattern flag')
    expect(challengeType({ ...base, flag: { kind: 'per_team' } })).toBe('Per-team flag')
    expect(challengeType({ ...base, flag_mode: 'dynamic', requires_instance: true })).toBe('Dynamic flag + instance')
    expect(challengeType({ ...base, multi_flag: true })).toBe('Several flags')
    expect(challengeType({ ...base, multi_flag: true, flag_mode: 'dynamic', requires_instance: true })).toBe('Dynamic flags + instance')
    expect(challengeType({ ...base, kind: 'dfir' })).toBe('Multi-question')
    expect(challengeType({ ...base, kind: 'code' })).toBe('Coding')
    expect(challengeType({ ...base, responder: true, requires_instance: true })).toBe('Responder')
    expect(challengeType({ ...base, requires_instance: true })).toBe('Flag + instance')
  })
})
