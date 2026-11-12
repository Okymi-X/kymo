import { useState, type FormEvent } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useNavigate } from 'react-router'
import { api } from '../api/client'
import type { Format } from '../api/types'
import { Button, ErrorText, Field, Input, Select } from './ui'
import { formatLabel } from '../lib/labels'

/** Keeps only the setup fields, so a full backup file can be used as a setup file too. */
function toSetup(raw: unknown) {
  const o = (raw ?? {}) as Record<string, unknown>
  const teams = Array.isArray(o.teams) ? (o.teams as Record<string, unknown>[]).map((t) => ({ name: t.name, score: t.score ?? 0 })) : []
  return { name: o.name, format: o.format, third_place: o.third_place, groups: o.groups, advance: o.advance, rounds: o.rounds, teams }
}

export function TournamentCreate() {
  const [name, setName] = useState('')
  const [format, setFormat] = useState<Format>('single_elimination')
  const [thirdPlace, setThirdPlace] = useState(false)
  const [groups, setGroups] = useState(2)
  const [advance, setAdvance] = useState(2)
  const [rounds, setRounds] = useState(0)
  const [bestOf, setBestOf] = useState(1)
  const client = useQueryClient()
  const navigate = useNavigate()
  const create = useMutation({
    mutationFn: () => api.create(name, format, { thirdPlace: format === 'single_elimination' && thirdPlace, groups, advance, rounds: format === 'swiss' ? rounds : 0, bestOf }),
    onSuccess: (t) => {
      void client.invalidateQueries({ queryKey: ['tournaments'] })
      void client.invalidateQueries({ queryKey: ['admin-events'] })
      void client.invalidateQueries({ queryKey: ['events'] })
      navigate(t.event_id !== null ? `/admin/events/${t.event_id}?view=tournament` : `/t/${t.slug}`)
    },
  })
  const [importError, setImportError] = useState<string | null>(null)
  const importSetup = useMutation({
    mutationFn: (setup: unknown) => api.importSetup(setup),
    onSuccess: (t) => {
      void client.invalidateQueries({ queryKey: ['tournaments'] })
      void client.invalidateQueries({ queryKey: ['admin-events'] })
      void client.invalidateQueries({ queryKey: ['events'] })
      navigate(t.event_id !== null ? `/admin/events/${t.event_id}?view=tournament` : `/t/${t.slug}`)
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
      <Field label="Match">
        <Select value={bestOf} onChange={(e) => setBestOf(Number(e.target.value))}>
          <option value={1}>Single game</option>
          <option value={3}>Best of 3</option>
          <option value={5}>Best of 5</option>
          <option value={7}>Best of 7</option>
        </Select>
      </Field>
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

export function DemoButton() {
  const client = useQueryClient()
  const load = useMutation({
    mutationFn: api.loadDemo,
    onSuccess: async () => {
      await client.invalidateQueries({ queryKey: ['tournaments'] })
      await client.invalidateQueries({ queryKey: ['admin-events'] })
      await client.invalidateQueries({ queryKey: ['events'] })
    },
  })
  return (
    <div className="mt-4 flex flex-col items-center gap-2">
      <Button onClick={() => load.mutate()} disabled={load.isPending}>
        {load.isPending ? 'Loading samples' : 'Load sample tournaments'}
      </Button>
      <ErrorText error={load.error} />
      {load.data && (
        <div role="status" className="mt-2 w-full max-w-lg rounded-card border border-line bg-surface p-4 text-left text-sm">
          <p>
            Added {load.data.added} {load.data.added === 1 ? 'event' : 'events'}. Every sample account uses this password, made just now:
          </p>
          <p className="my-2 select-all rounded-control border border-line bg-bg px-3 py-2 font-mono text-base">{load.data.password}</p>
          <p className="text-muted">
            Sign in as <code className="font-mono">demo-admin</code>, <code className="font-mono">demo-referee</code>, <code className="font-mono">demo-monitor</code> or a player such as <code className="font-mono">demo-ada</code>. Loading the samples again makes a new password.
          </p>
        </div>
      )}
    </div>
  )
}

