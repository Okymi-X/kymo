import { Navigate, Outlet, useLocation } from 'react-router'
import { useMyPermissions } from '../../api/platformHooks'
import { session, useSignedIn } from '../../api/client'
import { Loading, Panel } from '../platform/bits'
import { ORG, isStaffRole } from '../../lib/organizer'

/** The door of the console: no session goes to the organizer sign in, and a player's session
 *  gets told so instead of being sent in circles. The server checks every request anyway. */
export function RequireStaff() {
  const signedIn = useSignedIn()
  const { pathname, search } = useLocation()
  const mine = useMyPermissions()
  if (!signedIn) return <Navigate to={`${ORG}/login?next=${encodeURIComponent(pathname + search)}`} replace />
  if (mine.isPending) return <Loading />
  if (!mine.data || !isStaffRole(mine.data.role)) {
    return (
      <Panel className="mx-auto mt-16 max-w-md text-center">
        <h1 className="text-lg font-semibold">This is not an organizer account</h1>
        <p className="mt-2 text-sm text-muted">The organizer console is for organizers, monitors and referees. You are signed in as a player.</p>
        <div className="mt-4 flex justify-center gap-4 text-sm">
          <a href="/events" className="font-medium text-accent hover:underline">
            Back to the events
          </a>
          <button
            type="button"
            className="font-medium text-accent hover:underline"
            onClick={() => {
              session.clear()
              window.location.assign(`${ORG}/login`)
            }}
          >
            Sign in as an organizer
          </button>
        </div>
      </Panel>
    )
  }
  return <Outlet />
}
