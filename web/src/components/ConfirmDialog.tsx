import { useEffect, useRef } from 'react'
import { Button } from './ui'

interface Props {
  title: string
  message: string
  confirmLabel: string
  danger?: boolean
  pending?: boolean
  onConfirm: () => void
  onClose: () => void
}

export function ConfirmDialog({ title, message, confirmLabel, danger, pending, onConfirm, onClose }: Props) {
  const ref = useRef<HTMLDialogElement>(null)
  useEffect(() => {
    ref.current?.showModal()
  }, [])
  return (
    <dialog
      ref={ref}
      onClose={onClose}
      onClick={(e) => e.target === ref.current && onClose()}
      className="m-auto w-[min(92vw,24rem)] rounded-card border border-line bg-surface p-0 text-ink"
    >
      <div className="flex flex-col gap-4 p-6">
        <h2 className="text-lg font-semibold tracking-tight">{title}</h2>
        <p className="text-muted">{message}</p>
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button variant={danger ? 'danger' : 'primary'} onClick={onConfirm} disabled={pending}>
            {confirmLabel}
          </Button>
        </div>
      </div>
    </dialog>
  )
}
