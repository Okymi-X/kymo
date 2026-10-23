import { describe, expect, it } from 'vitest'
import { timeAgo } from './time'

const NOW = 1_000_000_000_000

describe('timeAgo', () => {
  const ago = (seconds: number) => timeAgo(NOW / 1000 - seconds, NOW)
  it('says just now for the first moments', () => {
    expect(ago(0)).toBe('just now')
    expect(ago(44)).toBe('just now')
  })
  it('uses minutes, hours and days with plurals', () => {
    expect(ago(60)).toBe('1 minute ago')
    expect(ago(5 * 60)).toBe('5 minutes ago')
    expect(ago(3600)).toBe('1 hour ago')
    expect(ago(3 * 3600)).toBe('3 hours ago')
    expect(ago(2 * 86400)).toBe('2 days ago')
  })
  it('never goes negative when the clock is behind the server', () => {
    expect(timeAgo(NOW / 1000 + 500, NOW)).toBe('just now')
  })
  it('switches to a date after a week', () => {
    expect(ago(8 * 86400)).toMatch(/\d/)
    expect(ago(8 * 86400)).not.toContain('ago')
  })
})
