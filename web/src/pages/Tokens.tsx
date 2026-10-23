import { useState, type FormEvent } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Navigate } from 'react-router'
import { api, useIsAdmin } from '../api/client'
import type { ApiToken } from '../api/types'
import { ConfirmDialog } from '../components/ConfirmDialog'
import { Button, ErrorText, Field, Input } from '../components/ui'

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

export function Tokens() {
  const admin = useIsAdmin()
  const client = useQueryClient()
  const [label, setLabel] = useState('')
  const [secret, setSecret] = useState<string | null>(null)
  const [revoking, setRevoking] = useState<ApiToken | null>(null)
  const tokens = useQuery({ queryKey: ['tokens'], queryFn: api.listTokens, enabled: admin })
  const refresh = () => client.invalidateQueries({ queryKey: ['tokens'] })

  const create = useMutation({
    mutationFn: () => api.createToken(label),
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
        <h1 className="text-4xl font-semibold tracking-tight">API tokens</h1>
        <p className="mt-2 text-muted">Use a token as a Bearer header to manage tournaments from scripts.</p>
      </div>
      <form onSubmit={submit} className="flex flex-wrap items-end gap-3">
        <Field label="Label">
          <Input value={label} onChange={(e) => setLabel(e.target.value)} placeholder="Score importer" className="w-64" maxLength={80} />
        </Field>
        <Button type="submit" variant="primary" disabled={!label.trim() || create.isPending}>
          Create token
        </Button>
      </form>
      <ErrorText error={create.error ?? tokens.error ?? revoke.error} />
      {secret && <NewSecret secret={secret} onDone={() => setSecret(null)} />}
      {tokens.data?.length === 0 && <p className="text-muted">No tokens yet.</p>}
      <ul className="flex flex-col gap-2">
        {tokens.data?.map((t) => (
          <li key={t.id} className="flex items-center justify-between gap-3 rounded-card border border-line bg-surface px-4 py-3">
            <div className="min-w-0">
              <p className="truncate font-medium">{t.label}</p>
              <p className="text-xs text-muted">Created {new Date(t.created_at * 1000).toLocaleDateString()}</p>
            </div>
            <Button variant="danger" onClick={() => setRevoking(t)}>
              Revoke
            </Button>
          </li>
        ))}
      </ul>
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
