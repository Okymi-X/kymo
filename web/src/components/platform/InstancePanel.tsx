import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Play, RotateCcw, Square } from 'lucide-react'
import { platform } from '../../api/platform'
import type { InstanceView } from '../../api/platformTypes'
import { countdown } from '../../lib/platform'
import { Button, ErrorText } from '../ui'
import { CopyButton, useNow } from './bits'

/** Start, restart and stop the player's own container for one challenge. */
export function InstancePanel({ slug, challengeId, open }: { slug: string; challengeId: number; open: boolean }) {
  const client = useQueryClient()
  const key = ['instance', slug, challengeId]
  const q = useQuery({ queryKey: key, queryFn: () => platform.instance(slug, challengeId), refetchInterval: 15_000 })
  const set = (view: InstanceView) => client.setQueryData(key, view)
  const start = useMutation({ mutationFn: () => platform.startInstance(slug, challengeId), onSuccess: set })
  const restart = useMutation({ mutationFn: () => platform.restartInstance(slug, challengeId), onSuccess: set })
  const stop = useMutation({ mutationFn: () => platform.stopInstance(slug, challengeId), onSuccess: set })
  const busy = start.isPending || restart.isPending || stop.isPending
  const now = useNow()
  const view = q.data
  if (q.isPending) return <p className="text-sm text-muted">Checking your instance...</p>
  if (!view) return <ErrorText error={q.error} />

  const live = view.state === 'running' || view.state === 'starting'
  const left = live && view.expires_at ? view.expires_at - Math.floor(now / 1000) : 0
  const expired = view.state === 'expired' || (live && left <= 0)
  const address = view.host && view.port ? `${view.host}:${view.port}` : null
  const budgetLeft = view.starts_max - view.starts_used

  return (
    <div className="flex flex-col gap-3 rounded-card border border-line p-4" aria-live="polite">
      <div className="flex items-center justify-between gap-3">
        <h3 className="text-sm font-semibold">Your instance</h3>
        <span className="text-xs text-muted">
          {budgetLeft} of {view.starts_max} starts left
        </span>
      </div>
      {live && !expired && address ? (
        <>
          <div className="flex flex-wrap items-center gap-2">
            <code className="rounded-control border border-line bg-bg px-2.5 py-1.5 font-mono text-sm">{address}</code>
            <CopyButton text={address} />
          </div>
          <p className="text-sm text-muted">
            Stops in <span className="font-mono tabular-nums text-ink">{countdown(left)}</span>
          </p>
          {view.network === 'vpn' && (
            <p className="text-sm text-muted">This address is private. It only works while your VPN is connected; download your profile from the event page if you have not yet.</p>
          )}
        </>
      ) : expired ? (
        <p className="text-sm text-muted">Your instance expired and was stopped. Start a new one to continue.</p>
      ) : view.state === 'failed' ? (
        <p className="text-sm text-danger">The last start failed. Try again.</p>
      ) : (
        <p className="text-sm text-muted">Nothing is running. Starting one gives you a private copy of this challenge.</p>
      )}
      <div className="flex flex-wrap gap-2">
        {live && !expired ? (
          <>
            <Button onClick={() => restart.mutate()} disabled={!open || busy}>
              <RotateCcw size={15} aria-hidden="true" /> Restart
            </Button>
            <Button variant="danger" onClick={() => stop.mutate()} disabled={busy}>
              <Square size={15} aria-hidden="true" /> Stop
            </Button>
          </>
        ) : (
          <Button variant="primary" onClick={() => start.mutate()} disabled={!open || busy || budgetLeft <= 0}>
            <Play size={15} aria-hidden="true" /> Start instance
          </Button>
        )}
      </div>
      <ErrorText error={start.error ?? restart.error ?? stop.error} />
    </div>
  )
}
