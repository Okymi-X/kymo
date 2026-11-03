import { describe, expect, it } from 'vitest'
import type { Category, PlayerChallenge } from '../api/platformTypes'
import { countdown, flagHint, fileSize, fromLocalInput, groupByCategory, nextStates, scoringLabel, shortDuration, toLocalInput } from './platform'

const challenge = (id: number, category_id: number, points: number, title = `c${id}`): PlayerChallenge => ({
  id,
  category_id,
  title,
  description: '',
  points,
  solved: false,
  solved_at: null,
  solved_by: 0,
  status: 'open',
  release_at: null,
  retire_at: null,
  flag_format: { prefix: '', suffix: '' },
  max_attempts: null,
  attempts_remaining: null,
  requires_instance: false,
  hints: [],
  attachments: [],
})

describe('countdown', () => {
  it('formats hours, minutes and seconds', () => {
    expect(countdown(3900)).toBe('1h 05m')
    expect(countdown(249)).toBe('4m 09s')
    expect(countdown(38)).toBe('38s')
    expect(countdown(0)).toBe('0s')
  })
  it('never goes negative', () => {
    expect(countdown(-5)).toBe('0s')
  })
})

describe('groupByCategory', () => {
  const cats: Category[] = [
    { id: 2, event_id: 1, name: 'Pwn', position: 1 },
    { id: 1, event_id: 1, name: 'Web', position: 0 },
    { id: 3, event_id: 1, name: 'Empty', position: 2 },
  ]
  it('orders categories by position and challenges by points then title', () => {
    const groups = groupByCategory(cats, [challenge(1, 1, 300), challenge(2, 1, 100, 'b'), challenge(3, 1, 100, 'a'), challenge(4, 2, 50)])
    expect(groups.map((g) => g.category.name)).toEqual(['Web', 'Pwn'])
    expect(groups[0].challenges.map((c) => c.id)).toEqual([3, 2, 1])
  })
  it('leaves out categories with no visible challenge', () => {
    expect(groupByCategory(cats, [])).toEqual([])
  })
})

describe('event states', () => {
  it('only offers the moves the server allows', () => {
    expect(nextStates.draft).toEqual(['registration_open', 'live'])
    expect(nextStates.ended).toEqual(['archived'])
    expect(nextStates.archived).toEqual([])
  })
})

describe('local date inputs', () => {
  it('round trips a whole minute', () => {
    const unix = Math.floor(new Date(2026, 5, 7, 14, 30).getTime() / 1000)
    expect(toLocalInput(unix)).toBe('2026-06-07T14:30')
    expect(fromLocalInput('2026-06-07T14:30')).toBe(unix)
  })
  it('treats empty and invalid values as none', () => {
    expect(toLocalInput(null)).toBe('')
    expect(fromLocalInput('')).toBeNull()
    expect(fromLocalInput('not a date')).toBeNull()
  })
})

describe('labels', () => {
  it('describes scoring', () => {
    expect(scoringLabel({ kind: 'static', points: 100 })).toBe('100 points')
    expect(scoringLabel({ kind: 'dynamic', initial: 500, minimum: 100, decay: 20, curve: 'linear' })).toBe('500 down to 100')
  })
  it('formats sizes and durations', () => {
    expect(fileSize(310)).toBe('310 B')
    expect(fileSize(2048)).toBe('2.0 KB')
    expect(fileSize(4.2 * 1024 * 1024)).toBe('4.2 MB')
    expect(shortDuration(900)).toBe('15 min')
    expect(shortDuration(7200)).toBe('2 h')
    expect(shortDuration(90)).toBe('90 s')
    expect(shortDuration(86400)).toBe('1 d')
  })
  it('shows the flag shape only when there is one', () => {
    expect(flagHint('KYMO{', '}')).toBe('KYMO{...}')
    expect(flagHint('', '')).toBe('')
  })
})
