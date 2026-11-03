import { useQuery } from '@tanstack/react-query'
import { Loader2 } from 'lucide-react'
import { platform } from '../../api/platform'
import { countdown } from '../../lib/platform'
import { CopyButton, Panel } from './bits'

/** Every instance the player has running in this event, so an address is never separated from its challenge. */
export function MyInstances({ slug }: { slug: string }) {
  const q = useQuery({ queryKey: ['instances', slug], queryFn: () => platform.instances(slug), refetchInterval: 10_000 })
  const list = q.data?.instances ?? []
  if (list.length === 0) return null
  return (
    <Panel className="flex flex-col gap-3">
      <h2 className="text-sm font-semibold">Your instances</h2>
      <ul className="flex flex-col divide-y divide-line">
        {list.map((i) => {
          const address = i.host && (i.machine || i.port) ? (i.machine ? i.host : `${i.host}:${i.port}`) : null
          return (
            <li key={i.id} className="flex flex-wrap items-center gap-x-4 gap-y-2 py-2.5 first:pt-0 last:pb-0">
              <span className="min-w-0 flex-1 truncate font-medium">{i.title}</span>
              {address ? (
                <>
                  <code className="rounded-control border border-line bg-bg px-2 py-1 font-mono text-xs">{address}</code>
                  <CopyButton text={address} />
                  <span className="w-20 text-right font-mono text-xs tabular-nums text-muted">{countdown(i.remaining_secs)}</span>
                  {(i.services ?? []).map((svc) => (
                    <span key={svc.name} className="flex basis-full items-center gap-2 pl-0 text-xs text-muted">
                      <span className="w-14">{svc.name}</span>
                      <code className="rounded-control border border-line bg-bg px-2 py-1 font-mono">{`${i.host}:${svc.port}`}</code>
                      <CopyButton text={`${i.host}:${svc.port}`} />
                    </span>
                  ))}
                </>
              ) : (
                <span className="flex items-center gap-2 text-sm text-muted">
                  <Loader2 size={14} aria-hidden="true" className="animate-spin" /> Starting
                </span>
              )}
            </li>
          )
        })}
      </ul>
    </Panel>
  )
}
