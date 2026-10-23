import { useMemo, useState, type FormEvent } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useNavigate } from 'react-router'
import { api, useIsAdmin } from '../api/client'
import { useTournaments } from '../api/hooks'
import type { Format, Status } from '../api/types'
import { CardSkeleton, TournamentCard } from '../components/TournamentCard'
import { Button, ErrorText, Field, Input, PageTitle, Select } from '../components/ui'
import { formatLabel } from '../lib/labels'

function CreateForm() {
  const [name, setName] = useState('')
  const [format, setFormat] = useState<Format>('single_elimination')
  const [thirdPlace, setThirdPlace] = useState(false)
  const [groups, setGroups] = useState(2)
  const [advance, setAdvance] = useState(2)
  const client = useQueryClient()
  const navigate = useNavigate()
  const create = useMutation({
    mutationFn: () => api.create(name, format, { thirdPlace: format === 'single_elimination' && thirdPlace, groups, advance }),
    onSuccess: (t) => {
      void client.invalidateQueries({ queryKey: ['tournaments'] })
      navigate(`/t/${t.slug}`)
    },
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
      {format === 'single_elimination' && (
        <label className="flex h-10 items-center gap-2 text-sm">
          <input type="checkbox" checked={thirdPlace} onChange={(e) => setThirdPlace(e.target.checked)} className="size-4 accent-accent" />
          Third place match
        </label>
      )}
      <Button type="submit" variant="primary" disabled={!name.trim() || create.isPending}>
        Create
      </Button>
      <ErrorText error={create.error} />
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

export function Home() {
  const admin = useIsAdmin()
  const { data, isPending, error } = useTournaments()
  const [filter, setFilter] = useState<Filter>('all')

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
          {counts.live > 0 ? `${counts.live} live now. ` : ''}Knockout brackets for CTF events, updated as results come in.
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
          <p className="mt-1 text-muted">{admin ? 'Create the first one above.' : 'Check back when the organizer starts one.'}</p>
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
