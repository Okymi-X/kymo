import { useEffect, useState, type ReactNode } from 'react'
import { Link, useNavigate } from 'react-router'
import { session, useIsAdmin } from '../api/client'
import { DEFAULT_APPEARANCE } from '../lib/theme'
import { CommandPalette } from './CommandPalette'
import { useAppearance } from './ThemeLoader'
import { Button } from './ui'

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
      <header className="border-b border-line bg-surface/80 backdrop-blur">
        <div className="mx-auto flex h-14 max-w-6xl items-center justify-between px-4">
          <Link to="/" aria-label={`${siteName} home`}>
            {siteName === DEFAULT_APPEARANCE.site_name ? (
              <img src="/logo.png" alt="Kymo" width={114} height={40} className="h-10 w-auto" />
            ) : (
              <span className="text-base font-semibold tracking-tight">{siteName}</span>
            )}
          </Link>
          <div className="flex items-center gap-4">
            <button type="button" onClick={() => setPaletteOpen(true)} className="rounded-control border border-line px-2.5 py-1 text-xs text-muted hover:text-ink" aria-label="Open command palette">
              Search <kbd className="ml-1 font-mono">Ctrl K</kbd>
            </button>
          {admin ? (
            <div className="flex items-center gap-4">
              <Link to="/settings" className="text-muted hover:text-ink">
                Appearance
              </Link>
              <Link to="/tokens" className="text-muted hover:text-ink">
                API tokens
              </Link>
              <Button
              variant="ghost"
              onClick={() => {
                session.clear()
                navigate('/')
              }}
            >
              Sign out
              </Button>
            </div>
          ) : (
            <Link to="/login" className="text-muted hover:text-ink">
              Organizer sign in
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
