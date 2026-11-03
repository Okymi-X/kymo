import { Menu, Search, X } from 'lucide-react'
import { useEffect, useRef, useState, type ReactNode } from 'react'
import { Link, Outlet, useLocation, useNavigate } from 'react-router'
import { useTournaments } from '../api/hooks'
import { session, useIsAdmin, useIsReferee, useWho } from '../api/client'
import { DEFAULT_APPEARANCE } from '../lib/theme'
import { AccountMenu } from './AccountMenu'
import { CommandPalette } from './CommandPalette'
import { ShortcutsDialog } from './ShortcutsDialog'
import { HeaderProvider, useHeaderInfo } from './HeaderContext'
import { ThemeToggle } from './ThemeToggle'
import { useAppearance } from './ThemeLoader'
import { StatusPill } from './ui'

const shortcut = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform) ? 'Cmd K' : 'Ctrl K'

function Brand({ siteName, logo }: { siteName: string; logo: string | null }) {
  return (
    <Link to="/" aria-label={`${siteName} home`} className="shrink-0">
      {logo ? (
        <img src={logo} alt={siteName} className="h-9 w-auto max-w-48 object-contain" />
      ) : siteName === DEFAULT_APPEARANCE.site_name ? (
        <img src="/logo.png" alt="Kymo" width={114} height={40} className="h-10 w-auto" />
      ) : (
        <span className="text-base font-semibold tracking-tight">{siteName}</span>
      )}
    </Link>
  )
}

/** The title of the page being read, shown in the header once the page's own title scrolls away. */
function Context() {
  const info = useHeaderInfo()
  if (!info) return null
  return (
    <div className="header-pop hidden min-w-0 items-center gap-2.5 md:flex" aria-live="polite">
      <span className="truncate font-medium">{info.title}</span>
      <StatusPill status={info.status} />
      {info.progress && info.progress.total > 0 && (
        <span className="font-mono text-xs tabular-nums text-muted">
          {info.progress.played}/{info.progress.total}
        </span>
      )}
    </div>
  )
}

/** How many tournaments are live right now, linking to that filter. */
function LiveBadge() {
  const live = (useTournaments().data ?? []).filter((t) => t.status === 'live').length
  if (live === 0) return null
  return (
    <Link
      to="/?status=live"
      className="hidden h-9 items-center gap-2 rounded-full border border-line px-3 text-sm font-medium text-ink transition-colors hover:bg-line/50 md:flex"
      aria-label={`${live} tournaments live now`}
    >
      <span className="relative flex size-2" aria-hidden="true">
        <span className="live-dot absolute inline-flex size-full rounded-full bg-win" />
        <span className="relative inline-flex size-2 rounded-full bg-win" />
      </span>
      {live} live
    </Link>
  )
}

const NAV: { to: string; label: string; match: (path: string) => boolean }[] = [
  { to: '/', label: 'Tournaments', match: (p) => p === '/' || p.startsWith('/t/') },
  { to: '/events', label: 'Events', match: (p) => p.startsWith('/events') || p.startsWith('/e/') },
  { to: '/ladder', label: 'Ladder', match: (p) => p === '/ladder' },
]

/** The primary links on a phone, where the header has no room for them. */
function MobileNav({ signedOut }: { signedOut: boolean }) {
  const [open, setOpen] = useState(false)
  const root = useRef<HTMLDivElement>(null)
  const { pathname } = useLocation()
  useEffect(() => setOpen(false), [pathname])
  useEffect(() => {
    if (!open) return
    const close = (e: Event) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false)
    }
    const key = (e: KeyboardEvent) => e.key === 'Escape' && setOpen(false)
    document.addEventListener('pointerdown', close)
    document.addEventListener('keydown', key)
    return () => {
      document.removeEventListener('pointerdown', close)
      document.removeEventListener('keydown', key)
    }
  }, [open])
  return (
    <div ref={root} className="sm:hidden">
      <button
        type="button"
        aria-label={open ? 'Close menu' : 'Open menu'}
        aria-expanded={open}
        aria-controls="mobile-nav"
        onClick={() => setOpen((v) => !v)}
        className="-ml-2 flex size-10 items-center justify-center rounded-control text-muted transition-colors hover:bg-line/50 hover:text-ink"
      >
        {open ? <X size={20} aria-hidden="true" /> : <Menu size={20} aria-hidden="true" />}
      </button>
      {open && (
        <nav id="mobile-nav" aria-label="Menu" className="menu-pop absolute inset-x-0 top-14 z-30 flex flex-col gap-1 border-b border-line bg-surface p-3 shadow-[var(--shadow-lift)]">
          {NAV.map((item) => (
            <Link
              key={item.to}
              to={item.to}
              aria-current={item.match(pathname) ? 'page' : undefined}
              className={`rounded-control px-3 py-3 text-base font-medium ${item.match(pathname) ? 'bg-line/60 text-ink' : 'text-muted hover:bg-line/40 hover:text-ink'}`}
            >
              {item.label}
            </Link>
          ))}
          {signedOut && (
            <Link to="/signin" className="mt-1 rounded-control bg-accent px-3 py-3 text-center text-base font-medium text-bg">
              Sign in
            </Link>
          )}
        </nav>
      )}
    </div>
  )
}

function Header({ onSearch }: { onSearch: () => void }) {
  const [scrolled, setScrolled] = useState(false)
  useEffect(() => {
    const onScroll = () => setScrolled(window.scrollY > 4)
    onScroll()
    window.addEventListener('scroll', onScroll, { passive: true })
    return () => window.removeEventListener('scroll', onScroll)
  }, [])
  const admin = useIsAdmin()
  const referee = useIsReferee()
  const who = useWho()
  const { pathname } = useLocation()
  const navigate = useNavigate()
  const look = useAppearance().data
  const siteName = look?.site_name ?? DEFAULT_APPEARANCE.site_name
  const onTournaments = pathname === '/' || pathname.startsWith('/t/')
  const onEvents = pathname.startsWith('/events') || pathname.startsWith('/e/')
  const info = useHeaderInfo()
  return (
    <header className={`no-print sticky top-0 z-20 border-b backdrop-blur-xl transition-[box-shadow,border-color,background-color] duration-200 supports-[backdrop-filter]:bg-surface/70 ${scrolled ? 'border-line bg-surface/90 shadow-[var(--shadow-lift)]' : 'border-transparent bg-surface/80'}`}>
      <div className="mx-auto flex h-14 max-w-6xl items-center gap-3 px-4">
        <MobileNav signedOut={!admin && !who && !referee} />
        <Brand siteName={siteName} logo={look?.logo ?? null} />
        <nav aria-label="Primary" className={`hidden items-center sm:flex ${info ? 'md:hidden lg:flex' : ''}`}>
          <Link
            to="/"
            aria-current={onTournaments ? 'page' : undefined}
            className={`rounded-control px-3 py-1.5 text-sm font-medium transition-colors ${onTournaments ? 'bg-line/60 text-ink' : 'text-muted hover:bg-line/40 hover:text-ink'}`}
          >
            Tournaments
          </Link>
          <Link
            to="/events"
            aria-current={onEvents ? 'page' : undefined}
            className={`rounded-control px-3 py-1.5 text-sm font-medium transition-colors ${onEvents ? 'bg-line/60 text-ink' : 'text-muted hover:bg-line/40 hover:text-ink'}`}
          >
            Events
          </Link>
          <Link
            to="/ladder"
            aria-current={pathname === '/ladder' ? 'page' : undefined}
            className={`rounded-control px-3 py-1.5 text-sm font-medium transition-colors ${pathname === '/ladder' ? 'bg-line/60 text-ink' : 'text-muted hover:bg-line/40 hover:text-ink'}`}
          >
            Ladder
          </Link>
        </nav>
        <div className="flex min-w-0 flex-1 justify-center">
          <Context />
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <LiveBadge />
          <button
            type="button"
            onClick={onSearch}
            aria-label="Open command palette"
            className="flex h-9 items-center gap-2 rounded-control border border-line bg-field/60 px-2.5 text-sm text-muted transition-colors hover:border-field-line hover:text-ink lg:w-60"
          >
            <Search size={15} aria-hidden="true" />
            <span className="hidden flex-1 text-left lg:inline">Search Kymo</span>
            <kbd className="hidden rounded border border-line px-1.5 py-0.5 font-mono text-[11px] sm:inline">{shortcut}</kbd>
          </button>
          <ThemeToggle />
          {admin || who ? (
            <AccountMenu />
          ) : referee ? (
            <button
              type="button"
              onClick={() => {
                session.clear()
                navigate('/')
              }}
              className="whitespace-nowrap rounded-control border border-line px-2.5 py-1.5 text-sm text-muted transition-colors hover:text-ink"
            >
              Referee, sign out
            </button>
          ) : (
            !pathname.startsWith('/signin') && (
              <Link to="/signin" className="whitespace-nowrap rounded-control bg-accent px-3 py-1.5 text-sm font-medium text-bg transition-opacity hover:opacity-90">
                Sign in
              </Link>
            )
          )}
        </div>
      </div>
    </header>
  )
}

export function Shell({ children }: { children?: ReactNode }) {
  const [paletteOpen, setPaletteOpen] = useState(false)
  const [help, setHelp] = useState(false)

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault()
        setPaletteOpen((open) => !open)
      } else if (e.key === '?' && !(e.target instanceof HTMLElement && e.target.closest('input, textarea, select, [contenteditable]'))) {
        setHelp(true)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  return (
    <HeaderProvider>
      <div className="min-h-screen">
        <a href="#main" className="sr-only focus:not-sr-only focus:fixed focus:left-4 focus:top-3 focus:z-50 focus:rounded-control focus:bg-surface focus:px-4 focus:py-2 focus:font-medium focus:shadow-[var(--shadow-lift)]">
          Skip to content
        </a>
        <Header onSearch={() => setPaletteOpen(true)} />
        <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} />
        {help && <ShortcutsDialog onClose={() => setHelp(false)} />}
        <main id="main" tabIndex={-1} className="mx-auto max-w-6xl px-4 py-10 focus:outline-none">{children ?? <Outlet />}</main>
      </div>
    </HeaderProvider>
  )
}
