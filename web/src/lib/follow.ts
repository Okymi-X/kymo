import { useCallback, useEffect, useState } from 'react'

const key = (slug: string) => `kymo.follow.${slug}`

function read(slug: string): number | null {
  try {
    const raw = localStorage.getItem(key(slug))
    const id = raw === null ? NaN : Number(raw)
    return Number.isInteger(id) ? id : null
  } catch {
    return null
  }
}

/** The team a visitor follows in one tournament, remembered in this browser only. */
export function useFollowedTeam(slug: string): [number | null, (teamId: number | null) => void] {
  const [followed, setFollowed] = useState<number | null>(() => read(slug))

  useEffect(() => {
    setFollowed(read(slug))
    const onStorage = (e: StorageEvent) => {
      if (e.key === key(slug)) setFollowed(read(slug))
    }
    window.addEventListener('storage', onStorage)
    return () => window.removeEventListener('storage', onStorage)
  }, [slug])

  const update = useCallback(
    (teamId: number | null) => {
      setFollowed(teamId)
      try {
        if (teamId === null) localStorage.removeItem(key(slug))
        else localStorage.setItem(key(slug), String(teamId))
      } catch {
        /* storage unavailable: the choice lasts until the page closes */
      }
    },
    [slug],
  )
  return [followed, update]
}
