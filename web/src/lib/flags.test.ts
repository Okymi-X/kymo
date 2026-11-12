import { describe, expect, it } from 'vitest'
import { blankRow, exampleFlag, flagProgress, manifestSkeleton, resizeRows, rowProblems, splitPoints } from './flags'

describe('splitPoints', () => {
  it('always adds up to the total and gives the remainder to the heaviest flag', () => {
    expect(splitPoints(100, [1, 3])).toEqual([25, 75])
    expect(splitPoints(100, [1, 1, 1])).toEqual([33, 33, 34])
    expect(splitPoints(10, [1, 2])).toEqual([3, 7])
    for (const total of [0, 1, 7, 100, 999]) {
      for (const weights of [[1], [1, 2], [5, 3, 2], [1, 1, 1, 1, 1, 1, 1, 1]]) {
        expect(splitPoints(total, weights).reduce((a, b) => a + b, 0)).toBe(total)
      }
    }
  })
  it('copes with odd input', () => {
    expect(splitPoints(-5, [1, 1])).toEqual([0, 0])
    expect(splitPoints(10, [])).toEqual([])
    expect(splitPoints(100, [0, 0])).toEqual([50, 50])
  })
})

describe('resizeRows', () => {
  it('keeps what was typed, adds blank named rows and never goes past 8 or below 1', () => {
    const rows = [{ ...blankRow('user'), value: 'NDH{u_' }]
    const more = resizeRows(rows, 3, '}')
    expect(more.map((r) => r.name)).toEqual(['user', 'flag2', 'flag3'])
    expect(more[0].value).toBe('NDH{u_')
    expect(more[1].closing).toBe('}')
    expect(resizeRows(more, 1, '}')).toHaveLength(1)
    expect(resizeRows(more, 20, '}')).toHaveLength(8)
    expect(resizeRows(more, 0, '}')).toHaveLength(1)
  })
})

describe('exampleFlag', () => {
  it('builds the shape a player gets, with the chosen length', () => {
    expect(exampleFlag('NDH{box_', 8, '}')).toBe('NDH{box_3fa91c2e}')
    expect(exampleFlag('a', 99, 'z').length).toBe(1 + 32 + 1)
    expect(exampleFlag('a', 1, 'z').length).toBe(1 + 4 + 1)
  })
})

describe('rowProblems', () => {
  const ok = { ...blankRow('user'), value: 'NDH{u_', closing: '}' }
  it('accepts a good dynamic row and names each problem otherwise', () => {
    expect(rowProblems(ok, 'dynamic', 'NDH{', '}', true, ['user'])).toEqual({})
    expect(rowProblems({ ...ok, name: 'Bad' }, 'dynamic', 'NDH{', '}', true, ['Bad']).name).toBeDefined()
    expect(rowProblems(ok, 'dynamic', 'NDH{', '}', true, ['user', 'user']).name).toBeDefined()
    expect(rowProblems({ ...ok, value: 'X{u_' }, 'dynamic', 'NDH{', '}', true, ['user']).value).toBeDefined()
    expect(rowProblems({ ...ok, closing: ')' }, 'dynamic', 'NDH{', '}', true, ['user']).closing).toBeDefined()
    expect(rowProblems({ ...ok, randomLen: '3' }, 'dynamic', 'NDH{', '}', true, ['user']).randomLen).toBeDefined()
    expect(rowProblems({ ...ok, weight: '0' }, 'dynamic', 'NDH{', '}', true, ['user']).weight).toBeDefined()
    expect(rowProblems({ ...ok, value: ' ' }, 'dynamic', 'NDH{', '}', true, ['user']).value).toBeDefined()
  })
  it('follows the case rule and checks a static flag against the accepted shape', () => {
    expect(rowProblems({ ...ok, value: 'ndh{u_' }, 'dynamic', 'NDH{', '}', false, ['user'])).toEqual({})
    expect(rowProblems({ ...ok, value: 'ndh{u_' }, 'dynamic', 'NDH{', '}', true, ['user']).value).toBeDefined()
    const fixed = { ...blankRow('one'), value: 'NDH{one}' }
    expect(rowProblems(fixed, 'static', 'NDH{', '}', true, ['one'])).toEqual({})
    expect(rowProblems({ ...fixed, value: 'one' }, 'static', 'NDH{', '}', true, ['one']).value).toBeDefined()
  })
})

describe('manifestSkeleton and flagProgress', () => {
  it('lists the flag names for the lab manifest, or the verifier block for a Responder', () => {
    const text = manifestSkeleton([{ name: 'user' }, { name: 'root' }], false)
    expect(text).toContain('"name": "user"')
    expect(text).toContain('FLAG_ROOT')
    expect(manifestSkeleton([{ name: 'flag' }], true)).toContain('"verifier"')
  })
  it('shows progress only for several flags', () => {
    expect(flagProgress(0, 1)).toBeNull()
    expect(flagProgress(1, 2)).toBe('1/2')
  })
})
