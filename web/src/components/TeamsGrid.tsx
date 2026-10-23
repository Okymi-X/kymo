import { useMemo, useState } from 'react'
import { slicePage, usePagination } from '../lib/paginate'
import type { TeamRow } from '../lib/stats'
import { Avatar } from './Avatar'
import { Pager } from './Pager'
import { StatusBadge } from './StatusBadge'
import { SearchField } from './ui'

const PAGE_SIZE = 24

export function TeamsGrid({ rows, onSelect }: { rows: TeamRow[]; onSelect: (teamId: number) => void }) {
  const [query, setQuery] = useState('')
  const needle = query.trim().toLowerCase()
  const filtered = useMemo(() => (needle ? rows.filter((r) => r.team.name.toLowerCase().includes(needle)) : rows), [rows, needle])
  const paging = usePagination(filtered.length, PAGE_SIZE, needle)
  return (
    <div className="flex flex-col gap-4">
      {rows.length > 12 && <SearchField label="Search teams" placeholder="Search teams" value={query} onChange={(e) => setQuery(e.target.value)} className="max-w-xs" />}
      {filtered.length === 0 && <p className="text-muted">No team matches that search.</p>}
    <ul className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
      {slicePage(filtered, paging.page, PAGE_SIZE).map((r) => (
        <li key={r.team.id} className="min-w-0">
          <button
            type="button"
            onClick={() => onSelect(r.team.id)}
            className="flex w-full items-center gap-3 rounded-card border border-line bg-surface p-4 text-left shadow-[var(--shadow-card)] transition-[border-color,box-shadow,transform] duration-150 hover:border-field-line hover:shadow-[var(--shadow-lift)] active:scale-[0.99]"
          >
            <Avatar name={r.team.name} size={40} />
            <span className="min-w-0 flex-1">
              <span className="block truncate font-medium">{r.team.name}</span>
              <span className="block font-mono text-xs tabular-nums text-muted">
                Seed {r.team.seed} · {r.wins}-{r.losses}
              </span>
            </span>
            <StatusBadge status={r.status} />
          </button>
        </li>
      ))}
    </ul>
      <Pager {...paging} total={filtered.length} onChange={paging.setPage} noun="teams" />
    </div>
  )
}
