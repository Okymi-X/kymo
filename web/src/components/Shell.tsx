import { Search } from 'lucide-react'
import { useEffect, useState, type ReactNode } from 'react'
import { Link, Outlet, useLocation } from 'react-router'
import { useIsAdmin } from '../api/client'
import { DEFAULT_APPEARANCE } from '../lib/theme'
import { AccountMenu } from './AccountMenu'
import { CommandPalette } from './CommandPalette'
import { HeaderProvider, useHeaderInfo } from './HeaderContext'
import { useAppearance } from './ThemeLoader'
import { StatusPill } from './ui'

const shortcut = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform) ? 'Cmd K' : 'Ctrl K'

function Brand({ siteName }: { siteName: string }) {
  return (
    <Link to="/" aria-label={`${siteName} home`} className="shrink-0">
      {siteName === DEFAULT_APPEARANCE.site_name ? (
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

function Header({ onSearch }: { onSearch: () => void }) {
  const admin = useIsAdmin()
  const { pathname } = useLocation()
  const siteName = useAppearance().data?.site_name ?? DEFAULT_APPEARANCE.site_name
  const onTournaments = pathname === '/' || pathname.startsWith('/t/')
  const info = useHeaderInfo()
  return (
    <header className="sticky top-0 z-20 border-b border-line bg-surface/80 backdrop-blur-md supports-[backdrop-filter]:bg-surface/70">
      <div className="mx-auto flex h-14 max-w-6xl items-center gap-3 px-4">
        <Brand siteName={siteName} />
        <nav aria-label="Primary" className={`hidden items-center sm:flex ${info ? 'md:hidden lg:flex' : ''}`}>
          <Link
            to="/"
            aria-current={onTournaments ? 'page' : undefined}
            className={`relative px-3 py-4 text-sm font-medium transition-colors ${onTournaments ? 'text-ink after:absolute after:inset-x-3 after:bottom-0 after:h-0.5 after:rounded-full after:bg-accent' : 'text-muted hover:text-ink'}`}
          >
            Tournaments
          </Link>
        </nav>
        <div className="flex min-w-0 flex-1 justify-center">
          <Context />
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <button
            type="button"
            onClick={onSearch}
            aria-label="Open command palette"
            className="flex h-9 items-center gap-2 rounded-control border border-line px-2.5 text-xs text-muted transition-colors hover:text-ink"
          >
            <Search size={15} aria-hidden="true" />
            <span className="hidden sm:inline">Search</span>
            <kbd className="hidden font-mono sm:inline">{shortcut}</kbd>
          </button>
          {admin ? (
            <AccountMenu />
          ) : (
            <Link to="/login" className="whitespace-nowrap px-1 text-sm text-muted hover:text-ink">
              <span className="sm:hidden">Sign in</span>
              <span className="hidden sm:inline">Organizer sign in</span>
            </Link>
          )}
        </div>
      </div>
    </header>
  )
}

export function Shell({ children }: { children?: ReactNode }) {
  const [paletteOpen, setPaletteOpen] = useState(false)

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault()
        setPaletteOpen((open) => !open)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  return (
    <HeaderProvider>
      <div className="min-h-screen">
        <Header onSearch={() => setPaletteOpen(true)} />
        <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} />
        <main className="mx-auto max-w-6xl px-4 py-10">{children ?? <Outlet />}</main>
      </div>
    </HeaderProvider>
  )
}
