import { describe, expect, it } from 'vitest'
import { movesFrom, statusLine, stepOf } from './lifecycle'

describe('the steps', () => {
  it('put a paused event on the Live step', () => {
    expect(stepOf('draft')).toBe(0)
    expect(stepOf('live')).toBe(2)
    expect(stepOf('paused')).toBe(2)
    expect(stepOf('archived')).toBe(4)
  })
})

describe('the moves', () => {
  it('list the usual next step first and the one that ends something last', () => {
    expect(movesFrom('draft').map((m) => m.label)).toEqual(['Open registration', 'Go live'])
    expect(movesFrom('registration_open').map((m) => m.label)).toEqual(['Go live', 'Back to draft'])
    expect(movesFrom('live').map((m) => [m.label, m.tone])).toEqual([['Pause', 'secondary'], ['End event', 'danger']])
    expect(movesFrom('paused').map((m) => m.label)).toEqual(['Resume', 'End event'])
    expect(movesFrom('ended').map((m) => m.label)).toEqual(['Archive', 'Reopen'])
    expect(movesFrom('archived')).toEqual([])
  })
})

describe('the status line', () => {
  const now = 1_000_000 * 1000
  const e = (state: Parameters<typeof statusLine>[0]['state'], extra = {}) => statusLine({ state, starts_at: null, ends_at: null, ended_at: null, ...extra }, now)
  it('says what matters for each state', () => {
    expect(e('draft')).toBe('Only organizers can see it')
    expect(e('registration_open', { starts_at: 1_000_000 + 7200 })).toBe('Starts in 2 hours')
    expect(e('registration_open')).toBe('Open for registration')
    expect(e('live', { ends_at: 1_000_000 + 3 * 3600 })).toBe('Ends in 3 hours')
    expect(e('live')).toBe('No end time')
    expect(e('live', { ends_at: 1_000_000 - 10 })).toBe('Past its end time')
    expect(e('paused')).toBe('Paused')
    expect(e('ended', { ended_at: 1_000_000 - 7200 })).toBe('Ended 2 hours ago')
    expect(e('archived')).toBe('Hidden from players')
  })
})
