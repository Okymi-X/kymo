import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Download, ShieldCheck } from 'lucide-react'
import { platform } from '../../api/platform'
import { Button, ErrorText } from '../ui'
import { Panel } from './bits'

const when = (t: number) => new Date(t * 1000).toLocaleDateString([], { dateStyle: 'medium' })

/** How a player gets their own VPN profile for an event whose instances live on private networks. */
export function VpnPanel({ labHost = null }: { labHost?: number | null }) {
  const client = useQueryClient()
  const q = useQuery({ queryKey: ['vpn', labHost], queryFn: () => platform.vpnStatus(labHost) })
  const download = useMutation({
    mutationFn: () => platform.vpnProfile(labHost),
    onSuccess: () => void client.invalidateQueries({ queryKey: ['vpn'] }),
  })
  const v = q.data
  if (!v) return null
  if (!v.enabled) {
    return (
      <Panel>
        <p className="text-sm text-muted">This event uses a private network, but the VPN is not enabled on this server. Ask the organizer.</p>
      </Panel>
    )
  }
  if (v.available === false) {
    return (
      <Panel>
        <p role="alert" className="text-sm text-danger">
          The VPN for this event is not available right now{v.reason ? `: ${v.reason}` : ''}. Try again shortly or ask the organizer.
        </p>
      </Panel>
    )
  }
  return (
    <Panel className="flex flex-col gap-4">
      <div className="flex items-start gap-3">
        <ShieldCheck size={20} aria-hidden="true" className="mt-0.5 shrink-0 text-accent" />
        <div className="flex flex-col gap-1">
          <h2 className="text-lg font-semibold tracking-tight">Your private network</h2>
          <p className="text-sm text-muted">
            Challenge instances in this event are not on the internet{v.host ? ` (the VPN server is ${v.host}${v.port ? `:${v.port}` : ''}, UDP)` : ''}. Each player gets their own isolated network, and you reach it through a VPN that only you can use. Other players cannot reach your instances, and you cannot reach theirs.
          </p>
        </div>
      </div>
      <ol className="ml-5 list-decimal space-y-1 text-sm">
        <li>Install an OpenVPN client (version 2.5 or newer), for example OpenVPN Connect or the <code className="font-mono">openvpn</code> command.</li>
        <li>Download your profile below and import it. Keep the file private: it is your key.</li>
        <li>Connect, then start an instance. Its private address appears on the challenge.</li>
      </ol>
      {v.has_profile && (
        <p className="rounded-control border border-line bg-bg p-3 text-sm">
          <strong>Heads up:</strong> downloading a new profile replaces your current one. The old file stops working at once, so only do it if you lost it or it stopped working.
        </p>
      )}
      <div className="flex flex-wrap items-center gap-3">
        <Button variant="primary" onClick={() => download.mutate()} disabled={download.isPending}>
          <Download size={15} aria-hidden="true" /> {v.has_profile ? 'Download a new profile' : 'Download my VPN profile'}
        </Button>
        {v.has_profile && (
          <span className="text-sm text-muted">
            Your tunnel address <code className="font-mono text-ink">{v.tunnel_ip}</code>
            {v.expires_at ? `, valid until ${when(v.expires_at)}` : ''}
          </span>
        )}
      </div>
      {download.isSuccess && (
        <p role="status" className="text-sm font-medium text-win">
          Profile generated and saved as kymo.ovpn. Look in your browser's downloads, then import it into your OpenVPN client.
        </p>
      )}
      <ErrorText error={download.error} />
    </Panel>
  )
}
