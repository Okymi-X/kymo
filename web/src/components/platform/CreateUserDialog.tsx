import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Check, Copy } from 'lucide-react'
import { useState, type FormEvent } from 'react'
import { platform } from '../../api/platform'
import type { Role } from '../../api/platformTypes'
import { Modal, ModalBody, ModalCancel, ModalFooter, ModalHeader, modalForm } from '../Modal'
import { Button, ErrorText, Field, Input, Select } from '../ui'

const ROLES: Role[] = ['player', 'referee', 'monitor', 'admin']

/** Makes one account without opening public sign-ups. The password is shown once. */
export function CreateUserDialog({ onClose }: { onClose: () => void }) {
  const client = useQueryClient()
  const [username, setUsername] = useState('')
  const [email, setEmail] = useState('')
  const [role, setRole] = useState<Role>('player')
  const [password, setPassword] = useState('')
  const [copied, setCopied] = useState(false)
  const create = useMutation({
    mutationFn: () =>
      platform.createUser({
        username: username.trim(),
        email: email.trim() || undefined,
        role,
        password: password || undefined,
      }),
    onSuccess: () => void client.invalidateQueries({ queryKey: ['users'] }),
  })
  const submit = (e: FormEvent) => {
    e.preventDefault()
    create.mutate()
  }
  const made = create.data

  if (made) {
    const shown = made.temporary_password
    return (
      <Modal label="Account created" onClose={onClose} width="30rem">
        <ModalHeader title="Account created" subtitle={`@${made.user.username} is a ${made.user.role}.`} />
        <ModalBody>
          {shown ? (
            <>
              <p className="text-sm">Give this temporary password to the player. It is shown only now, so copy it before closing. They can change it from their profile after signing in.</p>
              <div className="flex items-center gap-2 rounded-control border border-line px-3 py-2">
                <code className="min-w-0 flex-1 break-all font-mono text-base" data-testid="temporary-password">
                  {shown}
                </code>
                <Button
                  onClick={() => {
                    void navigator.clipboard?.writeText(shown).then(() => setCopied(true))
                  }}
                >
                  {copied ? <Check size={15} aria-hidden="true" /> : <Copy size={15} aria-hidden="true" />} {copied ? 'Copied' : 'Copy'}
                </Button>
              </div>
            </>
          ) : (
            <p className="text-sm">The account uses the password you entered. Tell the player their username and that password.</p>
          )}
        </ModalBody>
        <ModalFooter>
          <Button variant="primary" onClick={onClose}>
            Done
          </Button>
        </ModalFooter>
      </Modal>
    )
  }

  const dirty = username !== '' || email !== '' || password !== ''
  return (
    <Modal label="Create an account" onClose={onClose} width="30rem" dirty={dirty}>
      <form onSubmit={submit} className={modalForm}>
        <ModalHeader title="Create an account" subtitle="Public sign-ups can stay closed." />
        <ModalBody>
          <Field label="Username" hint="3 to 32 characters: letters, digits, _ and -.">
            <Input value={username} onChange={(e) => setUsername(e.target.value)} autoFocus required maxLength={32} autoComplete="off" />
          </Field>
          <Field label="Email (optional)">
            <Input type="email" value={email} onChange={(e) => setEmail(e.target.value)} autoComplete="off" />
          </Field>
          <Field label="Role">
            <Select value={role} onChange={(e) => setRole(e.target.value as Role)}>
              {ROLES.map((r) => (
                <option key={r} value={r}>
                  {r}
                </option>
              ))}
            </Select>
          </Field>
          <Field label="Password (optional)" hint="Leave empty and a temporary password is made for you to hand over.">
            <Input type="text" value={password} onChange={(e) => setPassword(e.target.value)} autoComplete="off" className="font-mono" />
          </Field>
          <ErrorText error={create.error} />
        </ModalBody>
        <ModalFooter>
          <ModalCancel />
          <Button type="submit" variant="primary" disabled={create.isPending || username.trim() === ''}>
            Create account
          </Button>
        </ModalFooter>
      </form>
    </Modal>
  )
}
