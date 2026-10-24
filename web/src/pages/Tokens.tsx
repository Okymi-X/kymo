import { usePageTitle } from '../lib/usePageTitle'
import { useState, type FormEvent } from 'react'
import { useTournaments } from '../api/hooks'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Navigate } from 'react-router'
import { api, useIsAdmin } from '../api/client'
import type { ApiToken } from '../api/types'
import { ConfirmDialog } from '../components/ConfirmDialog'
import { WebhooksSection } from '../components/WebhooksSection'
import { BackLink } from '../components/BackLink'
import { Button, ErrorText, Field, Input, PageTitle, Select } from '../components/ui'

function NewSecret({ secret, onDone }: { secret: string; onDone: () => void }) {
  const [copied, setCopied] = useState(false)
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(secret)
      setCopied(true)
    } catch {
      setCopied(false)
    }
  }
  return (
    <div role="status" className="flex flex-col gap-3 rounded-card border border-accent bg-surface p-4">
      <p className="font-medium">Copy this token now. It is not shown again.</p>
      <code className="break-all rounded-control border border-line bg-bg px-3 py-2 font-mono text-xs">{secret}</code>
      <div className="flex gap-2">
        <Button onClick={copy}>{copied ? 'Copied' : 'Copy'}</Button>
        <Button variant="ghost" onClick={onDone}>
          Done
        </Button>
      </div>
    </div>
  )
}

const EXPIRY: [string, string][] = [
  ['7', '7 days'],
  ['30', '30 days'],
  ['90', '90 days'],
  ['365', '1 year'],
  ['never', 'Never'],
]

export function Tokens() {
  usePageTitle('API tokens')
  const admin = useIsAdmin()
  const client = useQueryClient()
  const [label, setLabel] = useState('')
  const [days, setDays] = useState('30')
  const [referee, setReferee] = useState(false)
  const [picked, setPicked] = useState<number[]>([])
  const tournaments = useTournaments()
  const [secret, setSecret] = useState<string | null>(null)
  const [revoking, setRevoking] = useState<ApiToken | null>(null)
  const tokens = useQuery({ queryKey: ['tokens'], queryFn: api.listTokens, enabled: admin })
  const refresh = () => client.invalidateQueries({ queryKey: ['tokens'] })

  const create = useMutation({
    mutationFn: () => api.createToken(label, days === 'never' ? null : Number(days), referee ? picked : null),
    onSuccess: (made) => {
      setSecret(made.token)
      setLabel('')
      void refresh()
    },
  })
  const revoke = useMutation({
    mutationFn: (id: number) => api.revokeToken(id),
    onSuccess: () => {
      setRevoking(null)
      void refresh()
    },
  })

  if (!admin) return <Navigate to="/login" replace />

  const submit = (e: FormEvent) => {
    e.preventDefault()
    create.mutate()
  }

  return (
    <div className="flex max-w-2xl flex-col gap-8">
      <div>
        <BackLink>Tournaments</BackLink>
        <PageTitle>API tokens</PageTitle>
        <p className="mt-2 text-muted">Use a token as a Bearer header to manage tournaments from scripts.</p>
      </div>
      <form onSubmit={submit} className="flex flex-wrap items-end gap-3">
        <Field label="Label">
          <Input value={label} onChange={(e) => setLabel(e.target.value)} placeholder="Score importer" className="w-64" maxLength={80} />
        </Field>
        <Field label="Expires">
          <Select value={days} onChange={(e) => setDays(e.target.value)} className="w-40">
            {EXPIRY.map(([value, text]) => (
              <option key={value} value={value}>
                {text}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Access">
          <Select value={referee ? 'referee' : 'full'} onChange={(e) => setReferee(e.target.value === 'referee')} className="w-44">
            <option value="full">Full access</option>
            <option value="referee">Referee</option>
          </Select>
        </Field>
        <Button type="submit" variant="primary" disabled={!label.trim() || create.isPending || (referee && picked.length === 0)}>
          Create token
        </Button>
        {referee && (
          <fieldset className="flex w-full flex-col gap-2 rounded-card border border-line p-3">
            <legend className="px-1 text-xs font-medium text-ink/70">A referee records results and schedules only, for these tournaments</legend>
            {(tournaments.data ?? []).map((t) => (
              <label key={t.id} className="flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  className="size-4 accent-accent"
                  checked={picked.includes(t.id)}
                  onChange={(e) => setPicked((p) => (e.target.checked ? [...p, t.id] : p.filter((id) => id !== t.id)))}
                />
                {t.name}
              </label>
            ))}
          </fieldset>
        )}
      </form>
      <ErrorText error={create.error ?? tokens.error ?? revoke.error} />
      {secret && <NewSecret secret={secret} onDone={() => setSecret(null)} />}
      {tokens.data?.length === 0 && <p className="text-muted">No tokens yet.</p>}
      <ul className="flex flex-col gap-2">
        {tokens.data?.map((t) => (
          <li key={t.id} className="flex items-center justify-between gap-3 rounded-card border border-line bg-surface px-4 py-3">
            <div className="min-w-0">
              <p className="truncate font-medium">
                {t.label}
                {t.referee && <span className="ml-2 rounded-full border border-line px-2 py-0.5 text-xs font-normal text-muted">Referee, {t.scope?.length ?? 0} {t.scope?.length === 1 ? 'tournament' : 'tournaments'}</span>}
              </p>
              <p className="text-xs text-muted">Created {new Date(t.created_at * 1000).toLocaleDateString()}. {t.expires_at === null ? 'Never expires' : `Expires ${new Date(t.expires_at * 1000).toLocaleDateString()}`}</p>
            </div>
            <Button variant="danger" onClick={() => setRevoking(t)}>
              Revoke
            </Button>
          </li>
        ))}
      </ul>
      <WebhooksSection />
      {revoking && (
        <ConfirmDialog
          title={`Revoke ${revoking.label}?`}
          message="Scripts using this token stop working immediately."
          confirmLabel="Revoke"
          danger
          pending={revoke.isPending}
          onConfirm={() => revoke.mutate(revoking.id)}
          onClose={() => setRevoking(null)}
        />
      )}
    </div>
  )
}
