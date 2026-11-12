import { Navigate, Outlet, useLocation } from 'react-router'
import { useSignedIn } from '../api/client'

/** Pages behind the sign in rule: a visitor without a session is sent to sign in and comes back here afterwards. */
export function RequireSession() {
  const signedIn = useSignedIn()
  const { pathname, search } = useLocation()
  if (!signedIn) return <Navigate to={`/signin?next=${encodeURIComponent(pathname + search)}`} replace />
  return <Outlet />
}
