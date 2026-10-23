import { Search } from 'lucide-react'
import { useEffect, useState, type ReactNode } from 'react'
import { Link, useNavigate } from 'react-router'
import { api, session, useIsAdmin } from '../api/client'
import { DEFAULT_APPEARANCE } from '../lib/theme'
import { CommandPalette } from './CommandPalette'
import { useAppearance } from './ThemeLoader'
import { Button } from './ui'

const shortcut = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform) ? 'Cmd K' : 'Ctrl K'

export function Shell({ children }: { children: ReactNode }) {
  const admin = useIsAdmin()
  const navigate = useNavigate()
  const [paletteOpen, setPaletteOpen] = useState(false)
  const siteName = useAppearance().data?.site_name ?? DEFAULT_APPEARANCE.site_name

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
    <div className="min-h-screen">
      <header className="sticky top-0 z-20 border-b border-line bg-surface/80 backdrop-blur-md supports-[backdrop-filter]:bg-surface/70">
        <div className="mx-auto flex h-14 max-w-6xl items-center justify-between px-4">
          <Link to="/" aria-label={`${siteName} home`}>
            {siteName === DEFAULT_APPEARANCE.site_name ? (
              <img src="/logo.png" alt="Kymo" width={114} height={40} className="h-10 w-auto" />
            ) : (
              <span className="text-base font-semibold tracking-tight">{siteName}</span>
            )}
          </Link>
          <div className="flex items-center gap-2 sm:gap-4">
            <button
              type="button"
              onClick={() => setPaletteOpen(true)}
              className="flex h-9 items-center gap-2 rounded-control border border-line px-2.5 text-xs text-muted hover:text-ink"
              aria-label="Open command palette"
            >
              <Search size={15} aria-hidden="true" />
              <span className="hidden sm:inline">Search</span>
              <kbd className="hidden font-mono sm:inline">{shortcut}</kbd>
            </button>
            {admin ? (
              <>
                <Link to="/settings" className="hidden text-muted hover:text-ink sm:inline">
                  Appearance
                </Link>
                <Link to="/tokens" className="hidden whitespace-nowrap text-muted hover:text-ink sm:inline">
                  API tokens
                </Link>
                <Button
                  variant="ghost"
                  className="whitespace-nowrap px-2.5"
                  onClick={() => {
                    void api.logout().catch(() => undefined)
                    session.clear()
                    navigate('/')
                  }}
                >
                  Sign out
                </Button>
              </>
            ) : (
              <Link to="/login" className="whitespace-nowrap text-muted hover:text-ink">
                <span className="sm:hidden">Sign in</span>
                <span className="hidden sm:inline">Organizer sign in</span>
              </Link>
            )}
          </div>
        </div>
      </header>
      <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} />
      <main className="mx-auto max-w-6xl px-4 py-10">{children}</main>
    </div>
  )
}
