import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Upload } from 'lucide-react'
import { useRef, useState, type FormEvent } from 'react'
import { Link, useNavigate } from 'react-router'
import { platform } from '../../api/platform'
import { Gate, Loading, Panel, Pill, StatePill, Toggle } from '../../components/platform/bits'
import { Button, ErrorText, Field, Input, PageTitle, Select } from '../../components/ui'
import { usePageTitle } from '../../lib/usePageTitle'

function CreateEvent() {
  const [name, setName] = useState('')
  const [kind, setKind] = useState<'jeopardy' | 'tournament'>('jeopardy')
  const [mode, setMode] = useState<'solo' | 'team'>('team')
  const client = useQueryClient()
  const navigate = useNavigate()
  const create = useMutation({
    mutationFn: () => platform.createEvent({ name: name.trim(), kind, mode }),
    onSuccess: (e) => {
      void client.invalidateQueries({ queryKey: ['admin-events'] })
      navigate(`/admin/events/${e.id}`)
    },
  })
  const submit = (e: FormEvent) => {
    e.preventDefault()
    create.mutate()
  }
  return (
    <form onSubmit={submit} className="flex flex-wrap items-end gap-3 rounded-card border border-line bg-surface p-4">
      <Field label="New event">
        <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="Spring CTF 2026" className="w-64" maxLength={80} />
      </Field>
      <Field label="Type">
        <Select value={kind} onChange={(e) => setKind(e.target.value as 'jeopardy' | 'tournament')}>
          <option value="jeopardy">Jeopardy CTF</option>
          <option value="tournament">Tournament</option>
        </Select>
      </Field>
      <Field label="Players">
        <Select value={mode} onChange={(e) => setMode(e.target.value as 'solo' | 'team')}>
          <option value="team">Teams</option>
          <option value="solo">Solo</option>
        </Select>
      </Field>
      <Button type="submit" variant="primary" disabled={!name.trim() || create.isPending}>
        Create event
      </Button>
      <ErrorText error={create.error} />
    </form>
  )
}

function ImportBundle() {
  const input = useRef<HTMLInputElement>(null)
  const [keepTimes, setKeepTimes] = useState(false)
  const [keepState, setKeepState] = useState(false)
  const client = useQueryClient()
  const navigate = useNavigate()
  const run = useMutation({
    mutationFn: async (file: File) => platform.importEvent(await file.text(), keepTimes, keepState),
    onSuccess: (e) => {
      void client.invalidateQueries({ queryKey: ['admin-events'] })
      navigate(`/admin/events/${e.id}`)
    },
  })
  return (
    <Panel className="flex flex-col gap-3">
      <h2 className="text-sm font-semibold">Import an event bundle</h2>
      <p className="text-sm text-muted">Creates a new event from a file exported with Export on an event page. Imported challenges start as drafts unless you keep their state.</p>
      <div className="flex flex-wrap items-center gap-4">
        <Toggle label="Keep the schedule" checked={keepTimes} onChange={setKeepTimes} />
        <Toggle label="Keep published challenges" checked={keepState} onChange={setKeepState} />
        <input
          ref={input}
          type="file"
          accept="application/json,.json"
          className="sr-only"
          aria-label="Event bundle file"
          onChange={(e) => {
            const file = e.target.files?.[0]
            if (file) run.mutate(file)
            e.target.value = ''
          }}
        />
        <Button onClick={() => input.current?.click()} disabled={run.isPending}>
          <Upload size={15} aria-hidden="true" /> Choose file
        </Button>
      </div>
      <ErrorText error={run.error} />
    </Panel>
  )
}

function Body() {
  const q = useQuery({ queryKey: ['admin-events'], queryFn: platform.adminEvents })
  if (q.isPending) return <Loading />
  return (
    <div className="flex flex-col gap-6">
      <CreateEvent />
      <ul className="grid gap-3 sm:grid-cols-2">
        {(q.data ?? []).map((e) => (
          <li key={e.id}>
            <Link to={`/admin/events/${e.id}`} className="flex flex-col gap-2 rounded-card border border-line bg-surface p-4 transition-shadow hover:shadow-[var(--shadow-lift)]">
              <span className="flex items-start justify-between gap-3">
                <span className="font-medium">{e.name}</span>
                <StatePill state={e.effective_state} />
              </span>
              <span className="flex gap-2">
                <Pill>{e.kind === 'jeopardy' ? 'Jeopardy' : 'Tournament'}</Pill>
                <Pill>{e.mode === 'team' ? 'Teams' : 'Solo'}</Pill>
                <span className="font-mono text-xs text-muted">/{e.slug}</span>
              </span>
            </Link>
          </li>
        ))}
      </ul>
      <ImportBundle />
    </div>
  )
}

export function AdminEvents() {
  usePageTitle('Manage events')
  return (
    <div className="flex flex-col gap-6">
      <PageTitle>Manage events</PageTitle>
      <Gate need="admin">
        <Body />
      </Gate>
    </div>
  )
}
