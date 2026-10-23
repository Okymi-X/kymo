import { useState, type FormEvent } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Link, useNavigate } from 'react-router'
import { api, useIsAdmin } from '../api/client'
import { useTournaments } from '../api/hooks'
import type { Format } from '../api/types'
import { Button, ErrorText, Field, Input, Select, StatusPill } from '../components/ui'
import { formatLabel } from '../lib/labels'

function CreateForm() {
  const [name, setName] = useState('')
  const [format, setFormat] = useState<Format>('single_elimination')
  const [thirdPlace, setThirdPlace] = useState(false)
  const client = useQueryClient()
  const navigate = useNavigate()
  const create = useMutation({
    mutationFn: () => api.create(name, format, format === 'single_elimination' && thirdPlace),
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
        </Select>
      </Field>
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

export function Home() {
  const admin = useIsAdmin()
  const { data, isPending, error } = useTournaments()
  return (
    <div className="flex flex-col gap-8">
      <div>
        <h1 className="text-4xl font-semibold tracking-tight">Tournaments</h1>
        <p className="mt-2 text-muted">Knockout brackets for CTF events.</p>
      </div>
      {admin && <CreateForm />}
      {isPending && <p className="text-muted">Loading</p>}
      <ErrorText error={error} />
      {data?.length === 0 && <p className="text-muted">No tournaments yet.</p>}
      <ul className="grid gap-3 sm:grid-cols-2">
        {data?.map((t) => (
          <li key={t.id}>
            <Link to={`/t/${t.slug}`} className="flex items-center justify-between gap-3 rounded-card border border-line bg-surface p-4 transition-colors hover:bg-line/30">
              <div className="min-w-0">
                <p className="truncate font-medium">{t.name}</p>
                <p className="text-sm text-muted">
                  {formatLabel[t.format]} · {t.teams.length} teams
                </p>
              </div>
              <StatusPill status={t.status} />
            </Link>
          </li>
        ))}
      </ul>
    </div>
  )
}
