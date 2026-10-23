import { Link } from 'react-router'
import { PageTitle } from '../components/ui'

export function NotFound() {
  return (
    <div className="flex flex-col items-start gap-4 py-16">
      <p className="font-mono text-sm text-muted">404</p>
      <PageTitle>Page not found</PageTitle>
      <p className="max-w-prose text-muted">That address does not lead anywhere. The tournament may have been renamed or removed.</p>
      <Link to="/" className="font-medium text-accent hover:underline">
        Back to tournaments
      </Link>
    </div>
  )
}
