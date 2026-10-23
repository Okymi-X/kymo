import { useMemo, useState, type FormEvent } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useNavigate, useSearchParams } from 'react-router'
import { api, useIsAdmin } from '../api/client'
import { useTournaments } from '../api/hooks'
import { useAppearance } from '../components/ThemeLoader'
import type { Format, Status } from '../api/types'
import { CardSkeleton, TournamentCard } from '../components/TournamentCard'
import { Button, ErrorText, Field, Input, PageTitle, Select } from '../components/ui'
import { formatLabel } from '../lib/labels'
import { DEFAULT_APPEARANCE } from '../lib/theme'

/** Keeps only the setup fields, so a full backup file can be used as a setup file too. */
function toSetup(raw: unknown) {
  const o = (raw ?? {}) as Record<string, unknown>
  const teams = Array.isArray(o.teams) ? (o.teams as Record<string, unknown>[]).map((t) => ({ name: t.name, score: t.score ?? 0 })) : []
  return { name: o.name, format: o.format, third_place: o.third_place, groups: o.groups, advance: o.advance, rounds: o.rounds, teams }
}

function CreateForm() {
  const [name, setName] = useState('')
  const [format, setFormat] = useState<Format>('single_elimination')
  const [thirdPlace, setThirdPlace] = useState(false)
  const [groups, setGroups] = useState(2)
  const [advance, setAdvance] = useState(2)
  const [rounds, setRounds] = useState(0)
  const client = useQueryClient()
  const navigate = useNavigate()
  const create = useMutation({
    mutationFn: () => api.create(name, format, { thirdPlace: format === 'single_elimination' && thirdPlace, groups, advance, rounds: format === 'swiss' ? rounds : 0 }),
    onSuccess: (t) => {
      void client.invalidateQueries({ queryKey: ['tournaments'] })
      navigate(`/t/${t.slug}`)
    },
  })
  const [importError, setImportError] = useState<string | null>(null)
  const importSetup = useMutation({
    mutationFn: (setup: unknown) => api.importSetup(setup),
    onSuccess: (t) => {
      void client.invalidateQueries({ queryKey: ['tournaments'] })
      navigate(`/t/${t.slug}`)
    },
    onMutate: () => setImportError(null),
  })
  const submit = (e: FormEvent) => {
    e.preventDefault()
    create.mutate()
  }
  return (
    <form onSubmit={submit} className="flex flex-wrap items-end gap-3 rounded-card border border-line bg-surface p-4">
      <Field label="New tournament">
        <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="Spring Cup 2026" className="w-64" maxLength={80} />
      </Field>
      <Field label="Format">
        <Select value={format} onChange={(e) => setFormat(e.target.value as Format)}>
          <option value="single_elimination">{formatLabel.single_elimination}</option>
          <option value="double_elimination">{formatLabel.double_elimination}</option>
          <option value="round_robin">{formatLabel.round_robin}</option>
          <option value="group_knockout">{formatLabel.group_knockout}</option>
          <option value="swiss">{formatLabel.swiss}</option>
        </Select>
      </Field>
      {format === 'group_knockout' && (
        <>
          <Field label="Groups">
            <Select value={groups} onChange={(e) => setGroups(Number(e.target.value))}>
              {[2, 3, 4, 5, 6, 7, 8].map((n) => (
                <option key={n} value={n}>
                  {n}
                </option>
              ))}
            </Select>
          </Field>
          <Field label="Advance per group">
            <Select value={advance} onChange={(e) => setAdvance(Number(e.target.value))}>
              {[1, 2, 3, 4].map((n) => (
                <option key={n} value={n}>
                  {n}
                </option>
              ))}
            </Select>
          </Field>
        </>
      )}
      {format === 'swiss' && (
        <Field label="Rounds">
          <Select value={rounds} onChange={(e) => setRounds(Number(e.target.value))}>
            <option value={0}>Automatic</option>
            {Array.from({ length: 12 }, (_, i) => i + 1).map((n) => (
              <option key={n} value={n}>
                {n}
              </option>
            ))}
          </Select>
        </Field>
      )}
      {format === 'single_elimination' && (
        <label className="flex h-10 items-center gap-2 text-sm">
          <input type="checkbox" checked={thirdPlace} onChange={(e) => setThirdPlace(e.target.checked)} className="size-4 accent-accent" />
          Third place match
        </label>
      )}
      <Button type="submit" variant="primary" disabled={!name.trim() || create.isPending}>
        Create
      </Button>
      <label className="inline-flex h-10 cursor-pointer items-center rounded-control border border-line px-3.5 text-sm font-medium text-muted transition-colors hover:text-ink focus-within:outline focus-within:outline-2 focus-within:outline-accent">
        Import setup
        <input
          type="file"
          accept="application/json,.json"
          aria-label="Import a tournament setup file"
          className="sr-only"
          onChange={async (e) => {
            const file = e.target.files?.[0]
            e.target.value = ''
            if (!file) return
            try {
              importSetup.mutate(toSetup(JSON.parse(await file.text())))
            } catch {
              importSetup.reset()
              setImportError('That file is not valid JSON.')
            }
          }}
        />
      </label>
      <ErrorText error={create.error ?? importSetup.error} />
      {importError && <p role="alert" className="w-full text-sm text-danger">{importError}</p>}
    </form>
  )
}

type Filter = 'all' | Status

const FILTERS: { value: Filter; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'live', label: 'Live' },
  { value: 'draft', label: 'Draft' },
  { value: 'done', label: 'Finished' },
]

function FilterBar({ value, counts, onChange }: { value: Filter; counts: Record<Filter, number>; onChange: (f: Filter) => void }) {
  return (
    <div role="group" aria-label="Filter by status" className="inline-flex gap-1 self-start rounded-full border border-line bg-surface p-1">
      {FILTERS.map((f) => (
        <button
          key={f.value}
          type="button"
          aria-pressed={value === f.value}
          onClick={() => onChange(f.value)}
          className={`rounded-full px-3.5 py-1 text-sm font-medium transition-colors ${value === f.value ? 'bg-ink text-bg' : 'text-muted hover:text-ink'}`}
        >
          {f.label} <span className="font-mono text-xs tabular-nums opacity-70">{counts[f.value]}</span>
        </button>
      ))}
    </div>
  )
}

function DemoButton() {
  const client = useQueryClient()
  const load = useMutation({
    mutationFn: api.loadDemo,
    onSuccess: () => client.invalidateQueries({ queryKey: ['tournaments'] }),
  })
  return (
    <div className="mt-4 flex flex-col items-center gap-2">
      <Button onClick={() => load.mutate()} disabled={load.isPending}>
        {load.isPending ? 'Loading samples' : 'Load sample tournaments'}
      </Button>
      <ErrorText error={load.error} />
    </div>
  )
}

export function Home() {
  const admin = useIsAdmin()
  const { data, isPending, error } = useTournaments()
  const [params, setParams] = useSearchParams()
  const requested = params.get('status')
  const filter: Filter = FILTERS.find((f) => f.value === requested)?.value ?? 'all'
  const setFilter = (f: Filter) => setParams(f === 'all' ? {} : { status: f }, { replace: true })
  const tagline = useAppearance().data?.tagline ?? DEFAULT_APPEARANCE.tagline

  const counts = useMemo(() => {
    const c: Record<Filter, number> = { all: data?.length ?? 0, live: 0, draft: 0, done: 0 }
    for (const t of data ?? []) c[t.status] += 1
    return c
  }, [data])
  const shown = (data ?? []).filter((t) => filter === 'all' || t.status === filter)

  return (
    <div className="flex flex-col gap-10">
      <div className="flex flex-col gap-3">
        <PageTitle>Tournaments</PageTitle>
        <p className="max-w-xl text-lg text-muted">
          {counts.live > 0 ? `${counts.live} live now. ` : ''}
          {tagline}
        </p>
      </div>
      {admin && <CreateForm />}
      {data && data.length > 0 && <FilterBar value={filter} counts={counts} onChange={setFilter} />}
      <ErrorText error={error} />
      {isPending && (
        <ul className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3" aria-label="Loading tournaments">
          {[0, 1, 2].map((i) => (
            <li key={i}>
              <CardSkeleton />
            </li>
          ))}
        </ul>
      )}
      {data?.length === 0 && (
        <div className="rounded-card border border-dashed border-field-line p-10 text-center">
          <p className="text-lg font-medium">No tournaments yet</p>
          <p className="mt-1 text-muted">{admin ? 'Create the first one above, or load sample tournaments to explore every format.' : 'Check back when the organizer starts one.'}</p>
          {admin && <DemoButton />}
        </div>
      )}
      {data && data.length > 0 && shown.length === 0 && <p className="text-muted">Nothing matches this filter.</p>}
      <ul className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {shown.map((t) => (
          <li key={t.id} className="min-w-0">
            <TournamentCard t={t} />
          </li>
        ))}
      </ul>
    </div>
  )
}
