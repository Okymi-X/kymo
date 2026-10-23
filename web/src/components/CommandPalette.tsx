import { Fragment, useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react'
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
    const list: Command[] = [{ id: 'home', label: 'All tournaments', group: 'Pages', run: go('/') }]
    for (const t of data ?? []) list.push({ id: `t-${t.id}`, label: t.name, hint: t.status, group: 'Tournaments', run: go(`/t/${t.slug}`) })
    if (admin) {
      list.push({ id: 'appearance', label: 'Appearance', group: 'Pages', run: go('/settings') })
      list.push({ id: 'tokens', label: 'API tokens', group: 'Pages', run: go('/tokens') })
      list.push({ id: 'out', label: 'Sign out', group: 'Account', run: () => {
          void api.logout().catch(() => undefined)
          session.clear()
          navigate('/')
        } })
    } else {
      list.push({ id: 'in', label: 'Organizer sign in', group: 'Account', run: go('/login') })
    }
    const order = ['Pages', 'Tournaments', 'Account']
    return list.sort((a, b) => order.indexOf(a.group ?? '') - order.indexOf(b.group ?? ''))
  }, [data, admin, navigate])

  const results = useMemo(() => filterCommands(commands, query), [commands, query])

  useEffect(() => {
    document.getElementById(results[active] ? `palette-${results[active].id}` : '')?.scrollIntoView({ block: 'nearest' })
  }, [active, results])

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
      <ul id="palette-list" role="listbox" className="max-h-[22rem] overflow-y-auto p-1.5">
        {results.length === 0 && <li className="px-3 py-6 text-center text-muted">Nothing matches &ldquo;{query}&rdquo;</li>}
        {results.map((c, i) => (
          <Fragment key={c.id}>
            {c.group && c.group !== results[i - 1]?.group && (
              <li role="presentation" className="px-3 pb-1 pt-2.5 text-xs font-medium uppercase tracking-wider text-muted first:pt-1">
                {c.group}
              </li>
            )}
            <li
              id={`palette-${c.id}`}
              role="option"
              aria-selected={i === active}
              onMouseMove={() => setActive(i)}
              onClick={() => choose(c)}
              className={`flex cursor-pointer items-center justify-between gap-3 rounded-control px-3 py-2 ${i === active ? 'bg-accent/10 text-ink' : ''}`}
            >
              <span className="truncate">{c.label}</span>
              {c.hint && <span className="shrink-0 rounded-full border border-line px-2 py-0.5 text-xs text-muted">{c.hint}</span>}
            </li>
          </Fragment>
        ))}
      </ul>
      <p className="flex items-center gap-4 border-t border-line px-4 py-2 text-xs text-muted">
        <span><kbd className="font-mono">Up</kbd> <kbd className="font-mono">Down</kbd> to move</span>
        <span><kbd className="font-mono">Enter</kbd> to open</span>
        <span><kbd className="font-mono">Esc</kbd> to close</span>
      </p>
    </dialog>
  )
}
