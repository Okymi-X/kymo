import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { AlertTriangle, Download, Loader2, Play, RotateCcw, Square } from 'lucide-react'
import { platform } from '../../api/platform'
import type { InstanceView } from '../../api/platformTypes'
import { countdown } from '../../lib/platform'
import { Button, ErrorText } from '../ui'
import { CopyButton, useNow } from './bits'

const failureText: Record<string, string> = {
  start_failed: 'The container could not be started. Try again; if it keeps failing, tell an organizer.',
  never_ready: 'It started but never answered, so it was stopped. Try again.',
  exited: 'Your instance stopped unexpectedly. Start a new one.',
}

/** Shown on a private-network challenge when the player has no profile yet. */
function NeedsProfile({ onDone }: { onDone: () => void }) {
  const download = useMutation({ mutationFn: platform.vpnProfile, onSuccess: onDone })
  return (
    <div role="alert" className="flex flex-col gap-2 rounded-control border border-line bg-bg p-3 text-sm">
      <p className="flex items-start gap-2">
        <AlertTriangle size={16} aria-hidden="true" className="mt-0.5 shrink-0 text-danger" />
        <span>This event uses a private network. You have no VPN profile yet, so you could not reach your instance. Download it, import it into OpenVPN and connect.</span>
      </p>
      <Button className="self-start" onClick={() => download.mutate()} disabled={download.isPending}>
        <Download size={15} aria-hidden="true" /> Download my VPN profile
      </Button>
      <ErrorText error={download.error} />
    </div>
  )
}

interface Props {
  slug: string
  challengeId: number
  /** The challenge accepts starts right now. */
  open: boolean
  /** The event reaches instances through the player's VPN. */
  vpn?: boolean
}

/** Start, restart and stop the player's own container for one challenge, and say honestly what state it is in. */
export function InstancePanel({ slug, challengeId, open, vpn = false }: Props) {
  const client = useQueryClient()
  const key = ['instance', slug, challengeId]
  // Poll quickly while the service is coming up, slowly otherwise.
  const q = useQuery({
    queryKey: key,
    queryFn: () => platform.instance(slug, challengeId),
    refetchInterval: (query) => (query.state.data?.state === 'starting' ? 1500 : 15_000),
  })
  const profile = useQuery({ queryKey: ['vpn'], queryFn: platform.vpnStatus, enabled: vpn })
  const set = (view: InstanceView) => {
    client.setQueryData(key, view)
    void client.invalidateQueries({ queryKey: ['instances', slug] })
  }
  const start = useMutation({ mutationFn: () => platform.startInstance(slug, challengeId), onSuccess: set })
  const restart = useMutation({ mutationFn: () => platform.restartInstance(slug, challengeId), onSuccess: set })
  const stop = useMutation({ mutationFn: () => platform.stopInstance(slug, challengeId), onSuccess: set })
  const busy = start.isPending || restart.isPending || stop.isPending
  const now = useNow()
  const view = q.data
  if (q.isPending) return <p className="text-sm text-muted">Checking your instance...</p>
  if (!view) return <ErrorText error={q.error} />

  const left = view.expires_at ? view.expires_at - Math.floor(now / 1000) : 0
  const starting = start.isPending || restart.isPending || view.state === 'starting'
  const running = !starting && view.state === 'running' && left > 0
  const expired = view.state === 'expired' || (view.state === 'running' && left <= 0)
  const failed = view.state === 'failed'
  const address = running && view.host && view.port ? `${view.host}:${view.port}` : null
  const budgetLeft = view.starts_max - view.starts_used
  const privateNet = vpn || view.network === 'vpn'
  const needsProfile = privateNet && profile.data?.enabled === true && profile.data.has_profile === false

  return (
    <section aria-label="Your instance" className="flex flex-col gap-3 rounded-card border border-line p-4" aria-live="polite">
      <div className="flex items-center justify-between gap-3">
        <h3 className="text-sm font-semibold">Your instance</h3>
        <span className="text-xs text-muted">
          {budgetLeft} of {view.starts_max} starts left
        </span>
      </div>

      {needsProfile && <NeedsProfile onDone={() => void client.invalidateQueries({ queryKey: ['vpn'] })} />}

      {starting ? (
        <p className="flex items-center gap-2 text-sm">
          <Loader2 size={16} aria-hidden="true" className="animate-spin text-accent" />
          Starting your instance. The address appears when it is ready to accept connections.
        </p>
      ) : running && address ? (
        <>
          <div className="flex flex-wrap items-center gap-2">
            <code className="rounded-control border border-line bg-bg px-2.5 py-1.5 font-mono text-sm">{address}</code>
            <CopyButton text={address} />
          </div>
          {(view.services ?? []).length > 0 && (
            <ul className="flex flex-col gap-1.5" aria-label="Other services">
              {view.services?.map((svc) => (
                <li key={svc.name} className="flex flex-wrap items-center gap-2 text-sm">
                  <span className="w-14 text-muted">{svc.name}</span>
                  <code className="rounded-control border border-line bg-bg px-2 py-1 font-mono text-xs">{`${view.host}:${svc.port}`}</code>
                  <CopyButton text={`${view.host}:${svc.port}`} />
                </li>
              ))}
            </ul>
          )}
          <p className="text-sm text-muted">
            Stops in <span className="font-mono tabular-nums text-ink">{countdown(left)}</span>
            {privateNet && '. This address is private and only works while your VPN is connected.'}
          </p>
        </>
      ) : expired ? (
        <p className="text-sm text-muted">Your instance expired and was stopped. Start a new one to continue.</p>
      ) : failed ? (
        <p role="alert" className="text-sm text-danger">
          {failureText[view.failure ?? 'start_failed']}
        </p>
      ) : (
        <p className="text-sm text-muted">Nothing is running. Starting one gives you a private copy of this challenge.</p>
      )}

      <div className="flex flex-wrap gap-2">
        {starting ? (
          <Button variant="danger" onClick={() => stop.mutate()} disabled={busy || view.state !== 'starting'}>
            <Square size={15} aria-hidden="true" /> Cancel
          </Button>
        ) : running ? (
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
            <Play size={15} aria-hidden="true" /> {failed || expired ? 'Start a new instance' : 'Start instance'}
          </Button>
        )}
      </div>
      <ErrorText error={start.error ?? restart.error ?? stop.error} />
    </section>
  )
}
