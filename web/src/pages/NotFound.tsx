import { Link, useLocation } from 'react-router'
import { PageTitle } from '../components/ui'
import { usePageTitle } from '../lib/usePageTitle'

/** Where "back" should lead from an address that does not exist, judged by what it started like. */
export function notFoundBack(pathname: string): { to: string; label: string } {
  if (pathname.startsWith('/e/') || pathname.startsWith('/events')) return { to: '/events', label: 'Browse events' }
  if (pathname.startsWith('/t/')) return { to: '/', label: 'Browse tournaments' }
  if (pathname.startsWith('/admin') || pathname.startsWith('/monitor')) return { to: '/admin/events', label: 'Go to administration' }
  return { to: '/events', label: 'Browse events' }
}

export function NotFound() {
  usePageTitle('Page not found')
  const { pathname } = useLocation()
  const back = notFoundBack(pathname)
  return (
    <div className="flex flex-col items-start gap-4 py-16">
      <p className="font-mono text-sm text-muted">404</p>
      <PageTitle>Page not found</PageTitle>
      <p className="max-w-prose text-muted">That address does not lead anywhere. The page may have been moved, renamed or removed, or the link may be mistyped.</p>
      <div className="flex flex-wrap gap-x-6 gap-y-2">
        <Link to="/" className="font-medium text-accent hover:underline">
          Go to the home page
        </Link>
        <Link to={back.to} className="font-medium text-accent hover:underline">
          {back.label}
        </Link>
      </div>
    </div>
  )
}
