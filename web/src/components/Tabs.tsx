import { useRef, type KeyboardEvent, type ReactNode } from 'react'

export interface TabItem<T extends string> {
  id: T
  label: string
}

interface Props<T extends string> {
  items: TabItem<T>[]
  value: T
  onChange: (id: T) => void
  label: string
  children: ReactNode
}

/** Accessible tabs: roving focus with arrow keys, one panel. */
export function Tabs<T extends string>({ items, value, onChange, label, children }: Props<T>) {
  const refs = useRef(new Map<T, HTMLButtonElement>())
  const move = (e: KeyboardEvent, index: number) => {
    const step = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0
    if (!step) return
    e.preventDefault()
    const next = items[(index + step + items.length) % items.length]
    onChange(next.id)
    refs.current.get(next.id)?.focus()
  }
  return (
    <div className="flex flex-col gap-6">
      <div role="tablist" aria-label={label} className="flex gap-1 overflow-x-auto overflow-y-hidden border-b border-line">
        {items.map((item, i) => {
          const active = item.id === value
          return (
            <button
              key={item.id}
              ref={(el) => {
                if (el) refs.current.set(item.id, el)
              }}
              role="tab"
              type="button"
              id={`tab-${item.id}`}
              aria-selected={active}
              aria-controls={`panel-${item.id}`}
              tabIndex={active ? 0 : -1}
              onClick={() => onChange(item.id)}
              onKeyDown={(e) => move(e, i)}
              className={`-mb-px whitespace-nowrap border-b-2 px-3.5 py-2.5 text-sm font-medium transition-colors ${active ? 'border-accent text-ink' : 'border-transparent text-muted hover:text-ink'}`}
            >
              {item.label}
            </button>
          )
        })}
      </div>
      <div role="tabpanel" id={`panel-${value}`} aria-labelledby={`tab-${value}`}>
        {children}
      </div>
    </div>
  )
}
