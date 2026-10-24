import { useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { api } from '../api/client'
import { Avatar } from '../components/Avatar'
import { BackLink } from '../components/BackLink'
import { Pager } from '../components/Pager'
import { PageTitle, SearchField } from '../components/ui'
import { slicePage, usePagination } from '../lib/paginate'
import { usePageTitle } from '../lib/usePageTitle'

const PAGE_SIZE = 15

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
  const num = 'px-2 py-2.5 text-right font-mono tabular-nums md:px-3'
  return (
    <div className="flex flex-col gap-6">
      <div>
        <BackLink>Tournaments</BackLink>
        <PageTitle>Ladder</PageTitle>
        <p className="mt-2 max-w-prose text-muted">Points across every tournament here: 3 for a win, 1 for a draw and 10 for winning a tournament. Teams with the same name count as one team.</p>
      </div>
      {isPending && <p className="text-muted">Loading</p>}
      {error && <p role="alert" className="text-danger">The ladder could not be loaded.</p>}
      {data && data.length === 0 && <p className="text-muted">No results yet. The ladder fills in as matches are played.</p>}
      {data && data.length > 0 && (
        <>
          {data.length > PAGE_SIZE && <SearchField label="Search teams" placeholder="Search teams" value={query} onChange={(e) => setQuery(e.target.value)} className="max-w-xs" />}
          <div className="overflow-x-auto rounded-card border border-line bg-surface">
            <table className="w-full min-w-[22rem] text-sm">
              <caption className="sr-only">Ladder across all tournaments</caption>
              <thead>
                <tr className="border-b border-line text-left text-xs text-muted">
                  <th scope="col" className="w-8 px-2 py-3 font-medium md:w-10 md:px-4">#</th>
                  <th scope="col" className="px-2 py-3 font-medium">Team</th>
                  <th scope="col" className="hidden px-3 py-3 text-right font-medium md:table-cell">Events</th>
                  <th scope="col" className="px-2 py-3 text-right font-medium md:px-3">P</th>
                  <th scope="col" className="px-2 py-3 text-right font-medium md:px-3">W</th>
                  <th scope="col" className="hidden px-3 py-3 text-right font-medium md:table-cell">D</th>
                  <th scope="col" className="px-2 py-3 text-right font-medium md:px-3">Titles</th>
                  <th scope="col" className="px-2 py-3 text-right font-medium md:px-3">Pts</th>
                </tr>
              </thead>
              <tbody>
                {rows.map(({ line, rank }) => (
                  <tr key={line.name.toLowerCase()} className="border-b border-line last:border-0">
                    <td className="px-2 py-2.5 font-mono tabular-nums text-muted md:px-4">{rank}</td>
                    <td className="px-2 py-2">
                      <span className="flex items-center gap-2.5 font-medium">
                        <Avatar name={line.name} size={24} />
                        <span className="max-w-[7rem] truncate md:max-w-[16rem]">{line.name}</span>
                      </span>
                    </td>
                    <td className={`hidden md:table-cell ${num}`}>{line.tournaments}</td>
                    <td className={num}>{line.played}</td>
                    <td className={num}>{line.wins}</td>
                    <td className={`hidden md:table-cell ${num}`}>{line.draws}</td>
                    <td className={num}>{line.titles}</td>
                    <td className={`${num} font-semibold`}>{line.points}</td>
                  </tr>
                ))}
                {rows.length === 0 && (
                  <tr>
                    <td colSpan={8} className="px-4 py-8 text-center text-muted">No team matches that search.</td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
          <Pager {...paging} total={shown.length} onChange={paging.setPage} noun="teams" />
        </>
      )}
    </div>
  )
}
