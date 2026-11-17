import { describe, expect, it } from 'vitest'
import type { CodeResult } from '../api/platformTypes'
import { configProblem, languageId, summary, uniqueIds } from './code'

const result = (extra: Partial<CodeResult> = {}): CodeResult => ({ compile_error: null, passed: 2, total: 3, cases: [], ...extra })

describe('coding challenge words', () => {
  it('say how a run went', () => {
    expect(summary(result())).toBe('2 of 3 tests passed')
    expect(summary(result({ compile_error: 'x' }))).toBe('It did not compile')
    expect(summary(result({ passed: 3, verdict: 'correct', points: 100 }))).toBe('All 3 tests passed, 100 points')
  })
  it('make short ids from labels, never repeated', () => {
    expect(languageId('Python 3.12')).toBe('python-3-12')
    expect(languageId('C++ (g++ 13)')).toBe('c++-g++-13')
    expect(languageId('!!!')).toBe('lang')
    expect(uniqueIds(['Python 3', 'Python-3', 'Rust'])).toEqual(['python-3', 'python-3-2', 'rust'])
  })
  it('find what the server would refuse', () => {
    const ok = { languages: [{ id: 'py', label: 'Py', artifact: 'a' }], cases: [{ input: '', expected: '', sample: true }], time_limit_ms: 2000, memory_mb: 128 }
    expect(configProblem(ok)).toBeNull()
    expect(configProblem({ ...ok, languages: [] })).toMatch(/language/)
    expect(configProblem({ ...ok, cases: [] })).toMatch(/test/)
    expect(configProblem({ ...ok, cases: Array.from({ length: 6 }, () => ({ input: '', expected: '', sample: true })) })).toMatch(/shown/)
    expect(configProblem({ ...ok, time_limit_ms: 50 })).toMatch(/time/)
    expect(configProblem({ ...ok, memory_mb: 4096 })).toMatch(/memory/)
  })
})
