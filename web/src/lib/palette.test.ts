import { describe, expect, it } from 'vitest'
import { filterCommands, type Command } from './palette'

const cmd = (id: string, label: string): Command => ({ id, label, run: () => {} })
const all = [cmd('a', 'Spring Cup 2026'), cmd('b', 'Autumn Cup'), cmd('c', 'API tokens')]

describe('filterCommands', () => {
  it('returns everything for an empty query', () => {
    expect(filterCommands(all, '  ')).toHaveLength(3)
  })

  it('requires every word to match, in any order', () => {
    expect(filterCommands(all, 'cup spring').map((c) => c.id)).toEqual(['a'])
  })

  it('ignores case', () => {
    expect(filterCommands(all, 'API').map((c) => c.id)).toEqual(['c'])
  })

  it('returns nothing when no label matches', () => {
    expect(filterCommands(all, 'zzz')).toEqual([])
  })
})
