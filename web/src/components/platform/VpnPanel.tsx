import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Download, ShieldCheck } from 'lucide-react'
import { platform } from '../../api/platform'
import { Button, ErrorText } from '../ui'
import { Panel } from './bits'

const when = (t: number) => new Date(t * 1000).toLocaleDateString([], { dateStyle: 'medium' })

/** How a player gets their own VPN profile for an event whose instances live on private networks. */
export function VpnPanel() {
  const client = useQueryClient()
  const q = useQuery({ queryKey: ['vpn'], queryFn: platform.vpnStatus })
  const download = useMutation({
    mutationFn: platform.vpnProfile,
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
  return (
    <Panel className="flex flex-col gap-4">
      <div className="flex items-start gap-3">
        <ShieldCheck size={20} aria-hidden="true" className="mt-0.5 shrink-0 text-accent" />
        <div className="flex flex-col gap-1">
          <h2 className="text-lg font-semibold tracking-tight">Your private network</h2>
          <p className="text-sm text-muted">
            Challenge instances in this event are not on the internet. Each player gets their own isolated network, and you reach it through a VPN that only you can use. Other players cannot reach your instances, and you cannot reach theirs.
          </p>
        </div>
      </div>
      <ol className="ml-5 list-decimal space-y-1 text-sm">
        <li>Install an OpenVPN client (version 2.5 or newer), for example OpenVPN Connect or the <code className="font-mono">openvpn</code> command.</li>
        <li>Download your profile below and import it. Keep the file private: it is your key.</li>
        <li>Connect, then start an instance. Its private address appears on the challenge.</li>
      </ol>
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
      {v.has_profile && <p className="text-xs text-muted">A new profile replaces the old one: the old file stops working at once.</p>}
      <ErrorText error={download.error} />
    </Panel>
  )
}
