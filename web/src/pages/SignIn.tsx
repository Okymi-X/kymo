import { useState, type FormEvent } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Link, useNavigate, useSearchParams } from 'react-router'
import { session } from '../api/client'
import { platform } from '../api/platform'
import type { SessionOut } from '../api/platformTypes'
import { Button, ErrorText, Field, Input, PageTitle, Segmented } from '../components/ui'
import { usePageTitle } from '../lib/usePageTitle'

type Mode = 'login' | 'register'

/** Only same site paths, so a crafted link cannot send people elsewhere after signing in. */
export function safeNext(raw: string | null): string {
  return raw && raw.startsWith('/') && !raw.startsWith('//') ? raw : '/events'
}

export function SignIn() {
  usePageTitle('Sign in')
  const [params] = useSearchParams()
  const [mode, setMode] = useState<Mode>(params.get('mode') === 'register' ? 'register' : 'login')
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [email, setEmail] = useState('')
  const navigate = useNavigate()
  const client = useQueryClient()
  const go = useMutation({
    mutationFn: (): Promise<SessionOut> =>
      mode === 'login' ? platform.playerLogin(username.trim(), password) : platform.register(username.trim(), password, email.trim() || undefined),
    onSuccess: ({ token, user }) => {
      session.set(token, null, { id: user.id, username: user.username, role: user.role === 'visitor' ? 'player' : user.role })
      client.clear()
      navigate(safeNext(params.get('next')))
    },
  })
  const submit = (e: FormEvent) => {
    e.preventDefault()
    go.mutate()
  }
  const registering = mode === 'register'
  return (
    <form onSubmit={submit} className="mx-auto flex max-w-sm flex-col gap-5">
      <PageTitle>{registering ? 'Create account' : 'Sign in'}</PageTitle>
      <Segmented<Mode>
        label="Account"
        value={mode}
        onChange={setMode}
        options={[
          { value: 'login', label: 'Sign in' },
          { value: 'register', label: 'Create account' },
        ]}
      />
      <Field label="Username">
        <Input value={username} onChange={(e) => setUsername(e.target.value)} autoComplete="username" autoFocus maxLength={32} />
      </Field>
      {registering && (
        <Field label="Email (optional)">
          <Input type="email" value={email} onChange={(e) => setEmail(e.target.value)} autoComplete="email" />
        </Field>
      )}
      <Field label={registering ? 'Password (at least 10 characters)' : 'Password'}>
        <Input type="password" value={password} onChange={(e) => setPassword(e.target.value)} autoComplete={registering ? 'new-password' : 'current-password'} />
      </Field>
      <ErrorText error={go.error} />
      <Button type="submit" variant="primary" disabled={!username.trim() || !password || go.isPending}>
        {registering ? 'Create account' : 'Sign in'}
      </Button>
      <p className="text-sm text-muted">
        Running the event?{' '}
        <Link to="/login" className="font-medium text-accent hover:underline">
          Organizer sign in
        </Link>
      </p>
    </form>
  )
}
