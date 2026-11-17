import { Navigate, useLocation } from 'react-router'
import { legacyOrganizerPath } from '../../lib/organizer'

/** Old bookmarks and links to the organizer pages land in the console. */
export function LegacyRedirect() {
  const { pathname, search } = useLocation()
  const to = legacyOrganizerPath(pathname)
  return <Navigate to={to ? to + search : '/'} replace />
}
