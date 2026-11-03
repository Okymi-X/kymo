import { useEffect } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { useSignedIn, useWho } from './client'
import { platform } from './platform'

export const useEvents = () => useQuery({ queryKey: ['events'], queryFn: platform.events, refetchInterval: 30_000 })

export const useEvent = (slug: string) => {
  const signedIn = useSignedIn()
  return useQuery({ queryKey: ['event', slug, signedIn], queryFn: () => platform.event(slug), refetchInterval: 15_000 })
}

export const useMe = () => {
  const who = useWho()
  return useQuery({ queryKey: ['me', who?.id], queryFn: platform.me, enabled: who !== null })
}

export const useMyTeam = () => {
  const who = useWho()
  return useQuery({
    queryKey: ['team', who?.id],
    queryFn: async () => {
      try {
        return await platform.myTeam()
      } catch (e) {
        // Having no team is a normal state, not an error.
        if (e instanceof Error && 'status' in e && (e as { status: number }).status === 404) return null
        throw e
      }
    },
    enabled: who !== null,
  })
}

/** Refetches the board and scoreboard whenever the server reports a solve in this event. */
export function useEventStream(slug: string, enabled: boolean) {
  const client = useQueryClient()
  useEffect(() => {
    if (!enabled) return
    const source = new EventSource(`/api/v1/events/${encodeURIComponent(slug)}/stream`)
    source.addEventListener('update', () => {
      void client.invalidateQueries({ queryKey: ['challenges', slug] })
      void client.invalidateQueries({ queryKey: ['scoreboard', slug] })
      void client.invalidateQueries({ queryKey: ['progress', slug] })
    })
    return () => source.close()
  }, [slug, enabled, client])
}
