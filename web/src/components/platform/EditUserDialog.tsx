import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { platform } from '../../api/platform'
import type { User } from '../../api/platformTypes'
import { Modal, ModalBody, ModalCancel, ModalFooter, ModalHeader, modalForm } from '../Modal'
import { Button, ErrorText, Field, Input, Textarea } from '../ui'

/** An organizer edits someone's profile: name, bio, email, and a new password if they are locked out. */
export function EditUserDialog({ user, onClose }: { user: User; onClose: () => void }) {
  const client = useQueryClient()
  const [displayName, setDisplayName] = useState(user.display_name)
  const [bio, setBio] = useState(user.bio)
  const [email, setEmail] = useState(user.email ?? '')
  const [password, setPassword] = useState('')
  const dirty = displayName !== user.display_name || bio !== user.bio || email !== (user.email ?? '') || password !== ''
  const save = useMutation({
    mutationFn: () =>
      platform.updateUser(user.id, {
        ...(displayName !== user.display_name ? { display_name: displayName } : {}),
        ...(bio !== user.bio ? { bio } : {}),
        ...(email !== (user.email ?? '') ? { email } : {}),
        ...(password !== '' ? { new_password: password } : {}),
      }),
    onSuccess: async () => {
      await client.invalidateQueries({ queryKey: ['users'] })
      onClose()
    },
  })
  return (
    <Modal label={`Edit ${user.username}`} onClose={onClose} dirty={dirty} backdropCloses={false}>
      <form
        className={modalForm}
        onSubmit={(e) => {
          e.preventDefault()
          if (dirty) save.mutate()
        }}
      >
        <ModalHeader title={`Edit @${user.username}`} subtitle="Changes are written to the audit log." />
        <ModalBody>
          <Field label="Display name">
            <Input value={displayName} onChange={(e) => setDisplayName(e.target.value)} maxLength={40} />
          </Field>
          <Field label="Bio">
            <Textarea value={bio} onChange={(e) => setBio(e.target.value)} rows={3} maxLength={500} />
          </Field>
          <Field label="Email" hint="Leave empty to remove it.">
            <Input type="email" value={email} onChange={(e) => setEmail(e.target.value)} autoComplete="off" />
          </Field>
          <Field label="New password" hint="Sets the password and signs the account out everywhere. Tell them the new one yourself; it is not shown again.">
            <Input type="password" value={password} onChange={(e) => setPassword(e.target.value)} autoComplete="new-password" />
          </Field>
          <ErrorText error={save.error} />
        </ModalBody>
        <ModalFooter>
          <ModalCancel />
          <Button type="submit" variant="primary" disabled={!dirty || save.isPending}>
            Save changes
          </Button>
        </ModalFooter>
      </form>
    </Modal>
  )
}
