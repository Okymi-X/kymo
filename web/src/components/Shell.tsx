import type { ReactNode } from 'react'
import { Link, useNavigate } from 'react-router'
import { session, useIsAdmin } from '../api/client'
import { Button } from './ui'

export function Shell({ children }: { children: ReactNode }) {
  const admin = useIsAdmin()
  const navigate = useNavigate()
  return (
    <div className="min-h-screen">
      <header className="border-b border-line bg-surface/80 backdrop-blur">
        <div className="mx-auto flex h-14 max-w-6xl items-center justify-between px-4">
          <Link to="/" className="text-base font-semibold tracking-tight">
            Kymo
          </Link>
          {admin ? (
            <Button
              variant="ghost"
              onClick={() => {
                session.clear()
                navigate('/')
              }}
            >
              Sign out
            </Button>
          ) : (
            <Link to="/login" className="text-muted hover:text-ink">
              Organizer sign in
            </Link>
          )}
        </div>
      </header>
      <main className="mx-auto max-w-6xl px-4 py-10">{children}</main>
    </div>
  )
}
