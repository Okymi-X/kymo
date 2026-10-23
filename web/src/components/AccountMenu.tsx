import { ChevronDown, KeyRound, LogOut, Palette } from 'lucide-react'
import { useEffect, useRef, useState, type KeyboardEvent } from 'react'
import { Link, useNavigate } from 'react-router'
import { api, session } from '../api/client'

const ITEM = 'flex w-full items-center gap-2.5 rounded-[calc(var(--radius-control)-2px)] px-3 py-2 text-left text-sm hover:bg-line/60 focus:bg-line/60 focus:outline-none'

/** Organizer menu: appearance, tokens and sign out behind one button. */
export function AccountMenu() {
  const [open, setOpen] = useState(false)
  const root = useRef<HTMLDivElement>(null)
  const navigate = useNavigate()

  useEffect(() => {
    if (!open) return
    const close = (e: Event) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false)
    }
    document.addEventListener('pointerdown', close)
    return () => document.removeEventListener('pointerdown', close)
  }, [open])

  useEffect(() => {
    if (open) root.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus()
  }, [open])

  const onKeyDown = (e: KeyboardEvent) => {
    const items = [...(root.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])]
    const index = items.indexOf(document.activeElement as HTMLElement)
    if (e.key === 'Escape') {
      setOpen(false)
      root.current?.querySelector<HTMLElement>('button[aria-haspopup]')?.focus()
    } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault()
      const step = e.key === 'ArrowDown' ? 1 : -1
      items[(index + step + items.length) % items.length]?.focus()
    }
  }

  const signOut = () => {
    setOpen(false)
    void api.logout().catch(() => undefined)
    session.clear()
    navigate('/')
  }

  return (
    <div ref={root} className="relative" onKeyDown={onKeyDown}>
      <button
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label="Organizer menu"
        onClick={() => setOpen((v) => !v)}
        className="flex h-9 items-center gap-1.5 rounded-full border border-line pl-1 pr-2 text-muted transition-colors hover:text-ink"
      >
        <span className="flex size-7 items-center justify-center rounded-full bg-accent text-xs font-semibold text-bg" aria-hidden="true">
          O
        </span>
        <ChevronDown size={14} aria-hidden="true" className={`transition-transform duration-150 ${open ? 'rotate-180' : ''}`} />
      </button>
      {open && (
        <div role="menu" aria-label="Organizer" className="menu-pop absolute right-0 top-11 z-30 w-52 origin-top-right rounded-card border border-line bg-surface p-1.5 shadow-[var(--shadow-lift)]">
          <p className="px-3 pb-1 pt-1.5 text-xs text-muted">Signed in as organizer</p>
          <Link role="menuitem" to="/settings" className={ITEM} onClick={() => setOpen(false)}>
            <Palette size={16} aria-hidden="true" /> Appearance
          </Link>
          <Link role="menuitem" to="/tokens" className={ITEM} onClick={() => setOpen(false)}>
            <KeyRound size={16} aria-hidden="true" /> API tokens
          </Link>
          <div className="my-1 border-t border-line" />
          <button role="menuitem" type="button" className={ITEM} onClick={signOut}>
            <LogOut size={16} aria-hidden="true" /> Sign out
          </button>
        </div>
      )}
    </div>
  )
}
