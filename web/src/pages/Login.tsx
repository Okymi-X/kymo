import { usePageTitle } from '../lib/usePageTitle'
import { useState, type FormEvent } from 'react'
import { useMutation } from '@tanstack/react-query'
import { useNavigate } from 'react-router'
import { api, session } from '../api/client'
import { BackLink } from '../components/BackLink'
import { Button, ErrorText, Field, Input, PageTitle } from '../components/ui'

export function Login() {
  usePageTitle('Organizer sign in')
  const [password, setPassword] = useState('')
  const navigate = useNavigate()
  const login = useMutation({
    mutationFn: () => api.login(password),
    onSuccess: ({ token }) => {
      session.set(token)
      navigate('/')
    },
  })
  const [refereeToken, setRefereeToken] = useState('')
  const referee = useMutation({
    mutationFn: async () => {
      session.set(refereeToken.trim(), [])
      try {
        const me = await api.me()
        if (me.role !== 'referee') throw new Error('That is not a referee token.')
        session.set(refereeToken.trim(), me.tournaments ?? [])
      } catch (e) {
        session.clear()
        throw e
      }
    },
    onSuccess: () => navigate('/'),
  })
  const submit = (e: FormEvent) => {
    e.preventDefault()
    login.mutate()
  }
  return (
    <form onSubmit={submit} className="mx-auto flex max-w-sm flex-col gap-5">
      <BackLink>Tournaments</BackLink>
      <PageTitle>Organizer sign in</PageTitle>
      <Field label="Password">
        <Input type="password" autoComplete="current-password" value={password} onChange={(e) => setPassword(e.target.value)} autoFocus />
      </Field>
      <ErrorText error={login.error} />
      <Button type="submit" variant="primary" disabled={!password || login.isPending}>
        Sign in
      </Button>
      <details className="rounded-card border border-line p-4">
        <summary className="cursor-pointer select-none text-sm font-medium">I am a referee with a token</summary>
        <div className="mt-4 flex flex-col gap-4">
          <p className="text-sm text-muted">A referee can enter results and schedules for the tournaments the organizer chose, and nothing else.</p>
          <Field label="Referee token">
            <Input type="password" autoComplete="off" value={refereeToken} onChange={(e) => setRefereeToken(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault()
                  if (refereeToken.trim()) referee.mutate()
                }
              }}
            />
          </Field>
          <ErrorText error={referee.error} />
          <Button type="button" onClick={() => referee.mutate()} disabled={!refereeToken.trim() || referee.isPending}>
            Sign in as referee
          </Button>
        </div>
      </details>
    </form>
  )
}
