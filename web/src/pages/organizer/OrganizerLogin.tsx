import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useState, type FormEvent } from 'react'
import { Link, Navigate, useNavigate, useSearchParams } from 'react-router'
import { api, session, useSignedIn } from '../../api/client'
import { platform } from '../../api/platform'
import { Button, ErrorText, Field, Input, PageTitle, Segmented } from '../../components/ui'
import { useAppearance } from '../../components/ThemeLoader'
import { safeOrganizerNext } from '../../lib/organizer'
import { usePageTitle } from '../../lib/usePageTitle'

type Way = 'account' | 'password' | 'referee'

/** The organizers' own sign in. A player account is refused here with the same words as a wrong
 *  password, so this page cannot be used to find out who is staff. */
export function OrganizerLogin() {
  usePageTitle('Organizer sign in')
  const [params] = useSearchParams()
  const navigate = useNavigate()
  const client = useQueryClient()
  const signedIn = useSignedIn()
  const site = useAppearance().data?.site_name ?? 'Kymo'
  const [way, setWay] = useState<Way>('account')
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [token, setToken] = useState('')
  const next = safeOrganizerNext(params.get('next'))
  const enter = useMutation({
    mutationFn: async () => {
      if (way === 'referee') {
        // A referee token has no account: it covers some tournaments and nothing else.
        session.set(token.trim(), [])
        try {
          const me = await api.me()
          if (me.role !== 'referee') throw new Error('That is not a referee token.')
          session.set(token.trim(), me.tournaments ?? [])
        } catch (e) {
          session.clear()
          throw e
        }
        return
      }
      const out = await platform.organizerLogin(way === 'account' ? username.trim() : '', password)
      client.clear()
      session.set(out.token, null, out.user_id !== null && out.username !== null ? { id: out.user_id, username: out.username, role: out.role } : null)
    },
    onSuccess: () => navigate(way === 'referee' ? '/events' : next, { replace: true }),
  })
  if (signedIn && !enter.isPending && !enter.isSuccess) return <Navigate to={next} replace />
  const ready = way === 'referee' ? token.trim() !== '' : password !== '' && (way === 'password' || username.trim() !== '')
  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (ready && !enter.isPending) enter.mutate()
  }
  return (
    <div className="mx-auto flex min-h-screen max-w-sm flex-col justify-center gap-6 px-4 py-10">
      <div>
        <p className="text-sm font-medium text-muted">{site}</p>
        <PageTitle>Organizer sign in</PageTitle>
      </div>
      <form onSubmit={submit} className="flex flex-col gap-5">
        <Segmented<Way>
          label="Sign in with"
          value={way}
          onChange={(w) => {
            enter.reset()
            setWay(w)
          }}
          options={[
            { value: 'account', label: 'Account' },
            { value: 'password', label: 'Password' },
            { value: 'referee', label: 'Referee' },
          ]}
        />
        {way === 'account' && (
          <Field label="Username">
            <Input value={username} onChange={(e) => setUsername(e.target.value)} autoComplete="username" autoFocus />
          </Field>
        )}
        {way !== 'referee' ? (
          <Field label={way === 'password' ? 'Organizer password' : 'Password'} hint={way === 'password' ? 'The password the server was started with.' : undefined}>
            <Input type="password" value={password} onChange={(e) => setPassword(e.target.value)} autoComplete="current-password" autoFocus={way === 'password'} />
          </Field>
        ) : (
          <Field label="Referee token" hint="A referee can enter results and schedules for the tournaments the organizer chose, and nothing else.">
            <Input type="password" value={token} onChange={(e) => setToken(e.target.value)} autoComplete="off" autoFocus />
          </Field>
        )}
        <ErrorText error={enter.error} />
        <Button type="submit" variant="primary" disabled={!ready || enter.isPending}>
          Sign in
        </Button>
      </form>
      <p className="text-sm text-muted">
        Playing instead? <Link to="/signin" className="font-medium text-accent hover:underline">Player sign in</Link>
      </p>
    </div>
  )
}
