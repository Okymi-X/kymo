import { useMutation, useQueryClient } from '@tanstack/react-query'
import { CircleAlert, CircleCheck } from 'lucide-react'
import { useState, type FormEvent } from 'react'
import { platform } from '../../api/platform'
import type { LabCheck, LabHost } from '../../api/platformTypes'
import { Modal, ModalBody, ModalCancel, ModalFooter, ModalHeader, modalForm } from '../Modal'
import { Button, ErrorText, Field, Input } from '../ui'

/** The host name part of a URL, to suggest as the public address. */
export function hostOf(url: string): string {
  try {
    return new URL(url.trim()).hostname.replace(/^\[|\]$/g, '')
  } catch {
    return ''
  }
}

function Outcome({ check }: { check: LabCheck }) {
  return check.ok ? (
    <div className="flex flex-col gap-2 rounded-control border border-line p-3 text-sm" role="status">
      <p className="flex items-center gap-2 font-medium text-win">
        <CircleCheck size={16} aria-hidden="true" /> Connected in {check.ms} ms
      </p>
      {check.health && (
        <p className="text-muted">
          {check.health.name}, version {check.health.version}, {check.health.runtime}. Runs {check.health.mode === 'manifest' ? 'only the images it has manifests for' : 'any valid image'}; {check.health.running} of {check.health.max_instances} instances in use.
        </p>
      )}
      {(check.notes ?? []).map((n) => (
        <p key={n} className="text-danger">
          {n}
        </p>
      ))}
    </div>
  ) : (
    <div className="flex flex-col gap-1 rounded-control border border-danger/40 bg-danger/10 p-3 text-sm" role="alert">
      <p className="flex items-center gap-2 font-medium text-danger">
        <CircleAlert size={16} aria-hidden="true" /> It was saved, but the lab host did not answer
      </p>
      <p className="break-words">{check.error}</p>
      <p className="text-muted">Fix the settings below and save again, or use Test connection on the card.</p>
    </div>
  )
}

/** Adds a lab host, or edits one. A saved lab host is tested at once and the answer is shown. */
export function LabDialog({ lab, onClose }: { lab?: LabHost; onClose: () => void }) {
  const client = useQueryClient()
  const [name, setName] = useState(lab?.name ?? '')
  const [url, setUrl] = useState(lab?.api_url ?? '')
  const [token, setToken] = useState('')
  const [pin, setPin] = useState(lab?.fingerprint ?? '')
  const [host, setHost] = useState(lab?.public_host ?? '')
  const [result, setResult] = useState<LabCheck | null>(null)
  const refresh = () => void client.invalidateQueries({ queryKey: ['labs'] })
  const save = useMutation({
    mutationFn: async () => {
      if (lab) {
        await platform.updateLab(lab.id, { name, api_url: url, token: token || undefined, fingerprint: pin, public_host: host })
        return platform.testLab(lab.id)
      }
      return (await platform.createLab({ name, api_url: url, token, fingerprint: pin, public_host: host })).check
    },
    onSuccess: (check) => {
      refresh()
      setResult(check)
    },
  })
  const submit = (e: FormEvent) => {
    e.preventDefault()
    save.mutate()
  }
  const dirty = !result && (name !== (lab?.name ?? '') || url !== (lab?.api_url ?? '') || token !== '' || pin !== (lab?.fingerprint ?? '') || host !== (lab?.public_host ?? ''))
  const ready = name.trim() !== '' && url.trim() !== '' && (lab !== undefined || token.trim() !== '') && host.trim() !== ''

  if (result) {
    return (
      <Modal label="Lab host saved" onClose={onClose} width="32rem">
        <ModalHeader title={lab ? 'Lab host updated' : 'Lab host added'} subtitle={name} />
        <ModalBody>
          <Outcome check={result} />
        </ModalBody>
        <ModalFooter>
          <Button variant="primary" onClick={onClose}>
            Done
          </Button>
        </ModalFooter>
      </Modal>
    )
  }
  return (
    <Modal label={lab ? 'Edit lab host' : 'Add a lab host'} onClose={onClose} width="34rem" dirty={dirty}>
      <form onSubmit={submit} className={modalForm}>
        <ModalHeader title={lab ? 'Edit lab host' : 'Add a lab host'} subtitle="A separate machine that runs challenge containers." />
        <ModalBody>
          <Field label="Name" hint="Shown in the event settings, for example Lab Paris.">
            <Input value={name} onChange={(e) => setName(e.target.value)} autoFocus required maxLength={40} autoComplete="off" />
          </Field>
          <Field label="API address" hint="The lab host's address and port, for example https://203.0.113.7:7400.">
            <Input
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              onBlur={() => !host && setHost(hostOf(url))}
              required
              inputMode="url"
              autoComplete="off"
              placeholder="https://"
              className="font-mono"
            />
          </Field>
          <Field label="Token" hint={lab ? 'Leave empty to keep the saved token. It is never shown again.' : 'The secret printed by the installer, kept in the lab host settings file.'}>
            <Input type="password" value={token} onChange={(e) => setToken(e.target.value)} required={!lab} autoComplete="new-password" className="font-mono" />
          </Field>
          <Field label="Certificate fingerprint" hint='Printed by the installer, or by "kymo-lab fingerprint" on the lab host. Empty only for a certificate from a certificate authority, or plain http on a trusted network.'>
            <Input value={pin} onChange={(e) => setPin(e.target.value)} autoComplete="off" spellCheck={false} placeholder="AB:CD:EF:…" className="font-mono text-xs" />
          </Field>
          <Field label="Public address" hint="What players connect to for their instances: the lab host's public name or IP address. Its instance ports must be reachable from the internet.">
            <Input value={host} onChange={(e) => setHost(e.target.value)} required autoComplete="off" placeholder="203.0.113.7" className="font-mono" />
          </Field>
          {url.trim().startsWith('http://') && <p className="text-sm text-danger">This connection is not encrypted: the token and instance details cross the network in clear text. Use it only on a private network.</p>}
          <ErrorText error={save.error} />
        </ModalBody>
        <ModalFooter>
          <ModalCancel />
          <Button type="submit" variant="primary" disabled={!ready || save.isPending}>
            {save.isPending ? 'Connecting…' : lab ? 'Save and test' : 'Add and test'}
          </Button>
        </ModalFooter>
      </form>
    </Modal>
  )
}
