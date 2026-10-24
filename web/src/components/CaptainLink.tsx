import { useState } from 'react'
import { useMutation } from '@tanstack/react-query'
import { Link2 } from 'lucide-react'
import { api } from '../api/client'
import { Button, ErrorText } from './ui'

/** Gives the organizer a private link a team captain can use to report their own scores. */
export function CaptainLink({ tournamentId, teamId }: { tournamentId: number; teamId: number }) {
  const [url, setUrl] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)
  const make = useMutation({
    mutationFn: () => api.teamCode(tournamentId, teamId),
    onSuccess: ({ code }) => {
      setUrl(`${window.location.origin}/report/${code}`)
      setCopied(false)
    },
  })
  const copy = async () => {
    if (!url) return
    try {
      await navigator.clipboard.writeText(url)
      setCopied(true)
    } catch {
      setCopied(false)
    }
  }
  return (
    <div className="flex flex-col gap-2 rounded-card border border-line p-3">
      <p className="text-xs font-medium text-muted">Captain link</p>
      <p className="text-sm text-muted">The captain can submit scores for this team. You approve each one before it counts.</p>
      {url ? (
        <div className="flex flex-col gap-2">
          <code className="break-all rounded-control border border-line bg-bg px-3 py-2 font-mono text-xs">{url}</code>
          <div className="flex gap-2">
            <Button onClick={copy}>{copied ? 'Copied' : 'Copy link'}</Button>
            <Button variant="ghost" onClick={() => make.mutate()} disabled={make.isPending}>
              New link
            </Button>
          </div>
        </div>
      ) : (
        <Button onClick={() => make.mutate()} disabled={make.isPending} className="self-start">
          <Link2 size={16} aria-hidden="true" />
          Create captain link
        </Button>
      )}
      <ErrorText error={make.error} />
    </div>
  )
}
