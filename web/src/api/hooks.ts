import { useEffect } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from './client'

export const useTournaments = () => useQuery({ queryKey: ['tournaments'], queryFn: api.list, refetchInterval: 30_000 })

export const useTournament = (slug: string) =>
  useQuery({ queryKey: ['tournament', slug], queryFn: () => api.get(slug) })

export const useActivity = (slug: string) => useQuery({ queryKey: ['activity', slug], queryFn: () => api.activity(slug) })

/** Refetches the tournament whenever the server reports a change. */
export function useLiveUpdates(slug: string) {
  const client = useQueryClient()
  useEffect(() => {
    const source = new EventSource(`/api/v1/tournaments/${encodeURIComponent(slug)}/events`)
    source.addEventListener('update', () => {
      void client.invalidateQueries({ queryKey: ['tournament', slug] })
      void client.invalidateQueries({ queryKey: ['activity', slug] })
    })
    return () => source.close()
  }, [slug, client])
}
