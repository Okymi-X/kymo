import { useMemo } from 'react'
import { blobatarUri } from 'blobatar/uri'

/** Deterministic avatar: the same team name always renders the same blob. */
export function Avatar({ name, size = 20 }: { name: string; size?: number }) {
  const src = useMemo(() => blobatarUri(name), [name])
  return <img src={src} alt="" data-avatar width={size} height={size} className="shrink-0 rounded-full" />
}
