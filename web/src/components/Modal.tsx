import { useEffect, useRef, type ReactNode } from 'react'

/** Native dialog opened on mount; closes on backdrop click and Escape. */
export function Modal({ label, onClose, children, width = '28rem' }: { label: string; onClose: () => void; children: ReactNode; width?: string }) {
  const ref = useRef<HTMLDialogElement>(null)
  useEffect(() => {
    ref.current?.showModal()
  }, [])
  return (
    <dialog
      ref={ref}
      aria-label={label}
      onClose={onClose}
      onClick={(e) => e.target === ref.current && onClose()}
      style={{ width: `min(92vw, ${width})` }}
      className="m-auto rounded-card border border-line bg-surface p-0 text-ink"
    >
      {children}
    </dialog>
  )
}
