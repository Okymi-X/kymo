import { useEffect, useState, type FormEvent } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useMe } from '../api/platformHooks'
import { platform } from '../api/platform'
import { Gate, Loading, Panel } from '../components/platform/bits'
import { Button, ErrorText, Field, Input, PageTitle, Textarea } from '../components/ui'
import { usePageTitle } from '../lib/usePageTitle'

function ProfileForm() {
  const me = useMe().data
  const client = useQueryClient()
  const [displayName, setDisplayName] = useState('')
  const [bio, setBio] = useState('')
  useEffect(() => {
    if (me) {
      setDisplayName(me.display_name)
      setBio(me.bio)
    }
  }, [me])
  const save = useMutation({
    mutationFn: () => platform.updateMe({ display_name: displayName, bio }),
    onSuccess: () => void client.invalidateQueries({ queryKey: ['me'] }),
  })
  const [current, setCurrent] = useState('')
  const [next, setNext] = useState('')
  const change = useMutation({
    mutationFn: () => platform.updateMe({ current_password: current, new_password: next }),
    onSuccess: () => {
      setCurrent('')
      setNext('')
    },
  })
  if (!me) return <Loading />
  const onSave = (e: FormEvent) => {
    e.preventDefault()
    save.mutate()
  }
  const onChange = (e: FormEvent) => {
    e.preventDefault()
    change.mutate()
  }
  return (
    <div className="flex max-w-xl flex-col gap-6">
      <Panel>
        <form onSubmit={onSave} className="flex flex-col gap-4">
          <h2 className="text-lg font-semibold tracking-tight">Your details</h2>
          <p className="text-sm text-muted">
            Signed in as <span className="font-mono">{me.username}</span>
            {me.email ? `, ${me.email}` : ''}
          </p>
          <Field label="Display name">
            <Input value={displayName} onChange={(e) => setDisplayName(e.target.value)} maxLength={60} />
          </Field>
          <Field label="About you">
            <Textarea value={bio} onChange={(e) => setBio(e.target.value)} maxLength={500} />
          </Field>
          <ErrorText error={save.error} />
          <div className="flex items-center gap-3">
            <Button type="submit" variant="primary" disabled={save.isPending}>
              Save
            </Button>
            {save.isSuccess && <span className="text-sm text-win">Saved</span>}
          </div>
        </form>
      </Panel>
      <Panel>
        <form onSubmit={onChange} className="flex flex-col gap-4">
          <h2 className="text-lg font-semibold tracking-tight">Change password</h2>
          <Field label="Current password">
            <Input type="password" value={current} onChange={(e) => setCurrent(e.target.value)} autoComplete="current-password" />
          </Field>
          <Field label="New password (at least 10 characters)">
            <Input type="password" value={next} onChange={(e) => setNext(e.target.value)} autoComplete="new-password" />
          </Field>
          <p className="text-sm text-muted">Changing it signs you out everywhere else.</p>
          <ErrorText error={change.error} />
          <div className="flex items-center gap-3">
            <Button type="submit" disabled={!current || next.length < 10 || change.isPending}>
              Change password
            </Button>
            {change.isSuccess && <span className="text-sm text-win">Password changed</span>}
          </div>
        </form>
      </Panel>
    </div>
  )
}

export function Profile() {
  usePageTitle('Profile')
  return (
    <div className="flex flex-col gap-6">
      <PageTitle>Profile</PageTitle>
      <Gate need="player">
        <ProfileForm />
      </Gate>
    </div>
  )
}
