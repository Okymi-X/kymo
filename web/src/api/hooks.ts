import { useEffect } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from './client'

export const useTournaments = () => useQuery({ queryKey: ['tournaments'], queryFn: api.list })

export const useTournament = (slug: string) =>
  useQuery({ queryKey: ['tournament', slug], queryFn: () => api.get(slug) })

/** Refetches the tournament whenever the server reports a change. */
export function useLiveUpdates(slug: string) {
  const client = useQueryClient()
  useEffect(() => {
    const source = new EventSource(`/api/v1/tournaments/${encodeURIComponent(slug)}/events`)
    source.addEventListener('update', () => {
      void client.invalidateQueries({ queryKey: ['tournament', slug] })
    })
    return () => source.close()
  }, [slug, client])
}
