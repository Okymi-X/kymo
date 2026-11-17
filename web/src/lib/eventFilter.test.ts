import { describe, expect, it } from 'vitest'
import { countByGroup, filterEvents, groupOf } from './eventFilter'

const ev = (name: string, effective_state: 'live' | 'paused' | 'draft' | 'registration_open' | 'ended' | 'archived') => ({ name, slug: name.toLowerCase().replace(/ /g, '-'), effective_state })
const all = [ev('Old Cup', 'ended'), ev('Draft One', 'draft'), ev('Live CTF', 'live'), ev('Soon Cup', 'registration_open'), ev('Paused CTF', 'paused'), ev('Gone', 'archived')]

describe('event groups', () => {
  it('put a paused event with the live ones and an archived one with the finished', () => {
    expect(groupOf(ev('a', 'paused'))).toBe('live')
    expect(groupOf(ev('a', 'archived'))).toBe('finished')
    expect(countByGroup(all)).toEqual({ all: 6, live: 2, upcoming: 1, draft: 1, finished: 2 })
  })
  it('list live events first, then upcoming, drafts and finished, keeping the given order inside a group', () => {
    expect(filterEvents(all, 'all', '').map((e) => e.name)).toEqual(['Live CTF', 'Paused CTF', 'Soon Cup', 'Draft One', 'Old Cup', 'Gone'])
  })
  it('narrow by group and by name or address, ignoring case and spaces around', () => {
    expect(filterEvents(all, 'draft', '').map((e) => e.name)).toEqual(['Draft One'])
    expect(filterEvents(all, 'all', '  ctf ').map((e) => e.name)).toEqual(['Live CTF', 'Paused CTF'])
    expect(filterEvents(all, 'live', 'cup')).toEqual([])
    expect(filterEvents(all, 'all', 'soon-cup').map((e) => e.name)).toEqual(['Soon Cup'])
  })
})
