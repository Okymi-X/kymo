import { RefreshCw } from 'lucide-react'
import { useCallback, useEffect, useState } from 'react'
import { platform } from '../../api/platform'
import { Modal, ModalBody, ModalFooter, ModalHeader } from '../Modal'
import { Button, ErrorText } from '../ui'

/** What a virtual machine's screen shows right now, for working out why it does not answer. */
export function ConsoleDialog({ labId, name, onClose }: { labId: number; name: string; onClose: () => void }) {
  const [src, setSrc] = useState<string | null>(null)
  const [error, setError] = useState<unknown>(null)
  const [busy, setBusy] = useState(true)
  const load = useCallback(() => {
    setBusy(true)
    platform
      .labScreen(labId, name)
      .then((url) => {
        setSrc((old) => {
          if (old) URL.revokeObjectURL(old)
          return url
        })
        setError(null)
      })
      .catch((e: unknown) => setError(e))
      .finally(() => setBusy(false))
  }, [labId, name])
  useEffect(() => {
    load()
  }, [load])
  useEffect(() => () => setSrc((old) => (old && URL.revokeObjectURL(old), null)), [])
  return (
    <Modal label={`Console of ${name}`} onClose={onClose} width="52rem">
      <ModalHeader title="Console" subtitle={<span className="font-mono text-xs">{name}</span>} />
      <ModalBody>
        <div className="flex min-h-48 items-center justify-center overflow-hidden rounded-control border border-line bg-black">
          {src ? <img src={src} alt={`The screen of ${name}`} className="max-h-[60vh] w-full object-contain" /> : <span className="p-8 text-sm text-muted">{busy ? 'Taking a screenshot…' : 'No picture.'}</span>}
        </div>
        <p className="text-sm text-muted">A snapshot of what the machine shows on its screen. A login prompt means it has booted; a boot menu or an error means it is stuck there.</p>
        <ErrorText error={error} />
      </ModalBody>
      <ModalFooter>
        <Button onClick={load} disabled={busy}>
          <RefreshCw size={15} aria-hidden="true" className={busy ? 'animate-spin' : ''} /> Refresh
        </Button>
        <Button variant="primary" onClick={onClose}>
          Close
        </Button>
      </ModalFooter>
    </Modal>
  )
}
