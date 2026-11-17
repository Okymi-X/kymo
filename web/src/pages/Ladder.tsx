import { useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { api } from '../api/client'
import { Avatar } from '../components/Avatar'
import { LadderChart } from '../components/LadderChart'
import { Pager } from '../components/Pager'
import { PageTitle, SearchField } from '../components/ui'
import { slicePage, usePagination } from '../lib/paginate'
import { usePageTitle } from '../lib/usePageTitle'

const PAGE_SIZE = 10

/** Every team's results across all tournaments on this site. Teams are the same team when the names match. */
export function Ladder() {
  usePageTitle('Ladder')
  const { data, isPending, error } = useQuery({ queryKey: ['ladder'], queryFn: api.ladder })
  const [query, setQuery] = useState('')
  const needle = query.trim().toLowerCase()
  const ranked = useMemo(() => (data ?? []).map((line, i) => ({ line, rank: i + 1 })), [data])
  const shown = useMemo(() => (needle ? ranked.filter((x) => x.line.name.toLowerCase().includes(needle)) : ranked), [ranked, needle])
  const paging = usePagination(shown.length, PAGE_SIZE, needle)
  const rows = slicePage(shown, paging.page, PAGE_SIZE)
  return (
    <div className="flex flex-col gap-8">
      <div className="flex flex-col gap-1.5">
        <PageTitle>Ladder</PageTitle>
        <p className="text-sm text-muted">3 points a win, 1 a draw, 10 a title. Teams with the same name count as one.</p>
      </div>
      <LadderChart />
      {isPending && <p className="text-muted">Loading</p>}
      {error && <p role="alert" className="text-danger">The ladder could not be loaded.</p>}
      {data && data.length === 0 && <p className="text-muted">The ladder fills in as matches are played.</p>}
      {data && data.length > 0 && (
        <section aria-label="Ranking" className="flex flex-col gap-3">
          {data.length > PAGE_SIZE && <SearchField label="Search teams" placeholder="Search teams" value={query} onChange={(e) => setQuery(e.target.value)} className="max-w-xs" />}
          <div className="overflow-x-auto rounded-card border border-line bg-surface">
            <table className="w-full min-w-[20rem] text-sm">
              <caption className="sr-only">Ladder across all tournaments</caption>
              <thead>
                <tr className="border-b border-line text-left text-xs text-muted">
                  <th scope="col" className="w-10 px-4 py-3 font-medium">#</th>
                  <th scope="col" className="px-2 py-3 font-medium">Team</th>
                  <th scope="col" className="hidden px-3 py-3 text-right font-medium sm:table-cell">Played</th>
                  <th scope="col" className="px-3 py-3 text-right font-medium">Won</th>
                  <th scope="col" className="px-3 py-3 text-right font-medium">Titles</th>
                  <th scope="col" className="px-4 py-3 text-right font-medium">Points</th>
                </tr>
              </thead>
              <tbody>
                {rows.map(({ line, rank }) => (
                  <tr key={line.name.toLowerCase()} className="border-b border-line last:border-0">
                    <td className={`px-4 py-3 font-mono tabular-nums ${rank <= 3 ? 'font-semibold text-ink' : 'text-muted'}`}>{rank}</td>
                    <td className="px-2 py-2.5">
                      <span className="flex items-center gap-2.5 font-medium">
                        <Avatar name={line.name} size={24} />
                        <span className="max-w-[10rem] truncate sm:max-w-[18rem]">{line.name}</span>
                      </span>
                    </td>
                    <td className="hidden px-3 py-3 text-right font-mono tabular-nums text-muted sm:table-cell">{line.played}</td>
                    <td className="px-3 py-3 text-right font-mono tabular-nums">{line.wins}</td>
                    <td className="px-3 py-3 text-right font-mono tabular-nums">{line.titles || <span className="text-muted">·</span>}</td>
                    <td className="px-4 py-3 text-right font-mono font-semibold tabular-nums">{line.points}</td>
                  </tr>
                ))}
                {rows.length === 0 && (
                  <tr>
                    <td colSpan={6} className="px-4 py-8 text-center text-muted">No team matches that search.</td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
          <Pager {...paging} total={shown.length} onChange={paging.setPage} noun="teams" />
        </section>
      )}
    </div>
  )
}
