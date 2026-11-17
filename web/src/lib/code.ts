// Words and small rules for coding challenges, shared by the player's panel and the author's tester.

import type { CaseVerdict, CodeCaseResult, CodeConfig, CodeResult } from '../api/platformTypes'

export const VERDICT_LABEL: Record<CaseVerdict, string> = {
  accepted: 'Passed',
  wrong_answer: 'Wrong answer',
  time_limit: 'Too slow',
  memory_limit: 'Out of memory',
  runtime_error: 'Crashed',
  output_limit: 'Too much output',
  skipped: 'Not run',
}

export const passed = (c: Pick<CodeCaseResult, 'verdict'>): boolean => c.verdict === 'accepted'

/** One line on how a run went. */
export function summary(r: CodeResult): string {
  if (r.compile_error !== null) return 'It did not compile'
  if (r.verdict === 'correct') return r.points ? `All ${r.total} tests passed, ${r.points} points` : `All ${r.total} tests passed`
  return `${r.passed} of ${r.total} tests passed`
}

/** A short id for a language, from its label: `Python 3.12` becomes `python-3-12`. */
export function languageId(label: string): string {
  const id = label.toLowerCase().replace(/[^a-z0-9+_-]+/g, '-').replace(/^-+|-+$/g, '')
  return id.slice(0, 16) || 'lang'
}

/** The same id for each language but never repeated: a clash gets a number. */
export function uniqueIds(labels: string[]): string[] {
  const used = new Set<string>()
  return labels.map((label) => {
    const base = languageId(label)
    let id = base
    for (let n = 2; used.has(id); n += 1) id = `${base.slice(0, 13)}-${n}`
    used.add(id)
    return id
  })
}

/** What would be refused, found while editing. Empty when the config can be saved. */
export function configProblem(c: Pick<CodeConfig, 'languages' | 'cases' | 'time_limit_ms' | 'memory_mb'>): string | null {
  if (c.languages.length === 0) return 'Pick at least one language.'
  if (c.languages.length > 4) return 'At most 4 languages.'
  if (c.cases.length === 0) return 'Add at least one test.'
  if (c.cases.length > 50) return 'At most 50 tests.'
  if (c.cases.filter((x) => x.sample).length > 5) return 'At most 5 tests can be shown to players.'
  if (!Number.isInteger(c.time_limit_ms) || c.time_limit_ms < 100 || c.time_limit_ms > 10_000) return 'The time limit is 100 to 10000 ms.'
  if (!Number.isInteger(c.memory_mb) || c.memory_mb < 16 || c.memory_mb > 1024) return 'The memory limit is 16 to 1024 MB.'
  return null
}
