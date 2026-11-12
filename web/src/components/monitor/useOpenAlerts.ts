import { useQuery } from '@tanstack/react-query'
import { platform } from '../../api/platform'

/** How many open alerts are about signed in players, for the badges. Asked every 30 seconds, and only by people who may monitor. */
export function useOpenAlerts(enabled: boolean): number {
  const q = useQuery({
    queryKey: ['monitor-alerts-count'],
    queryFn: () => platform.alerts({ status: 'open', who: 'players', per_page: 1 }),
    enabled,
    refetchInterval: 30_000,
    retry: false,
  })
  return q.data?.open_players ?? 0
}
