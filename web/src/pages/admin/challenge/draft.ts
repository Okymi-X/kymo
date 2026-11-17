import type { AdminChallenge, ChallengeBody, CodeCase, CodeConfig, CodeLanguage, CompareMode, FlagInput, FlagSpec, Scoring } from '../../../api/platformTypes'
import { configProblem } from '../../../lib/code'
import { blankRow, MAX_FLAGS, resizeRows, rowProblems, type FlagRowDraft } from '../../../lib/flags'
import { fromLocalInput, toLocalInput } from '../../../lib/platform'

/** What the author edits for a coding challenge. Numbers stay text until they are sent. */
export interface CodeDraft {
  languages: CodeLanguage[]
  cases: CodeCase[]
  time: string
  memory: string
  compare: CompareMode
}

export const blankCode = (): CodeDraft => ({ languages: [], cases: [{ input: '', expected: '', sample: true }], time: '2000', memory: '128', compare: 'lines' })

export function toCodeConfig(d: Draft): CodeConfig {
  return { languages: d.code.languages, cases: d.code.cases, time_limit_ms: Math.trunc(Number(d.code.time)), memory_mb: Math.trunc(Number(d.code.memory)), compare: d.code.compare }
}

export interface Draft {
  /** `flag` (also on a container), `dfir` (answered question by question) or `code` (a program run against tests). */
  kind: 'flag' | 'dfir' | 'code'
  title: string
  description: string
  category_id: number
  /** The languages, tests and limits when `kind` is `code`. */
  code: CodeDraft
  scoring: 'static' | 'dynamic'
  points: string
  initial: string
  minimum: string
  decay: string
  curve: 'linear' | 'parabolic' | 'logarithmic'
  flagKind: FlagSpec['kind']
  flagValue: string
  prefix: string
  suffix: string
  caseSensitive: boolean
  maxAttempts: string
  release: string
  retire: string
  position: string
  /** `static` (the flag above) or `dynamic` (generated for every instance run). */
  flagMode: 'static' | 'dynamic'
  /** How many flags; more than one makes a multi flag challenge. */
  flagCount: string
  /** The flags of a dynamic or multi flag challenge. */
  rows: FlagRowDraft[]
  /** Players fix the machine and a Verifier gives the flag. */
  responder: boolean
  requiresInstance: boolean
  /** A virtual machine rather than a container; only decides what the form offers, the lab host's manifest decides the runtime. */
  machine: boolean
  /** The lab host this challenge runs on; empty follows the event. */
  labHost: string
  image: string
  port: string
  cpu: string
  memory: string
  pids: string
  ttl: string
}

export const blank = (category: number): Draft => ({
  kind: 'flag',
  title: '',
  description: '',
  category_id: category,
  code: blankCode(),
  scoring: 'static',
  points: '100',
  initial: '500',
  minimum: '100',
  decay: '20',
  curve: 'parabolic',
  flagKind: 'static',
  flagValue: '',
  prefix: '',
  suffix: '',
  caseSensitive: true,
  maxAttempts: '',
  release: '',
  retire: '',
  position: '0',
  flagMode: 'static',
  flagCount: '1',
  rows: [blankRow('flag')],
  responder: false,
  requiresInstance: false,
  machine: false,
  labHost: '',
  image: '',
  port: '8000',
  cpu: '500',
  memory: '256',
  pids: '128',
  ttl: '1800',
})

function flagText(f: FlagSpec): string {
  return f.kind === 'static' ? f.value : f.kind === 'regex' ? f.pattern : f.expected
}

export function fromChallenge(c: AdminChallenge): Draft {
  const cfg = c.instance_config
  return {
    ...blank(c.category_id),
    kind: c.kind,
    code: c.code ? { languages: c.code.languages, cases: c.code.cases, time: String(c.code.time_limit_ms), memory: String(c.code.memory_mb), compare: c.code.compare } : blankCode(),
    title: c.title,
    description: c.description,
    scoring: c.scoring.kind,
    points: c.scoring.kind === 'static' ? String(c.scoring.points) : '100',
    initial: c.scoring.kind === 'dynamic' ? String(c.scoring.initial) : '500',
    minimum: c.scoring.kind === 'dynamic' ? String(c.scoring.minimum) : '100',
    decay: c.scoring.kind === 'dynamic' ? String(c.scoring.decay) : '20',
    curve: c.scoring.kind === 'dynamic' ? c.scoring.curve : 'parabolic',
    flagKind: c.flag.kind,
    flagValue: flagText(c.flag),
    prefix: c.flag_prefix,
    suffix: c.flag_suffix,
    caseSensitive: c.case_sensitive,
    maxAttempts: c.max_attempts === null ? '' : String(c.max_attempts),
    release: toLocalInput(c.release_at),
    retire: toLocalInput(c.retire_at),
    position: String(c.position),
    flagMode: c.flag_mode ?? 'static',
    flagCount: String(Math.max(1, c.flags?.length ?? 0)),
    rows:
      c.flags && c.flags.length > 0
        ? c.flags.map((f) => ({ name: f.name, label: f.label, value: f.value, randomLen: String(f.random_len), closing: f.closing, weight: String(f.weight) }))
        : [blankRow('flag', c.flag_suffix)],
    responder: c.responder ?? false,
    requiresInstance: c.requires_instance,
    labHost: c.lab_host_id === null ? '' : String(c.lab_host_id),
    image: cfg?.image ?? '',
    port: String(cfg?.port ?? 8000),
    cpu: String(cfg?.cpu_millis ?? 500),
    memory: String(cfg?.memory_mb ?? 256),
    pids: String(cfg?.pids ?? 128),
    ttl: String(cfg?.ttl_secs ?? 1800),
  }
}

export const int = (s: string) => Math.trunc(Number(s))

/** The flags come from the rows, not from the single flag field. */
export const usesRows = (d: Draft): boolean => d.kind === 'flag' && (d.flagMode === 'dynamic' || int(d.flagCount) > 1)
/** Several flags: static points, split by weight. */
export const isMulti = (d: Draft): boolean => d.kind === 'flag' && int(d.flagCount) > 1

/** Turns the form into the API body; the server validates every field again. */
export function toBody(d: Draft, fixedPoints = false): ChallengeBody {
  const dfir = d.kind === 'dfir'
  const noFlag = d.kind !== 'flag'
  const scoring: Scoring =
    d.scoring === 'static' || fixedPoints || dfir || isMulti(d)
      ? { kind: 'static', points: int(d.points) }
      : { kind: 'dynamic', initial: int(d.initial), minimum: int(d.minimum), decay: int(d.decay), curve: d.curve }
  const flag: FlagSpec =
    d.flagKind === 'static' ? { kind: 'static', value: d.flagValue } : d.flagKind === 'regex' ? { kind: 'regex', pattern: d.flagValue } : { kind: 'per_team', expected: d.flagValue }
  const rows = usesRows(d)
  const flags: FlagInput[] = rows
    ? d.rows.slice(0, Math.max(1, int(d.flagCount))).map((r) => ({
        name: r.name.trim(),
        label: r.label.trim(),
        value: r.value,
        ...(d.flagMode === 'dynamic' ? { random_len: int(r.randomLen), closing: r.closing } : {}),
        weight: int(r.weight) || 1,
      }))
    : []
  return {
    category_id: d.category_id,
    title: d.title.trim(),
    description: d.description,
    scoring,
    kind: d.kind,
    ...(noFlag || rows ? {} : { flag }),
    ...(noFlag ? {} : { flag_mode: d.flagMode, responder: d.responder && d.requiresInstance, flags }),
    flag_prefix: d.prefix,
    flag_suffix: d.suffix,
    case_sensitive: d.caseSensitive,
    max_attempts: d.maxAttempts.trim() === '' ? null : int(d.maxAttempts),
    release_at: fromLocalInput(d.release),
    retire_at: fromLocalInput(d.retire),
    position: int(d.position) || 0,
    requires_instance: !noFlag && d.requiresInstance,
    instance_config: !noFlag && d.requiresInstance
      ? { image: d.image.trim(), port: int(d.port), cpu_millis: int(d.cpu), memory_mb: int(d.memory), pids: int(d.pids), ttl_secs: int(d.ttl) }
      : null,
  }
}

export type Problems = Partial<Record<keyof Draft, string>>

export const inRange = (value: string, min: number, max: number) => {
  const n = Number(value)
  return value.trim() !== '' && Number.isInteger(n) && n >= min && n <= max
}

/** What would be refused, found before the request is sent so the form can say why Save is off. */
export function validate(d: Draft, fixedPoints = false): Problems {
  const p: Problems = {}
  const dfir = d.kind === 'dfir'
  // A flag, an instance and the rest of the flag form belong to a plain flag challenge only.
  const noFlag = d.kind !== 'flag'
  if (d.title.trim() === '') p.title = 'Give the challenge a title.'
  if (d.category_id === 0) p.category_id = 'Add a category first.'
  if (d.scoring === 'static' || fixedPoints || dfir || isMulti(d)) {
    if (!inRange(d.points, 0, 100000)) p.points = 'Points are a whole number, 0 or more.'
  } else {
    if (!inRange(d.initial, 1, 100000)) p.initial = 'A whole number, 1 or more.'
    if (!inRange(d.minimum, 0, 100000)) p.minimum = 'A whole number, 0 or more.'
    else if (inRange(d.initial, 1, 100000) && Number(d.minimum) > Number(d.initial)) p.minimum = 'The floor cannot be above the start.'
    if (!inRange(d.decay, 1, 100000)) p.decay = 'A whole number, 1 or more.'
  }
  const rows = usesRows(d)
  // A dynamic flag without an instance goes into template files; only a Responder needs a machine.
  if (!noFlag && d.responder && !d.requiresInstance) p.requiresInstance = 'A Responder needs an instance.'
  const count = int(d.flagCount)
  if (!noFlag && (!Number.isInteger(count) || count < 1 || count > MAX_FLAGS)) p.flagCount = `1 to ${MAX_FLAGS}.`
  if (d.responder && count > 1) p.flagCount = 'A Responder has exactly one flag.'
  if (noFlag) {
    // The answers live in the questions, or in the tests.
  } else if (rows) {
    const used = d.rows.slice(0, Math.max(1, Math.min(count, MAX_FLAGS)))
    const names = used.map((r) => r.name)
    const bad = used.map((r) => rowProblems(r, d.flagMode, d.prefix, d.suffix, d.caseSensitive, names))
    const first = bad.map((b) => Object.values(b)[0]).find(Boolean)
    if (first) p.rows = first
  } else if (d.flagValue === '') {
    p.flagValue = 'Enter the flag.'
  } else if (d.flagKind === 'static' || d.flagKind === 'per_team') {
    // A flag that does not match the accepted format could never be submitted.
    const shown = d.caseSensitive ? d.flagValue : d.flagValue.toLowerCase()
    const pre = d.caseSensitive ? d.prefix : d.prefix.toLowerCase()
    const suf = d.caseSensitive ? d.suffix : d.suffix.toLowerCase()
    if (d.prefix && !shown.startsWith(pre)) p.flagValue = `The flag must start with the accepted prefix ${d.prefix}, or nobody can submit it.`
    else if (d.suffix && !shown.endsWith(suf)) p.flagValue = `The flag must end with the accepted suffix ${d.suffix}, or nobody can submit it.`
  } else {
    try {
      new RegExp(d.flagValue)
    } catch {
      p.flagValue = 'This is not a valid pattern.'
    }
  }
  if (d.kind === 'code') {
    const problem = configProblem(toCodeConfig(d))
    if (problem) p.code = problem
  }
  if (d.maxAttempts.trim() !== '' && !inRange(d.maxAttempts, 1, 10000)) p.maxAttempts = 'A whole number from 1, or empty for no limit.'
  const rel = fromLocalInput(d.release)
  const ret = fromLocalInput(d.retire)
  if (rel !== null && ret !== null && rel >= ret) p.retire = 'It must retire after it is released.'
  if (d.requiresInstance && !noFlag) {
    if (d.image.trim() === '') p.image = 'Name the image or machine to start.'
    if (!inRange(d.port, 1, 65535)) p.port = '1 to 65535.'
    if (!inRange(d.cpu, 1, 4000)) p.cpu = '1 to 4000.'
    if (!inRange(d.memory, 1, 2048)) p.memory = '1 to 2048.'
    if (!inRange(d.pids, 1, 512)) p.pids = '1 to 512.'
    if (!inRange(d.ttl, 1, 14400)) p.ttl = '1 to 14400 (4 hours).'
  }
  return p
}

/** One sentence that says what the dynamic settings mean in practice. */
export function dynamicSummary(d: Draft): string {
  const start = d.initial || '?'
  const floor = d.minimum || '?'
  const n = d.decay || '?'
  if (d.curve === 'linear') return `The first solver gets ${start}. Each solve after that takes ${n} points off, down to ${floor}.`
  return `The first solver gets ${start}, falling ${d.curve === 'parabolic' ? 'faster at first' : 'quickly then slowly'} to ${floor} after about ${n} solves. Everyone's points update as more teams solve it.`
}

/** Turns Responder on or off. On, it takes one flag, and that flag is per player: a static flag is the
 *  same for everyone, so the first player to pass could hand it to the rest. The flag is built on the
 *  accepted prefix and suffix, so it is valid as soon as the form says so. */
export function applyResponder(d: Draft, on: boolean): Draft {
  if (!on) return { ...d, responder: false }
  return {
    ...d,
    responder: true,
    flagCount: '1',
    flagMode: 'dynamic',
    rows: resizeRows(d.rows, 1, d.suffix).map((r) => (r.value === '' ? { ...r, name: 'flag', value: d.prefix, closing: r.closing || d.suffix } : r)),
  }
}
