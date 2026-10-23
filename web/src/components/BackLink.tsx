import { ChevronLeft } from 'lucide-react'
import type { ReactNode } from 'react'
import { Link } from 'react-router'

/** A visible way back to the parent page, shown above the title of every sub page. */
export function BackLink({ to = '/', children }: { to?: string; children: ReactNode }) {
  return (
    <Link
      to={to}
      className="no-print -ml-1.5 inline-flex w-fit items-center gap-0.5 rounded-control py-1 pl-1 pr-2 text-sm text-muted transition-colors hover:bg-line/50 hover:text-ink"
    >
      <ChevronLeft size={16} aria-hidden="true" />
      {children}
    </Link>
  )
}
