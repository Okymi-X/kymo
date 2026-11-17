import { LogOut, ExternalLink } from 'lucide-react'
import { Suspense } from 'react'
import { Link, NavLink, Outlet, useNavigate } from 'react-router'
import { api, session, useWho } from '../../api/client'
import { useMyPermissions } from '../../api/platformHooks'
import { ORG, ORG_LINKS } from '../../lib/organizer'
import { useOpenAlerts } from '../monitor/useOpenAlerts'
import { ThemeToggle } from '../ThemeToggle'
import { useAppearance } from '../ThemeLoader'

const GROUPS = ['Run', 'People', 'Platform'] as const
const LINK = 'flex items-center justify-between gap-2 rounded-control px-3 py-2 text-sm font-medium transition-colors'

/** The organizer's own frame: no player header, one menu that lists only what the role may do. */
export function OrganizerShell() {
  const mine = useMyPermissions()
  const who = useWho()
  const navigate = useNavigate()
  const site = useAppearance().data?.site_name ?? 'Kymo'
  const granted = new Set(mine.data?.permissions ?? [])
  const links = ORG_LINKS.filter((l) => granted.has(l.needs))
  const openAlerts = useOpenAlerts(granted.has('monitor'))
  const signOut = () => {
    void api.logout().catch(() => undefined)
    session.clear()
    navigate(`${ORG}/login`)
  }
  return (
    <div className="min-h-screen bg-bg">
      <a href="#main" className="sr-only focus:not-sr-only focus:fixed focus:left-4 focus:top-3 focus:z-50 focus:rounded-control focus:bg-surface focus:px-4 focus:py-2 focus:font-medium">
        Skip to content
      </a>
      <header className="no-print sticky top-0 z-20 border-b border-line bg-surface/90 backdrop-blur-xl">
        <div className="mx-auto flex h-14 max-w-7xl items-center gap-3 px-4">
          <Link to={ORG} className="flex items-baseline gap-2">
            <span className="text-base font-semibold tracking-tight">{site}</span>
            <span className="rounded-full border border-line px-2 py-0.5 text-xs font-medium text-muted">Organizer</span>
          </Link>
          <div className="flex-1" />
          <Link to="/events" className="hidden items-center gap-1.5 rounded-control px-2.5 py-1.5 text-sm text-muted hover:text-ink sm:flex">
            Player site <ExternalLink size={14} aria-hidden="true" />
          </Link>
          <ThemeToggle />
          <span className="hidden text-sm text-muted sm:inline">{who ? `${who.username} (${who.role})` : 'organizer'}</span>
          <button type="button" onClick={signOut} className="flex items-center gap-1.5 rounded-control border border-line px-2.5 py-1.5 text-sm text-muted transition-colors hover:text-ink">
            <LogOut size={14} aria-hidden="true" /> Sign out
          </button>
        </div>
      </header>
      <div className="mx-auto flex max-w-7xl flex-col gap-4 px-4 py-6 lg:flex-row lg:gap-8">
        <nav aria-label="Organizer" className="no-print lg:sticky lg:top-20 lg:w-52 lg:shrink-0 lg:self-start">
          <ul className="flex gap-1 overflow-x-auto pb-1 lg:flex-col lg:gap-5 lg:overflow-visible lg:pb-0">
            {GROUPS.map((group) => {
              const items = links.filter((l) => l.group === group)
              if (items.length === 0) return null
              return (
                <li key={group} className="flex gap-1 lg:flex-col">
                  <span className="sr-only px-3 pb-1 text-xs font-semibold uppercase tracking-wide text-muted lg:not-sr-only">{group}</span>
                  {items.map((l) => (
                    <NavLink
                      key={l.to}
                      to={l.to}
                      className={({ isActive }) => `${LINK} whitespace-nowrap ${isActive ? 'bg-line/60 text-ink' : 'text-muted hover:bg-line/40 hover:text-ink'}`}
                    >
                      {l.label}
                      {l.needs === 'monitor' && openAlerts > 0 && <span className="rounded-full bg-danger px-1.5 text-xs font-semibold text-bg">{openAlerts}</span>}
                    </NavLink>
                  ))}
                </li>
              )
            })}
          </ul>
        </nav>
        <main id="main" tabIndex={-1} className="min-w-0 flex-1 focus:outline-none">
          <Suspense fallback={<p role="status" className="py-10 text-center text-muted">Loading...</p>}>
            <Outlet />
          </Suspense>
        </main>
      </div>
    </div>
  )
}
