import { describe, expect, it } from 'vitest'
import { bySchedule, formatSlot, fromLocalInput, lateMinutes, planLine, relativeStart, toLocalInput } from './schedule'

const NOW = new Date(2026, 10, 7, 12, 0, 0).getTime()
const at = (d: Date) => Math.floor(d.getTime() / 1000)

describe('formatSlot', () => {
  it('names today and tomorrow and falls back to a date', () => {
    expect(formatSlot(at(new Date(2026, 10, 7, 14, 30)), NOW)).toMatch(/^Today 14:30|^Today 02:30/)
    expect(formatSlot(at(new Date(2026, 10, 8, 9, 0)), NOW)).toMatch(/^Tomorrow/)
    expect(formatSlot(at(new Date(2026, 10, 12, 9, 0)), NOW)).not.toMatch(/^Today|^Tomorrow/)
  })
})

describe('relativeStart', () => {
  it('counts minutes, hours and days in both directions', () => {
    expect(relativeStart((NOW + 12 * 60_000) / 1000, NOW)).toBe('in 12 minutes')
    expect(relativeStart((NOW + 60_000) / 1000, NOW)).toBe('in 1 minute')
    expect(relativeStart((NOW + 3 * 3_600_000) / 1000, NOW)).toBe('in 3 hours')
    expect(relativeStart((NOW - 5 * 60_000) / 1000, NOW)).toBe('5 minutes ago')
    expect(relativeStart(NOW / 1000 + 10, NOW)).toBe('starting now')
  })
})

describe('planLine and ordering', () => {
  it('joins time and place and handles missing parts', () => {
    expect(planLine({ scheduled_at: null, venue: null })).toBeNull()
    expect(planLine({ scheduled_at: null, venue: 'Room A' })).toBe('Room A')
    expect(planLine({ scheduled_at: at(new Date(2026, 10, 7, 15, 0)), venue: 'Room A' }, NOW)).toContain('Room A')
  })

  it('puts the earliest match first and unplanned matches last', () => {
    const list = [{ id: 1, scheduled_at: null }, { id: 2, scheduled_at: 200 }, { id: 3, scheduled_at: 100 }]
    expect(bySchedule(list).map((m) => m.id)).toEqual([3, 2, 1])
  })
})

describe('datetime-local conversion', () => {
  it('round trips in the local time zone', () => {
    const seconds = at(new Date(2026, 10, 7, 9, 5))
    expect(toLocalInput(seconds)).toBe('2026-11-07T09:05')
    expect(fromLocalInput('2026-11-07T09:05')).toBe(seconds)
    expect(fromLocalInput('')).toBeNull()
    expect(toLocalInput(null)).toBe('')
  })
})

describe('lateMinutes', () => {
  const team = (id: number) => ({ kind: 'team' as const, team: id })
  const base = { score: null, forfeit: false, slots: [team(1), team(2)] as [ReturnType<typeof team>, ReturnType<typeof team>] }
  it('counts only ready unplayed planned matches past the grace period', () => {
    const planned = (minutesAgo: number) => ({ ...base, scheduled_at: (NOW - minutesAgo * 60_000) / 1000 })
    expect(lateMinutes(planned(5), NOW)).toBeNull()
    expect(lateMinutes(planned(12), NOW)).toBe(12)
    expect(lateMinutes({ ...planned(30), score: [1, 0] as [number, number] }, NOW)).toBeNull()
    expect(lateMinutes({ ...planned(30), forfeit: true }, NOW)).toBeNull()
    expect(lateMinutes({ ...planned(30), slots: [team(1), { kind: 'empty' as const }] as never }, NOW)).toBeNull()
    expect(lateMinutes({ ...base, scheduled_at: null }, NOW)).toBeNull()
  })
})
