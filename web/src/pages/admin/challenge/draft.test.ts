import { describe, expect, it } from 'vitest'
import type { AdminChallenge } from '../../../api/platformTypes'
import { applyResponder, blank, fromChallenge, isMulti, parentChoices, toBody, toCodeConfig, usesRows, validate, type Draft } from './draft'

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
  it('needs no instance (the flag goes into template files), but a prefix inside the accepted one and a closing inside the accepted suffix', () => {
    expect(validate({ ...dyn, requiresInstance: false })).toEqual({})
    expect(validate({ ...dyn, requiresInstance: false, responder: true }).requiresInstance).toBeDefined()
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
      id: 1, event_id: 1, category_id: 2, lab_host_id: null, title: 'Box', description: '', author: '', scoring: { kind: 'static', points: 100 },
      flag: { kind: 'static', value: 'x' }, flag_prefix: 'KYMO{', flag_suffix: '}', case_sensitive: true, max_attempts: null, max_starts: null,
      state: 'draft', release_at: null, retire_at: null, position: 0, requires_instance: true, instance_config: null, kind: 'flag',
      created_at: 0, flag_mode: 'dynamic', multi_flag: true, responder: false, requires_challenge_id: null, solves: 0, hints: [], attachments: [],
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
    const d = fromChallenge({ ...({ kind: 'flag', scoring: { kind: 'static', points: 1 }, flag: { kind: 'static', value: 'a' }, flag_prefix: '', flag_suffix: '}', case_sensitive: true, max_attempts: null, max_starts: null, release_at: null, retire_at: null, position: 0, requires_instance: false, instance_config: null, category_id: 1, title: 't', description: '' } as unknown as AdminChallenge) })
    expect(d.flagMode).toBe('static')
    expect(d.flagCount).toBe('1')
    expect(d.rows).toHaveLength(1)
  })
})

describe('a coding challenge', () => {
  const code = base({ kind: 'code', requiresInstance: false, flagValue: '', code: { languages: [{ id: 'python', label: 'Python 3', artifact: 'judge/py:1' }], cases: [{ input: '1 2', expected: '3', sample: true }], time: '2000', memory: '128', compare: 'lines' } })
  it('sends no flag, no instance and no flag settings', () => {
    const body = toBody(code)
    expect(body.kind).toBe('code')
    expect(body.flag).toBeUndefined()
    expect(body.flags).toBeUndefined()
    expect(body.requires_instance).toBe(false)
    expect(body.instance_config).toBeNull()
    expect(validate(code)).toEqual({})
  })
  it('keeps dynamic points, unlike a multi-question challenge', () => {
    expect(toBody({ ...code, scoring: 'dynamic' }).scoring.kind).toBe('dynamic')
  })
  it('turns the tests into the config the server wants', () => {
    expect(toCodeConfig(code)).toEqual({ languages: code.code.languages, cases: code.code.cases, time_limit_ms: 2000, memory_mb: 128, compare: 'lines' })
  })
  it('needs a language, a test and sane limits', () => {
    expect(validate({ ...code, code: { ...code.code, languages: [] } }).code).toMatch(/language/)
    expect(validate({ ...code, code: { ...code.code, cases: [] } }).code).toMatch(/test/)
    expect(validate({ ...code, code: { ...code.code, time: '50' } }).code).toMatch(/time/)
  })
  it('is read back from a saved challenge', () => {
    const saved = { code: { languages: code.code.languages, cases: code.code.cases, time_limit_ms: 3000, memory_mb: 64, compare: 'tokens' as const } }
    const fixture = { scoring: { kind: 'static', points: 1 }, flag: { kind: 'static', value: 'code' }, flag_prefix: '', flag_suffix: '', case_sensitive: true, max_attempts: null, max_starts: null, release_at: null, retire_at: null, position: 0, requires_instance: false, instance_config: null, category_id: 1, title: 't', description: '' }
    const d = fromChallenge({ ...fixture, kind: 'code', ...saved } as unknown as AdminChallenge)
    expect(d.kind).toBe('code')
    expect(d.code.time).toBe('3000')
    expect(d.code.compare).toBe('tokens')
  })
})

describe('turning Responder on', () => {
  const start = base({ requiresInstance: true, prefix: 'KYMO{', suffix: '}', flagMode: 'static', flagCount: '3', rows: [row('', ''), row('b', 'x'), row('c', 'y')] })
  it('makes the flag per player, single, and valid on the accepted prefix and suffix', () => {
    const d = applyResponder(start, true)
    expect(d.responder && d.flagMode === 'dynamic' && d.flagCount === '1').toBe(true)
    expect(d.rows).toHaveLength(1)
    expect(d.rows[0]).toMatchObject({ name: 'flag', value: 'KYMO{', closing: '}' })
    expect(validate(d)).toEqual({})
    expect(toBody(d).flag_mode).toBe('dynamic')
    expect(toBody(d).responder).toBe(true)
  })
  it('keeps a flag the author already wrote', () => {
    const d = applyResponder({ ...start, rows: [row('flag', 'KYMO{fix_')] }, true)
    expect(d.rows[0]?.value).toBe('KYMO{fix_')
  })
  it('turns off without touching anything else', () => {
    const on = applyResponder(start, true)
    expect(applyResponder(on, false)).toEqual({ ...on, responder: false })
  })
})

describe('a challenge that requires another one', () => {
  it('is off unless chosen, and sends the parent id when it is', () => {
    expect(toBody(base()).requires_challenge_id).toBeNull()
    expect(toBody(base({ requires: '7' })).requires_challenge_id).toBe(7)
  })
  it('never offers itself or its own descendants as a parent', () => {
    const all = [
      { id: 1, title: 'Root', requires_challenge_id: null },
      { id: 2, title: 'Child', requires_challenge_id: 1 },
      { id: 3, title: 'Grandchild', requires_challenge_id: 2 },
      { id: 4, title: 'Other', requires_challenge_id: null },
    ]
    expect(parentChoices(all, 1).map((c) => c.id)).toEqual([4])
    expect(parentChoices(all, 2).map((c) => c.id)).toEqual([1, 4])
    expect(parentChoices(all, 4).map((c) => c.id)).toEqual([1, 2, 3])
    expect(parentChoices(all, null).map((c) => c.id)).toEqual([1, 2, 3, 4])
  })
})
