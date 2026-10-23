import { ChevronLeft, ChevronRight } from 'lucide-react'
import { pageWindow } from '../lib/paginate'

interface Props {
  page: number
  pages: number
  from: number
  to: number
  total: number
  onChange: (page: number) => void
  noun?: string
}

const step = 'flex h-9 min-w-9 items-center justify-center rounded-control px-2 text-sm font-medium transition-colors disabled:opacity-40'

/** Previous, numbered pages and next, with a "21 to 40 of 100" summary. Renders nothing for one page. */
export function Pager({ page, pages, from, to, total, onChange, noun = 'results' }: Props) {
  if (pages <= 1) return null
  return (
    <nav aria-label="Pagination" className="no-print flex flex-wrap items-center justify-between gap-3 pt-2">
      <p className="text-sm text-muted" aria-live="polite">
        <span className="font-mono tabular-nums">
          {from} to {to}
        </span>{' '}
        of <span className="font-mono tabular-nums">{total}</span> {noun}
      </p>
      <div className="flex items-center gap-1">
        <button type="button" className={`${step} text-muted hover:bg-line/50 hover:text-ink`} onClick={() => onChange(page - 1)} disabled={page <= 1} aria-label="Previous page">
          <ChevronLeft size={16} aria-hidden="true" />
        </button>
        {pageWindow(page, pages).map((p, i) =>
          p === 'gap' ? (
            <span key={`gap-${i}`} className="px-1 text-muted" aria-hidden="true">
              …
            </span>
          ) : (
            <button
              key={p}
              type="button"
              onClick={() => onChange(p)}
              aria-label={`Page ${p}`}
              aria-current={p === page ? 'page' : undefined}
              className={`${step} font-mono tabular-nums ${p === page ? 'bg-ink text-bg' : 'text-muted hover:bg-line/50 hover:text-ink'}`}
            >
              {p}
            </button>
          ),
        )}
        <button type="button" className={`${step} text-muted hover:bg-line/50 hover:text-ink`} onClick={() => onChange(page + 1)} disabled={page >= pages} aria-label="Next page">
          <ChevronRight size={16} aria-hidden="true" />
        </button>
      </div>
    </nav>
  )
}
