// Pure helpers for dynamic and multi flag challenges: the same rules the server applies, so the
// form can say what would be refused before Save, and show what a player will get.

export const MAX_FLAGS = 8
export const MIN_RANDOM = 4
export const MAX_RANDOM = 32
export const DEFAULT_RANDOM = 8
export const MAX_WEIGHT = 1000
export const MAX_FLAG_LENGTH = 500

export interface FlagRowDraft {
  name: string
  label: string
  /** The whole flag when static, the prefix when dynamic. */
  value: string
  randomLen: string
  closing: string
  weight: string
}

export const blankRow = (name: string, closing = ''): FlagRowDraft => ({
  name,
  label: '',
  value: '',
  randomLen: String(DEFAULT_RANDOM),
  closing,
  weight: '1',
})

/** Rows for a number of flags: keeps what was typed, adds blank rows, drops the extra ones. */
export function resizeRows(rows: FlagRowDraft[], count: number, closing: string): FlagRowDraft[] {
  const wanted = Math.min(Math.max(count, 1), MAX_FLAGS)
  const next = rows.slice(0, wanted)
  const taken = new Set(next.map((r) => r.name))
  while (next.length < wanted) {
    let n = next.length + 1
    while (taken.has(`flag${n}`)) n += 1
    const name = `flag${n}`
    taken.add(name)
    next.push(blankRow(name, closing))
  }
  return next
}

/** A made-up flag in the shape a player will get, for the live example. */
export function exampleFlag(prefix: string, randomLen: number, closing: string): string {
  const len = Math.min(Math.max(Number.isFinite(randomLen) ? randomLen : DEFAULT_RANDOM, MIN_RANDOM), MAX_RANDOM)
  const digits = '3fa91c2e7d4b06858a1f9c3e72b4d06f'
  return `${prefix}${digits.slice(0, len)}${closing}`
}

/** Splits total points by weight. Always adds up to the total; the remainder goes to the heaviest flag (the later one on a tie). */
export function splitPoints(total: number, weights: number[]): number[] {
  if (weights.length === 0) return []
  const t = Math.max(0, Math.trunc(total))
  const w = weights.map((x) => Math.max(1, Math.trunc(x) || 1))
  const sum = w.reduce((a, b) => a + b, 0)
  const parts = w.map((x) => Math.floor((t * x) / sum))
  const left = t - parts.reduce((a, b) => a + b, 0)
  let heaviest = 0
  w.forEach((x, i) => {
    if (x >= w[heaviest]) heaviest = i
  })
  parts[heaviest] += left
  return parts
}

const NAME = /^[a-z0-9_]{1,32}$/

export interface RowProblem {
  name?: string
  value?: string
  randomLen?: string
  closing?: string
  weight?: string
}

/** What the server would refuse for one row. */
export function rowProblems(row: FlagRowDraft, mode: 'static' | 'dynamic', prefix: string, suffix: string, caseSensitive: boolean, names: string[]): RowProblem {
  const p: RowProblem = {}
  const fold = (s: string) => (caseSensitive ? s : s.toLowerCase())
  if (!NAME.test(row.name)) p.name = 'Lowercase letters, digits and _, up to 32.'
  else if (names.filter((n) => n === row.name).length > 1) p.name = 'Each flag needs its own name.'
  const w = Number(row.weight)
  if (!Number.isInteger(w) || w < 1 || w > MAX_WEIGHT) p.weight = '1 to 1000.'
  if (row.value.trim() === '') {
    p.value = mode === 'dynamic' ? 'Enter the start of the flag.' : 'Enter the flag.'
    return p
  }
  if (mode === 'dynamic') {
    const len = Number(row.randomLen)
    if (!Number.isInteger(len) || len < MIN_RANDOM || len > MAX_RANDOM) p.randomLen = `${MIN_RANDOM} to ${MAX_RANDOM}.`
    if (prefix && !fold(row.value).startsWith(fold(prefix))) p.value = `It must start with the accepted prefix ${prefix}.`
    if (suffix && !fold(row.closing).endsWith(fold(suffix))) p.closing = `It must end with the accepted suffix ${suffix}.`
    if (!p.randomLen && row.value.length + len + row.closing.length > MAX_FLAG_LENGTH) p.value = 'The whole flag is at most 500 characters.'
  } else {
    if (row.value.length > MAX_FLAG_LENGTH) p.value = 'At most 500 characters.'
    else if (prefix && !fold(row.value).startsWith(fold(prefix))) p.value = `It must start with the accepted prefix ${prefix}, or nobody can submit it.`
    else if (suffix && !fold(row.value).endsWith(fold(suffix))) p.value = `It must end with the accepted suffix ${suffix}, or nobody can submit it.`
  }
  return p
}

/** The `inject` block the lab host's manifest needs for these flags, to copy into it. */
export function manifestSkeleton(rows: { name: string }[], responder: boolean): string {
  if (responder) {
    return JSON.stringify({ verifier: { definition: 'my-challenge', port: 8080 } }, null, 2)
  }
  const flags = rows.map((r) => ({ name: r.name, env: `FLAG_${r.name.toUpperCase()}`, path: `/path/to/${r.name}.txt` }))
  return JSON.stringify({ inject: { user: 'root', ssh_key: '/etc/kymo-lab/keys/my-box.key', flags } }, null, 2)
}

/** A progress count such as 1/2, shown only when there is more than one flag. */
export function flagProgress(found: number, total: number): string | null {
  return total > 1 ? `${found}/${total}` : null
}
