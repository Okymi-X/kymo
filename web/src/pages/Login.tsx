import { useState, type FormEvent } from 'react'
import { useMutation } from '@tanstack/react-query'
import { useNavigate } from 'react-router'
import { api, session } from '../api/client'
import { Button, ErrorText, Field, Input } from '../components/ui'

export function Login() {
  const [password, setPassword] = useState('')
  const navigate = useNavigate()
  const login = useMutation({
    mutationFn: () => api.login(password),
    onSuccess: ({ token }) => {
      session.set(token)
      navigate('/')
    },
  })
  const submit = (e: FormEvent) => {
    e.preventDefault()
    login.mutate()
  }
  return (
    <form onSubmit={submit} className="mx-auto flex max-w-sm flex-col gap-5">
      <h1 className="text-3xl font-semibold tracking-tight">Organizer sign in</h1>
      <Field label="Password">
        <Input type="password" autoComplete="current-password" value={password} onChange={(e) => setPassword(e.target.value)} autoFocus />
      </Field>
      <ErrorText error={login.error} />
      <Button type="submit" variant="primary" disabled={!password || login.isPending}>
        Sign in
      </Button>
    </form>
  )
}
