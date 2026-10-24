import { useState, type FormEvent } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '../api/client'
import type { Webhook } from '../api/types'
import { ConfirmDialog } from './ConfirmDialog'
import { Button, ErrorText, Field, Input } from './ui'

/** Where results and schedules are posted as they happen, for example a Discord or Slack channel. */
export function WebhooksSection() {
  const client = useQueryClient()
  const [label, setLabel] = useState('')
  const [url, setUrl] = useState('')
  const [removing, setRemoving] = useState<Webhook | null>(null)
  const [tested, setTested] = useState<{ id: number; ok: boolean; text: string } | null>(null)
  const hooks = useQuery({ queryKey: ['webhooks'], queryFn: api.listWebhooks })
  const refresh = () => client.invalidateQueries({ queryKey: ['webhooks'] })

  const create = useMutation({
    mutationFn: () => api.createWebhook(label, url),
    onSuccess: () => {
      setLabel('')
      setUrl('')
      void refresh()
    },
  })
  const remove = useMutation({
    mutationFn: (id: number) => api.deleteWebhook(id),
    onSuccess: () => {
      setRemoving(null)
      void refresh()
    },
  })
  const test = useMutation({
    mutationFn: (id: number) => api.testWebhook(id),
    onSuccess: ({ status }, id) => setTested({ id, ok: status >= 200 && status < 300, text: status >= 200 && status < 300 ? `Delivered (${status})` : `The receiver answered ${status}` }),
    onError: (e, id) => setTested({ id, ok: false, text: e instanceof Error ? e.message : 'Could not be reached' }),
  })

  const submit = (e: FormEvent) => {
    e.preventDefault()
    create.mutate()
  }

  return (
    <section aria-label="Webhooks" className="flex flex-col gap-4 border-t border-line pt-8">
      <div>
        <h2 className="text-xl font-semibold tracking-tight">Webhooks</h2>
        <p className="mt-1 text-muted">Post results, corrections, forfeits and schedules to a chat channel as they happen. Works with Discord and Slack incoming webhooks and any URL that accepts JSON.</p>
      </div>
      <form onSubmit={submit} className="flex flex-wrap items-end gap-3">
        <Field label="Label">
          <Input value={label} onChange={(e) => setLabel(e.target.value)} placeholder="Event chat" className="w-48" maxLength={80} />
        </Field>
        <Field label="Webhook URL">
          <Input value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://discord.com/api/webhooks/..." className="w-96 max-w-full" maxLength={500} />
        </Field>
        <Button type="submit" variant="primary" disabled={!label.trim() || !url.trim() || create.isPending}>
          Add webhook
        </Button>
      </form>
      <ErrorText error={create.error ?? hooks.error ?? remove.error} />
      {hooks.data?.length === 0 && <p className="text-muted">No webhooks yet.</p>}
      <ul className="flex flex-col gap-2">
        {hooks.data?.map((h) => (
          <li key={h.id} className="flex flex-wrap items-center justify-between gap-3 rounded-card border border-line bg-surface px-4 py-3">
            <div className="min-w-0">
              <p className="truncate font-medium">{h.label}</p>
              <p className="truncate text-xs text-muted">{h.host}</p>
              {tested?.id === h.id && (
                <p role="status" className={`text-xs ${tested.ok ? 'text-win' : 'text-danger'}`}>
                  {tested.text}
                </p>
              )}
            </div>
            <div className="flex gap-2">
              <Button onClick={() => test.mutate(h.id)} disabled={test.isPending}>
                Send test
              </Button>
              <Button variant="danger" onClick={() => setRemoving(h)}>
                Remove
              </Button>
            </div>
          </li>
        ))}
      </ul>
      {removing && (
        <ConfirmDialog
          title={`Remove ${removing.label}?`}
          message="Nothing more is posted to it. The channel itself is untouched."
          confirmLabel="Remove"
          danger
          pending={remove.isPending}
          onConfirm={() => remove.mutate(removing.id)}
          onClose={() => setRemoving(null)}
        />
      )}
    </section>
  )
}
