import { useEffect, useState } from 'react'

export const pageCount = (total: number, size: number) => Math.max(1, Math.ceil(total / size))

export const clampPage = (page: number, total: number, size: number) => Math.min(Math.max(1, page), pageCount(total, size))

export const slicePage = <T,>(items: T[], page: number, size: number): T[] => {
  const current = clampPage(page, items.length, size)
  return items.slice((current - 1) * size, current * size)
}

/** Page numbers to show with gaps, for example 1 … 4 5 6 … 20. */
export function pageWindow(page: number, pages: number, span = 1): (number | 'gap')[] {
  const wanted = new Set([1, pages, page])
  for (let i = 1; i <= span; i++) {
    wanted.add(page - i)
    wanted.add(page + i)
  }
  const sorted = [...wanted].filter((p) => p >= 1 && p <= pages).sort((a, b) => a - b)
  const out: (number | 'gap')[] = []
  sorted.forEach((p, i) => {
    if (i > 0 && p - sorted[i - 1] > 1) out.push('gap')
    out.push(p)
  })
  return out
}

/** Current page that returns to the first page whenever `resetKey` changes, for example a search term. */
export function usePagination(total: number, size: number, resetKey: string = '') {
  const [page, setPage] = useState(1)
  useEffect(() => setPage(1), [resetKey])
  const pages = pageCount(total, size)
  const current = clampPage(page, total, size)
  const from = total === 0 ? 0 : (current - 1) * size + 1
  const to = Math.min(total, current * size)
  return { page: current, setPage, pages, from, to }
}
