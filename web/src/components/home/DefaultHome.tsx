import { useQuery } from '@tanstack/react-query'
import { ArrowRight, Megaphone, Server } from 'lucide-react'
import { Link } from 'react-router'
import { useSignedIn, useWho } from '../../api/client'
import { platform } from '../../api/platform'
import { useEvents } from '../../api/platformHooks'
import type { HomeContent } from '../../api/platformTypes'
import { scoreboardPath } from '../../lib/home'
import { stateLabel } from '../../lib/platform'
import { EventCard } from '../platform/EventCard'
import { Panel, Pill } from '../platform/bits'
import { buttonClass } from '../ui'

function Banner({ text }: { text: string }) {
  return (
    <div role="note" className="flex items-start gap-3 rounded-card border border-accent/40 bg-accent/10 px-4 py-3 text-sm">
      <Megaphone size={16} aria-hidden="true" className="mt-0.5 shrink-0 text-accent" />
      <p className="whitespace-pre-line">{text}</p>
    </div>
  )
}

/** The player's running instance, if they have one: the quickest way back to it. */
function YourInstance() {
  const q = useQuery({ queryKey: ['my-instance'], queryFn: platform.myInstance, refetchInterval: 10_000, retry: false })
  const run = q.data?.instance
  if (!run) return null
  return (
    <Link
      to={`/e/${run.event_slug}?challenge=${run.challenge_id}`}
      className="flex items-center gap-3 rounded-card border border-accent/50 bg-accent/10 px-4 py-3 transition-colors hover:bg-accent/20"
    >
      <span className="relative flex size-9 shrink-0 items-center justify-center rounded-full border border-accent/50 text-accent">
        <Server size={16} aria-hidden="true" />
        <span className="absolute -right-0.5 -top-0.5 flex size-2.5" aria-hidden="true">
          <span className="live-dot absolute inline-flex size-full rounded-full bg-win" />
          <span className="relative inline-flex size-2.5 rounded-full bg-win" />
        </span>
      </span>
      <span className="flex min-w-0 flex-col">
        <span className="text-sm text-muted">Your instance is {run.state === 'starting' ? 'starting' : 'running'}</span>
        <span className="truncate font-medium">{run.title}</span>
      </span>
      <ArrowRight size={16} aria-hidden="true" className="ml-auto shrink-0 text-muted" />
    </Link>
  )
}

/** Events a signed in player can open now: running or about to start. */
function YourEvents() {
  const events = (useEvents().data ?? []).filter((e) => e.effective_state !== 'ended' && e.effective_state !== 'archived')
  if (events.length === 0) return null
  return (
    <section aria-label="Events" className="flex flex-col gap-3">
      <div className="flex items-baseline justify-between gap-3">
        <h2 className="text-lg font-semibold tracking-tight">Live and upcoming events</h2>
        <Link to="/events" className="text-sm font-medium text-accent hover:underline">
          All events
        </Link>
      </div>
      <ul className="grid gap-4 sm:grid-cols-2">
        {events.slice(0, 4).map((e) => (
          <li key={e.id} className="min-w-0">
            <EventCard event={e} />
          </li>
        ))}
      </ul>
    </section>
  )
}

/** Events whose scoreboard the organizer made public. Anyone may open these. */
function PublicScoreboards({ signedIn }: { signedIn: boolean }) {
  const q = useQuery({ queryKey: ['public-scoreboards'], queryFn: platform.publicScoreboards, retry: false, staleTime: 30_000 })
  const list = q.data ?? []
  if (list.length === 0) return null
  return (
    <section aria-label="Public scoreboards" className="flex flex-col gap-3">
      <h2 className="text-lg font-semibold tracking-tight">Scoreboards</h2>
      <ul className="grid gap-3 sm:grid-cols-2">
        {list.map((e) => (
          <li key={e.slug} className="min-w-0">
            <Link
              to={scoreboardPath(e.kind, e.slug, signedIn)}
              className="flex items-center justify-between gap-3 rounded-card border border-line bg-surface px-4 py-3 transition-shadow hover:shadow-[var(--shadow-lift)]"
            >
              <span className="min-w-0">
                <span className="block truncate font-medium">{e.name}</span>
                <span className="text-sm text-muted">{e.kind === 'tournament' ? 'Tournament' : 'Jeopardy CTF'}</span>
              </span>
              <Pill>{stateLabel[e.effective_state as keyof typeof stateLabel] ?? e.effective_state}</Pill>
            </Link>
          </li>
        ))}
      </ul>
    </section>
  )
}

/** The home page built from the organizer's texts. A visitor sees the sign in and sign up buttons; a player who is
 *  signed in sees a greeting, their running instance and the events they can open instead. */
export function DefaultHome({ home }: { home: HomeContent }) {
  const signedIn = useSignedIn()
  const who = useWho()
  const open = useQuery({ queryKey: ['sign-ups-open'], queryFn: platform.signUpsOpen, retry: false })
  const signUpsClosed = open.data?.open === false
  return (
    <div className="flex flex-col gap-10">
      {home.banner && <Banner text={home.banner} />}
      <section className="flex flex-col items-start gap-5 py-4 sm:py-8">
        {signedIn ? (
          <>
            <h1 className="text-3xl font-semibold leading-tight tracking-[-0.03em] sm:text-4xl">
              {home.welcome}
              {who?.username ? `, ${who.username}` : ''}
            </h1>
            <Link to="/events" className={buttonClass('primary')}>
              {home.continue_label} <ArrowRight size={16} aria-hidden="true" />
            </Link>
          </>
        ) : (
          <>
            <h1 className="max-w-3xl text-3xl font-semibold leading-tight tracking-[-0.03em] sm:text-5xl">{home.headline}</h1>
            {home.subheadline && <p className="max-w-2xl whitespace-pre-line text-lg text-muted">{home.subheadline}</p>}
            <div className="flex flex-wrap gap-3">
              <Link to="/signin" className={buttonClass('primary')}>
                {home.signin_label}
              </Link>
              {!signUpsClosed && (
                <Link to="/signin?mode=register" className={buttonClass('secondary')}>
                  {home.signup_label}
                </Link>
              )}
            </div>
          </>
        )}
      </section>
      {signedIn && <YourInstance />}
      {signedIn && home.show_events && <YourEvents />}
      {home.show_scoreboards && <PublicScoreboards signedIn={signedIn} />}
      {home.about_text && (
        <Panel className="flex flex-col gap-2">
          {home.about_title && <h2 className="text-lg font-semibold tracking-tight">{home.about_title}</h2>}
          <p className="whitespace-pre-line text-muted">{home.about_text}</p>
        </Panel>
      )}
      {home.footer && <p className="border-t border-line pt-6 text-center text-sm text-muted">{home.footer}</p>}
    </div>
  )
}
