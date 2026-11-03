import { Modal, ModalBody, ModalFooter, ModalHeader } from './Modal'
import { Button } from './ui'

interface Props {
  title: string
  message: string
  confirmLabel: string
  cancelLabel?: string
  danger?: boolean
  pending?: boolean
  onConfirm: () => void
  onClose: () => void
}

/** A small question. For a risky action the safe answer has focus, so Enter never destroys anything. */
export function ConfirmDialog({ title, message, confirmLabel, cancelLabel = 'Cancel', danger, pending, onConfirm, onClose }: Props) {
  return (
    <Modal label={title} onClose={onClose} width="24rem">
      <ModalHeader title={title} />
      <ModalBody>
        <p className="text-muted">{message}</p>
      </ModalBody>
      <ModalFooter>
        <Button variant="ghost" onClick={onClose} data-autofocus={danger ? '' : undefined}>
          {cancelLabel}
        </Button>
        <Button variant={danger ? 'danger' : 'primary'} onClick={onConfirm} disabled={pending} data-autofocus={danger ? undefined : ''}>
          {confirmLabel}
        </Button>
      </ModalFooter>
    </Modal>
  )
}
