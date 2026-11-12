import { useQuery } from '@tanstack/react-query'
import { Server, ShieldCheck, ShieldOff } from 'lucide-react'
import { Link } from 'react-router'
import { platform } from '../../api/platform'

/** How often the header asks again, so a tunnel that comes up shows without a refresh. */
const VPN_POLL_MS = 5_000
const INSTANCE_POLL_MS = 10_000

const minutes = (secs: number) => (secs >= 3600 ? `${Math.floor(secs / 3600)}h ${Math.round((secs % 3600) / 60)}m` : `${Math.max(1, Math.round(secs / 60))} min`)

/** Whether the player's VPN is connected right now. Hidden until they have a profile. */
function VpnPill() {
  const q = useQuery({
    queryKey: ['vpn-connection'],
    queryFn: platform.vpnConnection,
    refetchInterval: VPN_POLL_MS,
    refetchOnWindowFocus: true,
    retry: false,
  })
  if (!q.data?.has_profile) return null
  const on = q.data.connected
  const Icon = on ? ShieldCheck : ShieldOff
  return (
    <span
      role="status"
      aria-label={on ? 'VPN connected' : 'VPN not connected'}
      title={on ? 'VPN connected' : 'VPN not connected: open your VPN client and connect'}
      className={`flex h-9 items-center gap-2 rounded-full border px-2.5 text-sm font-medium transition-colors sm:px-3 ${on ? 'border-win/50 bg-win/10 text-win' : 'border-line text-muted'}`}
    >
      <Icon size={15} aria-hidden="true" />
      <span className="hidden sm:inline">{on ? 'VPN connected' : 'Not connected'}</span>
    </span>
  )
}

/** The player's running instance, pulsing while it is up. Opens its challenge. */
function InstanceBeacon() {
  const q = useQuery({
    queryKey: ['my-instance'],
    queryFn: platform.myInstance,
    refetchInterval: INSTANCE_POLL_MS,
    refetchOnWindowFocus: true,
    retry: false,
  })
  const run = q.data?.instance
  if (!run) return null
  const label = `Active instance: ${run.title}${run.state === 'starting' ? ' (starting)' : ''}, ${minutes(run.remaining_secs)} left. Open the challenge`
  return (
    <Link
      to={`/e/${run.event_slug}?challenge=${run.challenge_id}`}
      aria-label={label}
      title={label}
      className="relative flex size-9 items-center justify-center rounded-full border border-accent/50 bg-accent/10 text-accent transition-colors hover:bg-accent/20"
    >
      <Server size={16} aria-hidden="true" />
      <span className="absolute -right-0.5 -top-0.5 flex size-2.5" aria-hidden="true">
        <span className="live-dot absolute inline-flex size-full rounded-full bg-win" />
        <span className="relative inline-flex size-2.5 rounded-full bg-win" />
      </span>
    </Link>
  )
}

/** What a player wants to see at a glance: is the VPN up, and is an instance running. */
export function PlayerStatus() {
  return (
    <>
      <VpnPill />
      <InstanceBeacon />
    </>
  )
}
