import { useQuery } from '@tanstack/react-query'
import { Link, Navigate, useParams } from 'react-router'
import { useSignedIn } from '../api/client'
import { platform } from '../api/platform'
import type { EventState } from '../api/platformTypes'
import { BackLink } from '../components/BackLink'
import { Loading, Panel, Pill, StatePill } from '../components/platform/bits'
import { Scoreboard } from '../components/platform/Scoreboard'
import { buttonClass, PageTitle } from '../components/ui'
import { usePageTitle } from '../lib/usePageTitle'

/** The read only scoreboard of an event the organizer made public. Anyone may open it; it shows nothing else about the
 *  event. A tournament goes to its bracket page, which is the same read only view. */
export function PublicScoreboard() {
  const { slug = '' } = useParams()
  const signedIn = useSignedIn()
  const q = useQuery({ queryKey: ['public-event', slug], queryFn: () => platform.publicEvent(slug), retry: false })
  usePageTitle(q.data?.name)
  if (q.isPending) return <Loading />
  if (q.error || !q.data) {
    return (
      <div className="flex flex-col gap-4">
        <BackLink to="/">Home</BackLink>
        <Panel className="flex flex-col items-start gap-3">
          <h1 className="text-lg font-semibold tracking-tight">This scoreboard is not public</h1>
          <p className="text-muted">The organizer has not made it public, or it does not exist. If you are playing, sign in to see it.</p>
          <Link to={`/signin?next=${encodeURIComponent(`/e/${slug}?view=scoreboard`)}`} className={buttonClass('primary')}>
            Sign in
          </Link>
        </Panel>
      </div>
    )
  }
  const event = q.data
  if (event.kind === 'tournament') return <Navigate to={`/embed/${encodeURIComponent(slug)}`} replace />
  return (
    <div className="flex flex-col gap-6">
      <BackLink to="/">Home</BackLink>
      <div className="flex flex-wrap items-center gap-3">
        <PageTitle>{event.name}</PageTitle>
        <StatePill state={event.effective_state as EventState} />
        <Pill>{event.mode === 'team' ? 'Teams' : 'Solo'}</Pill>
        {!signedIn && (
          <Link to="/signin" className={`${buttonClass('secondary')} ml-auto`}>
            Sign in to play
          </Link>
        )}
      </div>
      <Scoreboard slug={event.slug} />
    </div>
  )
}
