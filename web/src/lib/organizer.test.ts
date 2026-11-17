import { describe, expect, it } from 'vitest'
import { legacyOrganizerPath, safeOrganizerNext } from './organizer'

describe('legacyOrganizerPath', () => {
  it('moves every old organizer address under the console', () => {
    expect(legacyOrganizerPath('/admin/events')).toBe('/organizer/events')
    expect(legacyOrganizerPath('/admin/events/4')).toBe('/organizer/events/4')
    expect(legacyOrganizerPath('/monitor')).toBe('/organizer/monitor')
    expect(legacyOrganizerPath('/settings')).toBe('/organizer/appearance')
    expect(legacyOrganizerPath('/tokens')).toBe('/organizer/tokens')
    expect(legacyOrganizerPath('/login')).toBe('/organizer/login')
  })
  it('leaves player pages alone', () => {
    expect(legacyOrganizerPath('/events')).toBeNull()
    expect(legacyOrganizerPath('/administrator')).toBeNull()
  })
})

describe('safeOrganizerNext', () => {
  it('keeps console paths and refuses anything else', () => {
    expect(safeOrganizerNext('/organizer/events/3?view=challenges')).toBe('/organizer/events/3?view=challenges')
    expect(safeOrganizerNext('/events')).toBe('/organizer')
    expect(safeOrganizerNext('//evil.example')).toBe('/organizer')
    expect(safeOrganizerNext('/organizer/\\evil.example')).toBe('/organizer')
    expect(safeOrganizerNext('https://evil.example')).toBe('/organizer')
    expect(safeOrganizerNext(null)).toBe('/organizer')
    expect(safeOrganizerNext('/organizer/login')).toBe('/organizer')
  })
})
