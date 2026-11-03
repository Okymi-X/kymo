import { X } from 'lucide-react'
import { createContext, useContext, useEffect, useId, useRef, useState, type ReactNode } from 'react'
import { ConfirmDialog } from './ConfirmDialog'

interface ModalContextValue {
  /** Closes the dialog, asking first when there are unsaved changes. */
  requestClose: () => void
  titleId: string
}

const ModalContext = createContext<ModalContextValue>({ requestClose: () => undefined, titleId: '' })

/** Nested dialogs share one lock, so the page only scrolls again when the last one closes. */
let openCount = 0
function lockScroll() {
  openCount += 1
  document.documentElement.classList.add('modal-open')
}
function unlockScroll() {
  openCount = Math.max(0, openCount - 1)
  if (openCount === 0) document.documentElement.classList.remove('modal-open')
}

interface ModalProps {
  label: string
  onClose: () => void
  children: ReactNode
  width?: string
  /** Unsaved changes: Escape, the close button and the backdrop ask before discarding them. */
  dirty?: boolean
  /** A click on the backdrop closes the dialog. Forms turn this off so a stray click loses nothing. */
  backdropCloses?: boolean
}

/**
 * A native dialog with one scrolling area. Build the content from ModalHeader, ModalBody and
 * ModalFooter: the header and footer stay in view and only the body scrolls.
 */
export function Modal({ label, onClose, children, width = '28rem', dirty = false, backdropCloses = true }: ModalProps) {
  const ref = useRef<HTMLDialogElement>(null)
  const titleId = useId()
  const [confirming, setConfirming] = useState(false)
  const pressedBackdrop = useRef(false)

  useEffect(() => {
    const dialog = ref.current
    if (!dialog) return
    dialog.showModal()
    lockScroll()
    // Focus the first field, so typing starts at once; otherwise leave the browser's choice.
    const first = dialog.querySelector<HTMLElement>('[data-autofocus], [autofocus], [data-modal-body] :is(input, textarea, select):not([type="hidden"]):not([disabled])')
    first?.focus()
    return unlockScroll
  }, [])

  const requestClose = () => (dirty ? setConfirming(true) : onClose())

  return (
    <ModalContext.Provider value={{ requestClose, titleId }}>
      <dialog
        ref={ref}
        aria-label={label}
        onCancel={(e) => {
          // Escape: stay in control, so unsaved work is never lost silently.
          e.preventDefault()
          requestClose()
        }}
        onPointerDown={(e) => {
          pressedBackdrop.current = e.target === ref.current
        }}
        onClick={(e) => {
          // Both the press and the release must be on the backdrop: selecting text and letting go outside does nothing.
          if (backdropCloses && pressedBackdrop.current && e.target === ref.current) requestClose()
          pressedBackdrop.current = false
        }}
        onKeyDown={(e) => {
          if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
            const form = ref.current?.querySelector('form')
            if (form) {
              e.preventDefault()
              form.requestSubmit()
            }
          }
        }}
        style={{ width: `min(calc(100vw - 1.5rem), ${width})` }}
        className="m-auto max-h-[92dvh] overflow-hidden rounded-card border border-line bg-surface p-0 text-ink"
      >
        <div className="flex max-h-[92dvh] min-h-0 flex-col">{children}</div>
      </dialog>
      {confirming && (
        <ConfirmDialog
          title="Discard your changes?"
          message="This form has changes that are not saved yet."
          confirmLabel="Discard"
          cancelLabel="Keep editing"
          danger
          onConfirm={onClose}
          onClose={() => setConfirming(false)}
        />
      )}
    </ModalContext.Provider>
  )
}

interface HeaderProps {
  title: ReactNode
  subtitle?: ReactNode
  /** Controls shown next to the close button, for example a status and a publish button. */
  actions?: ReactNode
}

/** Title, optional subtitle and actions, and the close button. Always visible. */
export function ModalHeader({ title, subtitle, actions }: HeaderProps) {
  const { requestClose, titleId } = useContext(ModalContext)
  return (
    <header className="flex flex-wrap items-start gap-x-3 gap-y-2 border-b border-line px-6 py-4">
      <div className="min-w-0 flex-1 basis-32">
        <h2 id={titleId} className="text-lg font-semibold leading-tight tracking-tight">
          {title}
        </h2>
        {subtitle && <div className="mt-1 text-sm text-muted">{subtitle}</div>}
      </div>
      {/* Beside the title on a wide screen; on a phone they drop under it, so the title keeps its width. */}
      {actions && <div className="order-3 flex w-full flex-wrap items-center gap-2 sm:order-2 sm:w-auto sm:shrink-0 sm:flex-nowrap">{actions}</div>}
      <button
        type="button"
        aria-label="Close"
        onClick={requestClose}
        className="order-2 -mr-2 -mt-1 flex size-9 shrink-0 items-center justify-center rounded-control text-muted transition-colors hover:bg-line/60 hover:text-ink sm:order-3"
      >
        <X size={18} aria-hidden="true" />
      </button>
    </header>
  )
}

/** A Cancel button that goes through the same guard as Escape, so unsaved changes are never lost silently. */
export function ModalCancel({ children = 'Cancel' }: { children?: ReactNode }) {
  const { requestClose } = useContext(ModalContext)
  return (
    <button
      type="button"
      onClick={requestClose}
      className="inline-flex h-10 items-center justify-center gap-2 rounded-control border border-transparent bg-transparent px-3.5 font-medium text-muted transition-colors hover:text-ink"
    >
      {children}
    </button>
  )
}

/** The only part that scrolls. */
export function ModalBody({ children, className = '' }: { children: ReactNode; className?: string }) {
  return (
    <div data-modal-body className={`flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto overscroll-contain px-6 py-5 ${className}`}>
      {children}
    </div>
  )
}

/** Actions, always visible. `note` carries status text such as unsaved changes: beside the
 *  buttons on a wide screen, above them on a phone so a long message never squeezes them. */
export function ModalFooter({ children, note }: { children: ReactNode; note?: ReactNode }) {
  return (
    <footer className="flex flex-col gap-2 border-t border-line bg-surface px-6 py-3.5 sm:flex-row sm:items-center sm:justify-between sm:gap-3">
      {note ? (
        <div className="min-w-0 text-sm text-muted sm:flex-1" aria-live="polite">
          {note}
        </div>
      ) : (
        <div className="hidden sm:block sm:flex-1" />
      )}
      <div className="flex shrink-0 items-center justify-end gap-2">{children}</div>
    </footer>
  )
}

/** Class for a `<form>` that wraps header, body and footer, so the body still scrolls. */
export const modalForm = 'flex min-h-0 flex-1 flex-col'
