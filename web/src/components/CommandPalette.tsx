import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react'
import { useNavigate } from 'react-router'
import { api, session, useIsAdmin } from '../api/client'
import { useTournaments } from '../api/hooks'
import { filterCommands, type Command } from '../lib/palette'

export function CommandPalette({ open, onClose }: { open: boolean; onClose: () => void }) {
  const ref = useRef<HTMLDialogElement>(null)
  const [query, setQuery] = useState('')
  const [active, setActive] = useState(0)
  const navigate = useNavigate()
  const admin = useIsAdmin()
  const { data } = useTournaments()

  const commands = useMemo<Command[]>(() => {
    const go = (path: string) => () => navigate(path)
    const list: Command[] = [{ id: 'home', label: 'All tournaments', run: go('/') }]
    for (const t of data ?? []) list.push({ id: `t-${t.id}`, label: t.name, hint: t.status, run: go(`/t/${t.slug}`) })
    if (admin) {
      list.push({ id: 'appearance', label: 'Appearance', run: go('/settings') })
      list.push({ id: 'tokens', label: 'API tokens', run: go('/tokens') })
      list.push({ id: 'out', label: 'Sign out', run: () => {
          void api.logout().catch(() => undefined)
          session.clear()
          navigate('/')
        } })
    } else {
      list.push({ id: 'in', label: 'Organizer sign in', run: go('/login') })
    }
    return list
  }, [data, admin, navigate])

  const results = useMemo(() => filterCommands(commands, query), [commands, query])

  useEffect(() => {
    const dialog = ref.current
    if (!dialog) return
    if (open && !dialog.open) {
      setQuery('')
      setActive(0)
      dialog.showModal()
    } else if (!open && dialog.open) {
      dialog.close()
    }
  }, [open])

  const choose = (command: Command | undefined) => {
    if (!command) return
    onClose()
    command.run()
  }

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      setActive((i) => Math.min(i + 1, results.length - 1))
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      setActive((i) => Math.max(i - 1, 0))
    } else if (e.key === 'Enter') {
      e.preventDefault()
      choose(results[active])
    }
  }

  return (
    <dialog
      ref={ref}
      onClose={onClose}
      onClick={(e) => e.target === ref.current && onClose()}
      aria-label="Command palette"
      className="mx-auto mt-[15vh] w-[min(92vw,32rem)] rounded-card border border-line bg-surface p-0 text-ink"
    >
      <input
        role="combobox"
        aria-expanded="true"
        aria-controls="palette-list"
        aria-activedescendant={results[active] ? `palette-${results[active].id}` : undefined}
        value={query}
        onChange={(e) => {
          setQuery(e.target.value)
          setActive(0)
        }}
        onKeyDown={onKeyDown}
        placeholder="Go to a tournament or page"
        className="w-full border-b border-line bg-transparent px-4 py-3 text-base placeholder:text-muted focus:outline-none"
      />
      <ul id="palette-list" role="listbox" className="max-h-72 overflow-y-auto p-1.5">
        {results.length === 0 && <li className="px-3 py-2 text-muted">No matches</li>}
        {results.map((c, i) => (
          <li
            key={c.id}
            id={`palette-${c.id}`}
            role="option"
            aria-selected={i === active}
            onMouseMove={() => setActive(i)}
            onClick={() => choose(c)}
            className={`flex cursor-pointer items-center justify-between rounded-control px-3 py-2 ${i === active ? 'bg-line/60' : ''}`}
          >
            <span className="truncate">{c.label}</span>
            {c.hint && <span className="text-xs text-muted">{c.hint}</span>}
          </li>
        ))}
      </ul>
    </dialog>
  )
}
