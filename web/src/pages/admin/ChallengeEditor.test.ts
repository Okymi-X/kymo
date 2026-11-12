import { describe, expect, it } from 'vitest'
import type { AdminChallenge } from '../../api/platformTypes'
import { blank, fromChallenge, isMulti, toBody, usesRows, validate, type Draft } from './ChallengeEditor'

const base = (extra: Partial<Draft> = {}): Draft => ({
  ...blank(1),
  title: 'Box',
  prefix: 'KYMO{',
  suffix: '}',
  flagValue: 'KYMO{x}',
  requiresInstance: true,
  image: 'ctf/web:1',
  ...extra,
})

const row = (name: string, value: string, extra: Record<string, string> = {}) => ({ name, label: '', value, randomLen: '8', closing: '}', weight: '1', ...extra })

describe('a plain static challenge', () => {
  it('is unchanged: one flag field, no rows', () => {
    const d = base()
    expect(usesRows(d)).toBe(false)
    expect(isMulti(d)).toBe(false)
    const body = toBody(d)
    expect(body.flag).toEqual({ kind: 'static', value: 'KYMO{x}' })
    expect(body.flag_mode).toBe('static')
    expect(body.responder).toBe(false)
    expect(body.flags).toEqual([])
    expect(validate(d)).toEqual({})
  })
})

describe('a dynamic flag', () => {
  const dyn = base({ flagMode: 'dynamic', rows: [row('flag', 'KYMO{box_')] })
  it('sends the rows instead of a flag, with the random length and closing text', () => {
    expect(usesRows(dyn)).toBe(true)
    const body = toBody(dyn)
    expect(body.flag).toBeUndefined()
    expect(body.flag_mode).toBe('dynamic')
    expect(body.flags).toEqual([{ name: 'flag', label: '', value: 'KYMO{box_', random_len: 8, closing: '}', weight: 1 }])
    expect(validate(dyn)).toEqual({})
  })
  it('needs an instance, a prefix inside the accepted one, and a closing inside the accepted suffix', () => {
    expect(validate({ ...dyn, requiresInstance: false }).requiresInstance).toBeDefined()
    expect(validate({ ...dyn, rows: [row('flag', 'OTHER{')] }).rows).toBeDefined()
    expect(validate({ ...dyn, rows: [row('flag', 'KYMO{box_', { closing: ')' })] }).rows).toBeDefined()
    expect(validate({ ...dyn, rows: [row('flag', 'KYMO{box_', { randomLen: '40' })] }).rows).toBeDefined()
  })
})

describe('several flags', () => {
  const two = base({ flagMode: 'dynamic', flagCount: '2', rows: [row('user', 'KYMO{u_', { weight: '1' }), row('root', 'KYMO{r_', { weight: '3' })] })
  it('forces fixed points and sends both rows with their weights', () => {
    expect(isMulti(two)).toBe(true)
    const body = toBody({ ...two, scoring: 'dynamic' })
    expect(body.scoring).toEqual({ kind: 'static', points: 100 })
    expect(body.flags?.map((f) => [f.name, f.weight])).toEqual([['user', 1], ['root', 3]])
    expect(validate(two)).toEqual({})
  })
  it('refuses repeated names and bad counts', () => {
    expect(validate({ ...two, rows: [row('a', 'KYMO{u_'), row('a', 'KYMO{r_')] }).rows).toBeDefined()
    expect(validate({ ...two, flagCount: '9' }).flagCount).toBeDefined()
    expect(validate({ ...two, flagCount: '0' }).flagCount).toBeDefined()
  })
  it('ignores rows past the count', () => {
    const one = { ...two, flagCount: '1' }
    expect(toBody(one).flags).toHaveLength(1)
  })
  it('works with static flags and no instance', () => {
    const fixed = base({ requiresInstance: false, flagCount: '2', rows: [row('one', 'KYMO{one}'), row('two', 'KYMO{two}')] })
    expect(toBody(fixed).flags?.[0]).toEqual({ name: 'one', label: '', value: 'KYMO{one}', weight: 1 })
    expect(validate(fixed)).toEqual({})
  })
})

describe('a Responder', () => {
  it('needs an instance and exactly one flag', () => {
    const r = base({ responder: true })
    expect(validate(r)).toEqual({})
    expect(toBody(r).responder).toBe(true)
    expect(validate({ ...r, requiresInstance: false }).requiresInstance).toBeDefined()
    expect(validate({ ...r, flagCount: '2', rows: [row('a', 'KYMO{a}'), row('b', 'KYMO{b}')] }).flagCount).toBeDefined()
  })
})

describe('fromChallenge', () => {
  it('reads the flag settings back into the form', () => {
    const c = {
      id: 1, event_id: 1, category_id: 2, title: 'Box', description: '', scoring: { kind: 'static', points: 100 },
      flag: { kind: 'static', value: 'x' }, flag_prefix: 'KYMO{', flag_suffix: '}', case_sensitive: true, max_attempts: null,
      state: 'draft', release_at: null, retire_at: null, position: 0, requires_instance: true, instance_config: null, kind: 'flag',
      created_at: 0, flag_mode: 'dynamic', multi_flag: true, responder: false, solves: 0, hints: [], attachments: [],
      flags: [
        { id: 1, challenge_id: 1, position: 0, name: 'user', label: 'User', value: 'KYMO{u_', random_len: 8, closing: '}', weight: 1 },
        { id: 2, challenge_id: 1, position: 1, name: 'root', label: 'Root', value: 'KYMO{r_', random_len: 12, closing: '}', weight: 3 },
      ],
    } as AdminChallenge
    const d = fromChallenge(c)
    expect(d.flagMode).toBe('dynamic')
    expect(d.flagCount).toBe('2')
    expect(d.rows[1]).toEqual({ name: 'root', label: 'Root', value: 'KYMO{r_', randomLen: '12', closing: '}', weight: '3' })
  })
  it('gives an older challenge one blank row and the static mode', () => {
    const d = fromChallenge({ ...({ kind: 'flag', scoring: { kind: 'static', points: 1 }, flag: { kind: 'static', value: 'a' }, flag_prefix: '', flag_suffix: '}', case_sensitive: true, max_attempts: null, release_at: null, retire_at: null, position: 0, requires_instance: false, instance_config: null, category_id: 1, title: 't', description: '' } as unknown as AdminChallenge) })
    expect(d.flagMode).toBe('static')
    expect(d.flagCount).toBe('1')
    expect(d.rows).toHaveLength(1)
  })
})
