import { useQuery } from '@tanstack/react-query'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Info } from 'lucide-react'
import { useState, type FormEvent } from 'react'
import { Link, useNavigate, useSearchParams } from 'react-router'
import { session } from '../api/client'
import { platform } from '../api/platform'
import type { SessionOut } from '../api/platformTypes'
import { Button, ErrorText, Field, Input, PageTitle, Segmented } from '../components/ui'
import { MIN_PASSWORD, canSignUp, validateSignUp } from '../lib/signup'
import { usePageTitle } from '../lib/usePageTitle'

type Mode = 'login' | 'register'

/** Only same site paths, so a crafted link cannot send people elsewhere after signing in. */
export function safeNext(raw: string | null): string {
  return raw && raw.startsWith('/') && !raw.startsWith('//') ? raw : '/events'
}

export function SignIn() {
  usePageTitle('Sign in')
  const [params] = useSearchParams()
  const open = useQuery({ queryKey: ['sign-ups-open'], queryFn: platform.signUpsOpen })
  // Until the answer arrives, assume open: the server refuses a closed sign-up anyway.
  const closed = open.data?.open === false
  const [chosen, setChosen] = useState<Mode>(params.get('mode') === 'register' ? 'register' : 'login')
  const mode: Mode = closed ? 'login' : chosen
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [email, setEmail] = useState('')
  const [show, setShow] = useState(false)
  const navigate = useNavigate()
  const client = useQueryClient()
  const registering = mode === 'register'
  const problems = validateSignUp(username, password)
  const go = useMutation({
    mutationFn: (): Promise<SessionOut> =>
      registering ? platform.register(username.trim(), password, email.trim() || undefined) : platform.playerLogin(username.trim(), password),
    onSuccess: ({ token, user }) => {
      session.set(token, null, { id: user.id, username: user.username, role: user.role === 'visitor' ? 'player' : user.role })
      client.clear()
      navigate(safeNext(params.get('next')))
    },
  })
  const ready = registering ? canSignUp(username, password) : username.trim() !== '' && password !== ''
  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (ready && !go.isPending) go.mutate()
  }
  return (
    <form onSubmit={submit} className="mx-auto flex max-w-sm flex-col gap-5">
      <PageTitle>{registering ? 'Join Kymo' : 'Welcome back'}</PageTitle>
      <Segmented<Mode>
        label="Account"
        value={mode}
        onChange={(m) => {
          go.reset()
          setChosen(m)
        }}
        options={[
          { value: 'login', label: 'Sign in' },
          { value: 'register', label: 'Create account' },
        ]}
      />
      {closed && (
        <p role="status" className="flex items-start gap-2 rounded-control border border-line bg-surface p-3 text-sm">
          <Info size={16} aria-hidden="true" className="mt-0.5 shrink-0 text-accent" />
          <span>
            Creating accounts is switched off on this server. If you already have one, sign in below. Otherwise ask an organizer to make an account for you.
          </span>
        </p>
      )}
      <Field label="Username" error={registering ? problems.username : undefined} hint={registering ? '3 to 32 letters, numbers, _ or -.' : undefined}>
        <Input value={username} onChange={(e) => setUsername(e.target.value)} autoComplete="username" autoFocus maxLength={32} aria-invalid={registering && problems.username ? true : undefined} />
      </Field>
      {registering && (
        <Field label="Email (optional)" hint="Only used by organizers to reach you.">
          <Input type="email" value={email} onChange={(e) => setEmail(e.target.value)} autoComplete="email" />
        </Field>
      )}
      <div className="flex flex-col gap-2">
        <Field
          label={registering ? `Password (at least ${MIN_PASSWORD} characters)` : 'Password'}
          error={registering ? problems.password : undefined}
        >
          <Input
            type={show ? 'text' : 'password'}
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoComplete={registering ? 'new-password' : 'current-password'}
            aria-invalid={registering && problems.password ? true : undefined}
          />
        </Field>
        <label className="flex items-center gap-2 text-xs text-muted">
          <input type="checkbox" checked={show} onChange={(e) => setShow(e.target.checked)} className="size-3.5 accent-[var(--color-accent)]" /> Show the password
        </label>
      </div>
      <ErrorText error={go.error} />
      <Button type="submit" variant="primary" disabled={!ready || go.isPending}>
        {go.isPending ? 'One moment...' : registering ? 'Create account' : 'Sign in'}
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
